use anyhow::Result;
use serde::Deserialize;
use std::time::Instant;
use tokio::{task, time};
use tracing::{info, warn};

use crate::app::{
    config::{INSTALL_STATUS_PATH, INSTALL_TASK_INTERVAL, INSTALL_WARN_RATE_LIMIT, install_startup_grace},
    state::{InstallStage, SharedState},
};

#[derive(Debug, Deserialize)]
struct StatusLine {
    #[serde(rename = "statusShort")]
    status_short: Option<String>,
    stage: Option<StageValue>,
    level: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StageValue {
    Int(i32),
    String(String),
}

impl StageValue {
    fn to_i32(&self) -> Option<i32> {
        match self {
            Self::Int(v) => Some(*v),
            Self::String(v) => v.trim().parse::<i32>().ok(),
        }
    }
}

#[derive(Debug)]
enum StatusInputState {
    Valid(StatusLine),
    MissingFile,
    EmptyFile,
    InvalidJson,
    MissingStage,
}

fn read_status(path: &str) -> StatusInputState {
    let Ok(content) = std::fs::read_to_string(path) else {
        return StatusInputState::MissingFile;
    };
    let Some(line) = content.lines().rfind(|l| !l.trim().is_empty()) else {
        return StatusInputState::EmptyFile;
    };
    let Ok(parsed) = serde_json::from_str::<StatusLine>(line) else {
        return StatusInputState::InvalidJson;
    };
    if parsed.stage.is_none() {
        return StatusInputState::MissingStage;
    }
    StatusInputState::Valid(parsed)
}

fn should_emit_warn(last_warn_at: Option<Instant>) -> bool {
    match last_warn_at {
        Some(at) => at.elapsed() >= INSTALL_WARN_RATE_LIMIT,
        None => true,
    }
}

pub async fn install_stage_loop(state: SharedState) -> Result<()> {
    let mut interval = time::interval(INSTALL_TASK_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let started_at = Instant::now();
    let startup_grace = install_startup_grace();
    let mut last_valid_stage_at: Option<Instant> = None;
    let mut consecutive_parse_failures: u64 = 0;
    let mut last_warn_log_at: Option<Instant> = None;
    let mut fallback_applied = false;

    loop {
        interval.tick().await;

        match read_status(INSTALL_STATUS_PATH) {
            StatusInputState::Valid(parsed) => {
                consecutive_parse_failures = 0;
                last_valid_stage_at = Some(Instant::now());
                let stage_raw = parsed.stage.as_ref().and_then(StageValue::to_i32);

                let mut guard = state.write().await;
                let old_stage = guard.install.stage;
                if let Some(status) = parsed.status_short {
                    guard.install.status_short = status;
                }

                if let Some(stage_raw) = stage_raw {
                    let new_stage = InstallStage::from_raw(stage_raw);
                    if old_stage == InstallStage::Stage2 && new_stage == InstallStage::Done {
                        guard.install.needs_stage_done_animation = true;
                    }
                    if old_stage != new_stage {
                        info!(
                            old_stage = ?old_stage,
                            new_stage = ?new_stage,
                            status_short = guard.install.status_short,
                            "Install stage transition"
                        );
                    }
                    guard.install.stage = new_stage;
                }

                let previous_error = guard.install.any_error;
                match parsed.level.as_deref() {
                    Some("ERROR") => {
                        match stage_raw.unwrap_or(-1) {
                            0 => guard.install.stage0_error = true,
                            1 => guard.install.stage1_error = true,
                            2 => guard.install.stage2_error = true,
                            _ => {}
                        }
                        guard.install.any_error = true;
                    }
                    Some("INFO") => {
                        guard.install.stage0_error = false;
                        guard.install.stage1_error = false;
                        guard.install.stage2_error = false;
                        guard.install.any_error = false;
                    }
                    _ => {}
                }
                if previous_error != guard.install.any_error {
                    info!(any_error = guard.install.any_error, "Install error state changed");
                }
            }
            failure_state => {
                consecutive_parse_failures = consecutive_parse_failures.saturating_add(1);
                if should_emit_warn(last_warn_log_at) {
                    warn!(
                        status_path = INSTALL_STATUS_PATH,
                        failures = consecutive_parse_failures,
                        state = ?failure_state,
                        "Install status read/parse failed"
                    );
                    last_warn_log_at = Some(Instant::now());
                }

                if !fallback_applied
                    && last_valid_stage_at.is_none()
                    && started_at.elapsed() >= startup_grace
                {
                    let mut guard = state.write().await;
                    if guard.install.stage != InstallStage::Done {
                        info!(
                            grace_seconds = startup_grace.as_secs(),
                            "No valid install status seen; forcing dashboard mode"
                        );
                        guard.install.stage = InstallStage::Done;
                        guard.install.status_short = "fallback: status unavailable".to_owned();
                    }
                    fallback_applied = true;
                }
            }
        }
        task::yield_now().await;
    }
}

#[cfg(test)]
mod tests {
    use super::{StageValue, should_emit_warn};
    use crate::app::state::InstallStage;
    use std::time::{Duration, Instant};

    #[test]
    fn map_install_stage_boundaries() {
        assert_eq!(InstallStage::from_raw(-1), InstallStage::Unknown);
        assert_eq!(InstallStage::from_raw(0), InstallStage::Stage0);
        assert_eq!(InstallStage::from_raw(1), InstallStage::Stage1);
        assert_eq!(InstallStage::from_raw(2), InstallStage::Stage2);
        assert_eq!(InstallStage::from_raw(100), InstallStage::Done);
    }

    #[test]
    fn parse_stage_string_or_int() {
        assert_eq!(StageValue::Int(100).to_i32(), Some(100));
        assert_eq!(StageValue::String("100".to_owned()).to_i32(), Some(100));
        assert_eq!(StageValue::String("abc".to_owned()).to_i32(), None);
    }

    #[test]
    fn warn_rate_limiter() {
        assert!(should_emit_warn(None));
        assert!(!should_emit_warn(Some(Instant::now())));
        assert!(should_emit_warn(Some(Instant::now() - Duration::from_secs(30))));
    }
}
