use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Deserialize;
use tokio::time;
use tracing::{debug, warn};

use crate::app::{
    config::{ETH_HTTP_TIMEOUT_SECONDS, EthStatusConfig},
    state::{SharedState, SyncStatus},
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
    peers: u64,
    head_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct BeaconProbe {
    is_syncing: bool,
    sync_distance: u64,
    peers: u64,
}

fn parse_hex_u64(value: &str) -> Option<u64> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(digits, 16).ok()
}

/// Parse `systemctl is-active <exec> <cons>` stdout: one state per line.
/// Anything but "active" (inactive/failed/activating/unknown/...) is not active.
fn parse_is_active_lines(stdout: &str) -> (bool, bool) {
    let mut lines = stdout.lines().map(|l| l.trim() == "active");
    (lines.next().unwrap_or(false), lines.next().unwrap_or(false))
}

/// One systemctl spawn for both units. `systemctl is-active` exits non-zero
/// when any unit is not active, so the exit code is deliberately ignored.
/// Returns None when the spawn failed or timed out.
async fn unit_states(unit_exec: &str, unit_cons: &str) -> Option<(bool, bool)> {
    let output = time::timeout(
        Duration::from_secs(SYSTEMCTL_TIMEOUT_SECONDS),
        tokio::process::Command::new("systemctl")
            .args(["is-active", "--", unit_exec, unit_cons])
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
        .and_then(|v| v.as_str().and_then(parse_hex_u64))
        .unwrap_or(0);
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
    let peers = beacon_peer_count(client, base).await.unwrap_or(0);
    Some(BeaconProbe {
        is_syncing: parsed.data.is_syncing,
        sync_distance,
        peers,
    })
}

fn map_exec(unit_active: bool, probe: Option<GethProbe>, now_unix: u64) -> SyncStatus {
    if !unit_active {
        return SyncStatus::Inactive;
    }
    let Some(p) = probe else {
        return SyncStatus::Waiting;
    };
    if p.syncing {
        return SyncStatus::Syncing;
    }
    let head_fresh = p
        .head_timestamp
        .is_some_and(|ts| now_unix.saturating_sub(ts) <= EXEC_HEAD_MAX_AGE_SECS);
    if !head_fresh {
        return SyncStatus::Syncing;
    }
    if p.peers == 0 {
        return SyncStatus::Waiting;
    }
    SyncStatus::Synced
}

fn map_cons(unit_active: bool, probe: Option<BeaconProbe>) -> SyncStatus {
    if !unit_active {
        return SyncStatus::Inactive;
    }
    let Some(p) = probe else {
        return SyncStatus::Waiting;
    };
    if p.is_syncing || p.sync_distance > CONS_MAX_SYNC_DISTANCE {
        return SyncStatus::Syncing;
    }
    if p.peers == 0 {
        return SyncStatus::Waiting;
    }
    SyncStatus::Synced
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
    let mut exec_active = false;
    let mut cons_active = false;
    let mut spawn_failed_logged = false;
    loop {
        interval.tick().await;
        match unit_states(&cfg.unit_exec, &cfg.unit_cons).await {
            Some((exec, cons)) => {
                exec_active = exec;
                cons_active = cons;
                spawn_failed_logged = false;
            }
            None => {
                if !spawn_failed_logged {
                    warn!("systemctl is-active failed or timed out; keeping last unit states");
                    spawn_failed_logged = true;
                }
            }
        }

        // Probe both endpoints concurrently: sequentially the worst case
        // (5 stalled requests x 3s timeout) would exceed the poll interval.
        let (geth, beacon) = tokio::join!(
            async {
                if exec_active {
                    probe_geth(&client, &cfg.geth_rpc).await
                } else {
                    None
                }
            },
            async {
                if cons_active {
                    probe_beacon(&client, &cfg.beacon_rest).await
                } else {
                    None
                }
            }
        );

        let exec = map_exec(exec_active, geth, now_unix());
        let cons = map_cons(cons_active, beacon);
        let node = exec.worst_of(cons);
        debug!(?exec, ?cons, ?node, "eth status poll");

        let mut guard = state.write().await;
        guard.chain.exec = exec;
        guard.chain.cons = cons;
        guard.chain.node = node;
    }
}

#[cfg(test)]
mod tests {
    use super::{BeaconProbe, GethProbe, map_cons, map_exec, parse_hex_u64, parse_is_active_lines};
    use crate::app::state::SyncStatus;

    const NOW: u64 = 1_700_000_000;

    fn geth(syncing: bool, peers: u64, head_age: Option<u64>) -> Option<GethProbe> {
        Some(GethProbe {
            syncing,
            peers,
            head_timestamp: head_age.map(|age| NOW - age),
        })
    }

    #[test]
    fn parse_is_active_variants() {
        assert_eq!(parse_is_active_lines("active\nactive\n"), (true, true));
        assert_eq!(parse_is_active_lines("active\ninactive\n"), (true, false));
        assert_eq!(parse_is_active_lines("failed\nactivating\n"), (false, false));
        assert_eq!(parse_is_active_lines("unknown\n"), (false, false));
        assert_eq!(parse_is_active_lines(""), (false, false));
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
        assert_eq!(map_exec(false, None, NOW), SyncStatus::Inactive);
        assert_eq!(map_exec(true, None, NOW), SyncStatus::Waiting);
        assert_eq!(map_exec(true, geth(true, 5, Some(10)), NOW), SyncStatus::Syncing);
        // "synced" but stale head (WAN outage heuristic)
        assert_eq!(map_exec(true, geth(false, 5, Some(120)), NOW), SyncStatus::Syncing);
        // "synced" but head timestamp unavailable counts as stale
        assert_eq!(map_exec(true, geth(false, 5, None), NOW), SyncStatus::Syncing);
        assert_eq!(map_exec(true, geth(false, 0, Some(10)), NOW), SyncStatus::Waiting);
        assert_eq!(map_exec(true, geth(false, 5, Some(10)), NOW), SyncStatus::Synced);
    }

    #[test]
    fn cons_mapping() {
        let probe = |is_syncing, sync_distance, peers| {
            Some(BeaconProbe {
                is_syncing,
                sync_distance,
                peers,
            })
        };
        assert_eq!(map_cons(false, None), SyncStatus::Inactive);
        assert_eq!(map_cons(true, None), SyncStatus::Waiting);
        assert_eq!(map_cons(true, probe(true, 0, 5)), SyncStatus::Syncing);
        assert_eq!(map_cons(true, probe(false, 3, 5)), SyncStatus::Syncing);
        assert_eq!(map_cons(true, probe(false, 2, 0)), SyncStatus::Waiting);
        assert_eq!(map_cons(true, probe(false, 2, 5)), SyncStatus::Synced);
    }

    #[test]
    fn node_is_worst_of() {
        assert_eq!(SyncStatus::Synced.worst_of(SyncStatus::Syncing), SyncStatus::Syncing);
        assert_eq!(SyncStatus::Syncing.worst_of(SyncStatus::Waiting), SyncStatus::Waiting);
        assert_eq!(SyncStatus::Waiting.worst_of(SyncStatus::Inactive), SyncStatus::Inactive);
        assert_eq!(SyncStatus::Synced.worst_of(SyncStatus::Synced), SyncStatus::Synced);
    }
}
