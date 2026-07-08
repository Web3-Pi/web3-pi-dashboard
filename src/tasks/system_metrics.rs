use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::Result;
use get_if_addrs::get_if_addrs;
use sysinfo::Disks;
use tokio::time;

use crate::app::{
    config::{HIGH_TASK_INTERVAL, LOW_TASK_INTERVAL, MEDIUM_TASK_INTERVAL},
    state::SharedState,
};

/// Pi downstream kernels name the SoC sensor "cpu-thermal"; it is not always
/// thermal_zone0, so resolve by type once and cache the path.
fn resolve_thermal_zone() -> PathBuf {
    if let Ok(entries) = std::fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_zone = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("thermal_zone"));
            if !is_zone {
                continue;
            }
            let is_cpu = std::fs::read_to_string(path.join("type"))
                .is_ok_and(|t| t.trim() == "cpu-thermal");
            if is_cpu {
                return path.join("temp");
            }
        }
    }
    PathBuf::from("/sys/class/thermal/thermal_zone0/temp")
}

fn cpu_temperature() -> f32 {
    static TEMP_PATH: OnceLock<PathBuf> = OnceLock::new();
    let path = TEMP_PATH.get_or_init(resolve_thermal_zone);
    match std::fs::read_to_string(path) {
        Ok(v) => v
            .trim()
            .parse::<f32>()
            .ok()
            .map(|millis| millis / 1000.0)
            .unwrap_or(0.0),
        Err(_) => 0.0,
    }
}

fn hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|v| v.into_string().ok())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// First interface with Destination == 00000000 in /proc/net/route.
fn default_route_iface_from(content: &str) -> Option<String> {
    for line in content.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let (Some(iface), Some(dest)) = (parts.next(), parts.next()) else {
            continue;
        };
        if dest == "00000000" {
            return Some(iface.to_owned());
        }
    }
    None
}

fn default_route_iface() -> Option<String> {
    let content = std::fs::read_to_string("/proc/net/route").ok()?;
    default_route_iface_from(&content)
}

fn local_ip() -> Option<String> {
    let addrs = get_if_addrs().ok()?;
    let ipv4_of = |name: &str| -> Option<String> {
        addrs.iter().filter(|a| a.name == name).find_map(|a| match a.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() => Some(ip.to_string()),
            _ => None,
        })
    };
    // Prefer the default-route interface; interface names under
    // systemd-networkd vary (eth0 vs end0), so do not rely on one name.
    if let Some(ip) = default_route_iface().and_then(|iface| ipv4_of(&iface)) {
        return Some(ip);
    }
    for pref in ["eth0", "end0", "wlan0"] {
        if let Some(ip) = ipv4_of(pref) {
            return Some(ip);
        }
    }
    addrs.iter().find_map(|a| match a.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() => Some(ip.to_string()),
        _ => None,
    })
}

/// Pick the mount whose mount point is the longest path-prefix of `target`;
/// returns (total, available) in bytes.
fn best_mount<'a>(
    mounts: impl Iterator<Item = (&'a Path, u64, u64)>,
    target: &Path,
) -> Option<(u64, u64)> {
    mounts
        .filter(|(mount, _, _)| target.starts_with(mount))
        .max_by_key(|(mount, _, _)| mount.as_os_str().len())
        .map(|(_, total, available)| (total, available))
}

fn used_stats(total: u64, available: u64) -> (f32, f64) {
    if total == 0 {
        return (0.0, 0.0);
    }
    let used = total.saturating_sub(available);
    let used_tb = used as f64 / 1024_f64.powi(4);
    let percent = (used as f64 * 100.0 / total as f64) as f32;
    (percent, used_tb)
}

/// Returns (used percent, used TB) for the filesystem holding the chain data
/// (first existing of /var/lib/el, /mnt/storage, /).
fn disk_stats() -> (f32, f64) {
    let target = ["/var/lib/el", "/mnt/storage", "/"]
        .into_iter()
        .map(Path::new)
        .find(|p| p.exists())
        .unwrap_or(Path::new("/"));
    let disks = Disks::new_with_refreshed_list();
    let mounts = disks
        .list()
        .iter()
        .map(|d| (d.mount_point(), d.total_space(), d.available_space()));
    match best_mount(mounts, target) {
        Some((total, available)) => used_stats(total, available),
        None => (0.0, 0.0),
    }
}

