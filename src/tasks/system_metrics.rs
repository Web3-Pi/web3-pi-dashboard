use std::path::Path;

use anyhow::Result;
use get_if_addrs::get_if_addrs;
use tokio::time;

use crate::app::{
    config::{HIGH_TASK_INTERVAL, LOW_TASK_INTERVAL, MEDIUM_TASK_INTERVAL},
    state::SharedState,
};

fn cpu_temperature() -> f32 {
    let path = "/sys/class/thermal/thermal_zone0/temp";
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

fn local_ip() -> Option<String> {
    let addrs = get_if_addrs().ok()?;
    for pref in ["eth0", "wlan0"] {
        let candidate = addrs.iter().find(|a| a.name == pref && !a.is_loopback())?;
        if let std::net::IpAddr::V4(ip) = candidate.ip() {
            let ip_s = ip.to_string();
            if !ip_s.starts_with("127.") {
                return Some(ip_s);
            }
        }
    }
    None
}

fn disk_stats() -> (f32, f64) {
    let mount = if Path::new("/mnt/storage/").is_dir() {
        "/mnt/storage/"
    } else {
        "/home/"
    };

    let output = std::process::Command::new("df")
        .arg("-B1")
        .arg(mount)
        .output()
        .ok();
    let Some(output) = output else {
        return (0.0, 0.0);
    };
    if !output.status.success() {
        return (0.0, 0.0);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().nth(1).unwrap_or_default();
    let mut parts = line.split_whitespace();
    let _filesystem = parts.next();
    let total = parts.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let used = parts.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    if total <= 0.0 {
        return (0.0, 0.0);
    }
    let used_tb = used / 1024_f64.powi(4);
    let percent = ((used / total) * 100.0) as f32;
    (percent, used_tb)
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
    use std::time::Duration;

    use crate::app::config::{HIGH_TASK_INTERVAL, LOW_TASK_INTERVAL, MEDIUM_TASK_INTERVAL};

    use crate::app::state::SyncStatus;

    #[test]
    fn map_sync_status_boundaries() {
        assert_eq!(SyncStatus::from_percent(0), SyncStatus::Inactive);
        assert_eq!(SyncStatus::from_percent(25), SyncStatus::Inactive);
        assert_eq!(SyncStatus::from_percent(26), SyncStatus::Waiting);
        assert_eq!(SyncStatus::from_percent(45), SyncStatus::Waiting);
        assert_eq!(SyncStatus::from_percent(46), SyncStatus::Syncing);
        assert_eq!(SyncStatus::from_percent(76), SyncStatus::Syncing);
        assert_eq!(SyncStatus::from_percent(77), SyncStatus::Synced);
        assert_eq!(SyncStatus::from_percent(100), SyncStatus::Synced);
        assert_eq!(SyncStatus::from_percent(101), SyncStatus::Unknown);
    }

    #[test]
    fn intervals_are_positive() {
        assert!(HIGH_TASK_INTERVAL > Duration::ZERO);
        assert!(MEDIUM_TASK_INTERVAL > Duration::ZERO);
        assert!(LOW_TASK_INTERVAL > Duration::ZERO);
    }
}
