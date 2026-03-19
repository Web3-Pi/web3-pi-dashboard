use anyhow::Result;
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::time;
use tracing::{error, info, warn};

use crate::app::{
    config::{INFLUX_FETCH_INTERVAL, INFLUX_RETRY_BASE_SECONDS, INFLUX_TIMEOUT_SECONDS, InfluxConfig},
    state::SharedState,
};

#[derive(Debug, Deserialize)]
struct InfluxResponse {
    results: Option<Vec<InfluxResult>>,
}

#[derive(Debug, Deserialize)]
struct InfluxResult {
    series: Option<Vec<InfluxSeries>>,
}

#[derive(Debug, Deserialize)]
struct InfluxSeries {
    values: Option<Vec<Vec<serde_json::Value>>>,
}

fn parse_active_percent(body: &str) -> Option<i32> {
    let parsed = serde_json::from_str::<InfluxResponse>(body).ok()?;
    let value = parsed
        .results?
        .into_iter()
        .next()?
        .series?
        .into_iter()
        .next()?
        .values?
        .into_iter()
        .next()?
        .into_iter()
        .nth(1)?;
    value.as_i64().map(|v| v as i32)
}

async fn query_percent(
    client: &reqwest::Client,
    cfg: &InfluxConfig,
    host: &str,
    measurement: &str,
) -> Result<Option<i32>> {
    let q = format!(
        "SELECT \"active_percent\" FROM \"{measurement}\" WHERE \"host\"::tag =~ /^{host}_s$/ ORDER BY time DESC LIMIT 1"
    );
    let encoded_q = urlencoding::encode(&q);
    let url = format!(
        "http://{}:{}/query?db={}&q={}",
        cfg.host, cfg.port, cfg.database, encoded_q
    );
    let response = client
        .get(url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await?;
    if response.status() != StatusCode::OK {
        return Ok(None);
    }
    let body = response.text().await?;
    Ok(parse_active_percent(&body))
}

pub async fn influx_loop(state: SharedState, cfg: InfluxConfig) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(INFLUX_TIMEOUT_SECONDS))
        .build()?;

    let mut backoff_pow = 0_u32;
    loop {
        let host = {
            let guard = state.read().await;
            guard.system.hostname.clone()
        };
        let res = async {
            let exec = query_percent(&client, &cfg, &host, "status_exec").await?;
            let node = query_percent(&client, &cfg, &host, "status_node").await?;
            let cons = query_percent(&client, &cfg, &host, "status_consensus").await?;
            Ok::<_, anyhow::Error>((exec, node, cons))
        }
        .await;

        match res {
            Ok((exec, node, cons)) => {
                {
                    let mut guard = state.write().await;
                    if let Some(v) = exec {
                        guard.chain.exec_percent = v;
                    }
                    if let Some(v) = node {
                        guard.chain.node_percent = v;
                    }
                    if let Some(v) = cons {
                        guard.chain.cons_percent = v;
                    }
                }
                backoff_pow = 0;
                time::sleep(INFLUX_FETCH_INTERVAL).await;
            }
            Err(err) => {
                error!("InfluxDB polling failed: {err}");
                {
                    let mut guard = state.write().await;
                    guard.chain.exec_percent = 0;
                    guard.chain.node_percent = 0;
                    guard.chain.cons_percent = 0;
                }
                let delay = (INFLUX_RETRY_BASE_SECONDS as f64 * 1.5_f64.powi(backoff_pow as i32))
                    .min(300.0) as u64;
                if backoff_pow == 0 {
                    warn!("InfluxDB disconnected, retrying...");
                } else {
                    info!("InfluxDB retry in {delay}s");
                }
                time::sleep(std::time::Duration::from_secs(delay)).await;
                backoff_pow = backoff_pow.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_active_percent;

    #[test]
    fn parse_influx_value() {
        let body = r#"{"results":[{"series":[{"name":"status_exec","columns":["time","active_percent"],"values":[["2024-01-01T00:00:00Z",78]]}]}]}"#;
        assert_eq!(parse_active_percent(body), Some(78));
    }
}