pub async fn high_frequency_loop(state: SharedState) -> Result<()> {
    let mut interval = time::interval(HIGH_TASK_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    let mut sys = sysinfo::System::new();
    loop {
        interval.tick().await;
        sys.refresh_cpu_usage();
        let cpu = if state.read().await.show_per_core {
            sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
        } else {
            sys.global_cpu_usage()
        };
        let cpu_temp = cpu_temperature();
        let mut guard = state.write().await;
        guard.system.cpu_percent = cpu;
        guard.system.cpu_temp = cpu_temp;
    }
}

pub async fn medium_frequency_loop(state: SharedState) -> Result<()> {
    let mut interval = time::interval(MEDIUM_TASK_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut sys = sysinfo::System::new();
    loop {
        interval.tick().await;
        sys.refresh_memory();
        let mem = sys.total_memory();
        let mem_used = sys.used_memory();
        let swap = sys.total_swap();
        let swap_used = sys.used_swap();
        let mut guard = state.write().await;
        guard.system.mem_percent = if mem == 0 {
            0.0
        } else {
            (mem_used as f64 * 100.0 / mem as f64) as f32
        };
        guard.system.swap_percent = if swap == 0 {
            0.0
        } else {
            (swap_used as f64 * 100.0 / swap as f64) as f32
        };
    }
}

pub async fn low_frequency_loop(state: SharedState) -> Result<()> {
    {
        let mut guard = state.write().await;
        guard.system.hostname = hostname();
    }
    let mut interval = time::interval(LOW_TASK_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let (disk_percent, disk_used_tb) = disk_stats();
        let ip = local_ip();
        let mut guard = state.write().await;
        guard.system.disk_percent = disk_percent;
        guard.system.disk_used_tb = disk_used_tb;
        guard.system.ip_local_address = ip;
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use super::{best_mount, default_route_iface_from, used_stats};
    use crate::app::config::{HIGH_TASK_INTERVAL, LOW_TASK_INTERVAL, MEDIUM_TASK_INTERVAL};

    #[test]
    fn intervals_are_positive() {
        assert!(HIGH_TASK_INTERVAL > Duration::ZERO);
        assert!(MEDIUM_TASK_INTERVAL > Duration::ZERO);
        assert!(LOW_TASK_INTERVAL > Duration::ZERO);
    }

    #[test]
    fn default_route_iface_parsing() {
        let route = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
                     end0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
                     end0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
        assert_eq!(default_route_iface_from(route), Some("end0".to_owned()));
    }

    #[test]
    fn default_route_iface_none_without_default() {
        let route = "Iface\tDestination\tGateway\n\
                     wlan0\t0001A8C0\t00000000\n";
        assert_eq!(default_route_iface_from(route), None);
        assert_eq!(default_route_iface_from(""), None);
    }

    #[test]
    fn best_mount_longest_prefix_wins() {
        let mounts = [
            (Path::new("/"), 100_u64, 50_u64),
            (Path::new("/var/lib/el"), 4000, 1000),
            (Path::new("/var"), 200, 100),
        ];
        let picked = best_mount(mounts.iter().copied(), Path::new("/var/lib/el"));
        assert_eq!(picked, Some((4000, 1000)));
        let root = best_mount(mounts.iter().copied(), Path::new("/home"));
        assert_eq!(root, Some((100, 50)));
        assert_eq!(best_mount([].into_iter(), Path::new("/")), None);
    }

    #[test]
    fn used_stats_semantics() {
        let (percent, used_tb) = used_stats(0, 0);
        assert_eq!((percent, used_tb), (0.0, 0.0));
        let one_tb = 1024_u64.pow(4);
        let (percent, used_tb) = used_stats(2 * one_tb, one_tb / 2);
        assert!((percent - 75.0).abs() < 0.01);
        assert!((used_tb - 1.5).abs() < 0.001);
    }
}
