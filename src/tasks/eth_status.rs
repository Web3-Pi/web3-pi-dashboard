use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Deserialize;
use tokio::time;
use tracing::{debug, warn};

use crate::app::{
    config::{ETH_HTTP_TIMEOUT_SECONDS, EthStatusConfig},
    state::{ClientState, ServiceState, SharedState, SyncState},
};

/// Head older than this is treated as stale: geth reports `eth_syncing=false`
/// even when offline (e.g. WAN outage), so a fresh head is required for green.
const EXEC_HEAD_MAX_AGE_SECS: u64 = 90;
/// Consensus sync distance (in slots) still considered "synced".
const CONS_MAX_SYNC_DISTANCE: u64 = 2;
/// Bound on `systemctl is-active` (talks to PID 1 over D-Bus, which can block
/// far longer than the poll interval when systemd is busy, e.g. early in boot).
const SYSTEMCTL_TIMEOUT_SECONDS: u64 = 3;

#[derive(Debug, Clone, Copy)]
struct GethProbe {
    syncing: bool,
    /// None when `net_peerCount` did not respond this cycle.
    peers: Option<u64>,
    head_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct BeaconProbe {
    is_syncing: bool,
    sync_distance: u64,
    /// None when `/eth/v1/node/peer_count` did not respond this cycle.
    peers: Option<u64>,
}

fn parse_hex_u64(value: &str) -> Option<u64> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(digits, 16).ok()
}

/// Parse `systemctl is-active <exec> <cons> <vali>` stdout: one state per
/// line, in argument order. Missing lines map to Unknown.
fn parse_is_active_lines(stdout: &str) -> [ServiceState; 3] {
    let mut lines = stdout.lines();
    std::array::from_fn(|_| {
        lines
            .next()
            .map_or(ServiceState::Unknown, ServiceState::from_systemctl)
    })
}

/// One systemctl spawn for all three units. `systemctl is-active` exits
/// non-zero when any unit is not active, so the exit code is deliberately
/// ignored. Returns None when the spawn failed or timed out.
async fn unit_states(
    unit_exec: &str,
    unit_cons: &str,
    unit_vali: &str,
) -> Option<[ServiceState; 3]> {
    let output = time::timeout(
        Duration::from_secs(SYSTEMCTL_TIMEOUT_SECONDS),
        tokio::process::Command::new("systemctl")
            .args(["is-active", "--", unit_exec, unit_cons, unit_vali])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    Some(parse_is_active_lines(&String::from_utf8_lossy(&output.stdout)))
}

async fn rpc_call(
    client: &reqwest::Client,
    base: &str,
    method: &str,
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let response = client.post(base).json(&body).send().await.ok()?;
    let value: serde_json::Value = response.json().await.ok()?;
    value.get("result").cloned()
}

/// Returns None when the RPC endpoint is unreachable.
async fn probe_geth(client: &reqwest::Client, base: &str) -> Option<GethProbe> {
    // eth_syncing -> false | {currentBlock, highestBlock, ...}
    let syncing_result = rpc_call(client, base, "eth_syncing", serde_json::json!([])).await?;
    let syncing = !matches!(syncing_result, serde_json::Value::Bool(false));
    let peers = rpc_call(client, base, "net_peerCount", serde_json::json!([]))
        .await
        .and_then(|v| v.as_str().and_then(parse_hex_u64));
    let head_timestamp = rpc_call(
        client,
        base,
        "eth_getBlockByNumber",
        serde_json::json!(["latest", false]),
    )
    .await
    .and_then(|v| v.get("timestamp").and_then(|t| t.as_str()).and_then(parse_hex_u64));
    Some(GethProbe {
        syncing,
        peers,
        head_timestamp,
    })
}

#[derive(Debug, Deserialize)]
struct BeaconSyncingResponse {
    data: BeaconSyncingData,
}

#[derive(Debug, Deserialize)]
struct BeaconSyncingData {
    // Numeric values are JSON strings in the beacon REST API.
    sync_distance: String,
    is_syncing: bool,
}

#[derive(Debug, Deserialize)]
struct BeaconPeerCountResponse {
    data: BeaconPeerCountData,
}

#[derive(Debug, Deserialize)]
struct BeaconPeerCountData {
    connected: String,
}

async fn beacon_peer_count(client: &reqwest::Client, base: &str) -> Option<u64> {
    let url = format!("{base}/eth/v1/node/peer_count");
    let response = client.get(&url).send().await.ok()?;
    let parsed: BeaconPeerCountResponse = response.json().await.ok()?;
    parsed.data.connected.parse::<u64>().ok()
}

/// Returns None when the beacon REST endpoint is unreachable.
async fn probe_beacon(client: &reqwest::Client, base: &str) -> Option<BeaconProbe> {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/eth/v1/node/syncing");
    let response = client.get(&url).send().await.ok()?;
    let parsed: BeaconSyncingResponse = response.json().await.ok()?;
    let sync_distance = parsed.data.sync_distance.parse::<u64>().ok()?;
    let peers = beacon_peer_count(client, base).await;
    Some(BeaconProbe {
        is_syncing: parsed.data.is_syncing,
        sync_distance,
        peers,
    })
}

/// Sync line and peers exist only while the service is running; a running
/// service with an unreachable API shows "no api" and no peer count.
fn map_exec(service: ServiceState, probe: Option<GethProbe>, now_unix: u64) -> ClientState {
    if service != ServiceState::Running {
        return ClientState { service, sync: None, peers: None };
    }
    let Some(p) = probe else {
        return ClientState { service, sync: Some(SyncState::NoApi), peers: None };
    };
    // geth reports `eth_syncing=false` even when offline (e.g. WAN outage),
    // so a stale/unknown head counts as still syncing.
    let head_fresh = p
        .head_timestamp
        .is_some_and(|ts| now_unix.saturating_sub(ts) <= EXEC_HEAD_MAX_AGE_SECS);
    let sync = if p.syncing || !head_fresh {
        SyncState::Syncing
    } else {
        SyncState::Synced
    };
    ClientState { service, sync: Some(sync), peers: p.peers }
}

fn map_cons(service: ServiceState, probe: Option<BeaconProbe>) -> ClientState {
    if service != ServiceState::Running {
        return ClientState { service, sync: None, peers: None };
    }
    let Some(p) = probe else {
        return ClientState { service, sync: Some(SyncState::NoApi), peers: None };
    };
    let sync = if p.is_syncing || p.sync_distance > CONS_MAX_SYNC_DISTANCE {
        SyncState::Syncing
    } else {
        SyncState::Synced
    };
    ClientState { service, sync: Some(sync), peers: p.peers }
}

/// The validator client has no sync/peers concept — service state only.
fn map_vali(service: ServiceState) -> ClientState {
    ClientState { service, sync: None, peers: None }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub async fn eth_status_loop(state: SharedState, cfg: EthStatusConfig) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(ETH_HTTP_TIMEOUT_SECONDS))
        .build()?;
    let mut interval = time::interval(cfg.poll_interval);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    // Last-known unit states survive a failed systemctl spawn.
    let mut services = [ServiceState::Unknown; 3];
    let mut spawn_failed_logged = false;
    loop {
        interval.tick().await;
        match unit_states(&cfg.unit_exec, &cfg.unit_cons, &cfg.unit_vali).await {
            Some(states) => {
                services = states;
                spawn_failed_logged = false;
            }
            None => {
                if !spawn_failed_logged {
                    warn!("systemctl is-active failed or timed out; keeping last unit states");
                    spawn_failed_logged = true;
                }
            }
        }
        let [exec_service, cons_service, vali_service] = services;

        // Probe both endpoints concurrently: sequentially the worst case
        // (5 stalled requests x 3s timeout) would exceed the poll interval.
        let (geth, beacon) = tokio::join!(
            async {
                if exec_service == ServiceState::Running {
                    probe_geth(&client, &cfg.geth_rpc).await
                } else {
                    None
                }
            },
            async {
                if cons_service == ServiceState::Running {
                    probe_beacon(&client, &cfg.beacon_rest).await
                } else {
                    None
                }
            }
        );

        let exec = map_exec(exec_service, geth, now_unix());
        let cons = map_cons(cons_service, beacon);
        let vali = map_vali(vali_service);
        debug!(?exec, ?cons, ?vali, "eth status poll");

        let mut guard = state.write().await;
        guard.chain.exec = exec;
        guard.chain.cons = cons;
        guard.chain.vali = vali;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BeaconProbe, GethProbe, map_cons, map_exec, map_vali, parse_hex_u64,
        parse_is_active_lines,
    };
    use crate::app::state::{ClientState, ServiceState, SyncState};

    const NOW: u64 = 1_700_000_000;

    fn geth(syncing: bool, peers: Option<u64>, head_age: Option<u64>) -> Option<GethProbe> {
        Some(GethProbe {
            syncing,
            peers,
            head_timestamp: head_age.map(|age| NOW - age),
        })
    }

    fn client(
        service: ServiceState,
        sync: Option<SyncState>,
        peers: Option<u64>,
    ) -> ClientState {
        ClientState { service, sync, peers }
    }

    #[test]
    fn parse_is_active_variants() {
        use ServiceState::{Failed, Running, Starting, Stopped, Unknown};
        assert_eq!(parse_is_active_lines("active\nactive\nactive\n"), [Running; 3]);
        assert_eq!(
            parse_is_active_lines("active\ninactive\nfailed\n"),
            [Running, Stopped, Failed]
        );
        assert_eq!(
            parse_is_active_lines("activating\nreloading\ndeactivating\n"),
            [Starting, Starting, Stopped]
        );
        // missing lines and unrecognised states map to Unknown
        assert_eq!(parse_is_active_lines("active\n"), [Running, Unknown, Unknown]);
        assert_eq!(parse_is_active_lines("bogus\n"), [Unknown; 3]);
        assert_eq!(parse_is_active_lines(""), [Unknown; 3]);
    }

    #[test]
    fn parse_hex_values() {
        assert_eq!(parse_hex_u64("0x0"), Some(0));
        assert_eq!(parse_hex_u64("0x1a"), Some(26));
        assert_eq!(parse_hex_u64("ff"), Some(255));
        assert_eq!(parse_hex_u64("nope"), None);
    }

    #[test]
    fn exec_mapping() {
        use ServiceState::{Failed, Running, Stopped};
        // not running: no sync line, no peers — whatever the probe says
        assert_eq!(map_exec(Stopped, None, NOW), client(Stopped, None, None));
        assert_eq!(
            map_exec(Failed, geth(false, Some(5), Some(10)), NOW),
            client(Failed, None, None)
        );
        // running but RPC unreachable
        assert_eq!(
            map_exec(Running, None, NOW),
            client(Running, Some(SyncState::NoApi), None)
        );
        // syncing
        assert_eq!(
            map_exec(Running, geth(true, Some(5), Some(10)), NOW),
            client(Running, Some(SyncState::Syncing), Some(5))
        );
        // "synced" but stale head (WAN outage heuristic)
        assert_eq!(
            map_exec(Running, geth(false, Some(5), Some(120)), NOW),
            client(Running, Some(SyncState::Syncing), Some(5))
        );
        // "synced" but head timestamp unavailable counts as stale
        assert_eq!(
            map_exec(Running, geth(false, Some(5), None), NOW),
            client(Running, Some(SyncState::Syncing), Some(5))
        );
        // synced; peer count passes through, including 0 and absent
        assert_eq!(
            map_exec(Running, geth(false, Some(5), Some(10)), NOW),
            client(Running, Some(SyncState::Synced), Some(5))
        );
        assert_eq!(
            map_exec(Running, geth(false, Some(0), Some(10)), NOW),
            client(Running, Some(SyncState::Synced), Some(0))
        );
        assert_eq!(
            map_exec(Running, geth(false, None, Some(10)), NOW),
            client(Running, Some(SyncState::Synced), None)
        );
    }

    #[test]
    fn cons_mapping() {
        use ServiceState::{Running, Stopped};
        let probe = |is_syncing, sync_distance, peers| {
            Some(BeaconProbe {
                is_syncing,
                sync_distance,
                peers,
            })
        };
        assert_eq!(map_cons(Stopped, None), client(Stopped, None, None));
        assert_eq!(
            map_cons(Running, None),
            client(Running, Some(SyncState::NoApi), None)
        );
        assert_eq!(
            map_cons(Running, probe(true, 0, Some(5))),
            client(Running, Some(SyncState::Syncing), Some(5))
        );
        assert_eq!(
            map_cons(Running, probe(false, 3, Some(5))),
            client(Running, Some(SyncState::Syncing), Some(5))
        );
        assert_eq!(
            map_cons(Running, probe(false, 2, Some(5))),
            client(Running, Some(SyncState::Synced), Some(5))
        );
        assert_eq!(
            map_cons(Running, probe(false, 2, None)),
            client(Running, Some(SyncState::Synced), None)
        );
    }

    #[test]
    fn vali_mapping_never_has_sync_or_peers() {
        for service in [
            ServiceState::Running,
            ServiceState::Starting,
            ServiceState::Stopped,
            ServiceState::Failed,
            ServiceState::Unknown,
        ] {
            assert_eq!(map_vali(service), client(service, None, None));
        }
    }
}
