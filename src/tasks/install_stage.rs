use anyhow::Result;
use serde::Deserialize;
use std::time::Instant;
use tokio::{task, time};
use tracing::{info, warn};

use crate::app::{
    config::{
        INSTALL_TASK_INTERVAL, INSTALL_WARN_RATE_LIMIT, VOS_INSTALL_STAGE_PATH,
        install_startup_grace, install_status_path,
    },
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
    /// Neither the jlog nor the vOS stage file exists.
    NoStatusFile,
    EmptyFile,
    InvalidJson,
    MissingStage,
    InvalidVosStage,
}

fn parse_jlog(content: &str) -> StatusInputState {
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

/// vOS writes a plain integer to /root/.install_stage: rc.local writes `0` at
/// first-boot start and `1` once first boot completes; nothing ever writes
/// `100` today. Semantics therefore differ from the jlog stages: `0` =
/// installing, `>= 1` = done. Remap onto the jlog shape (0 -> Stage0,
/// >= 1 -> 100/Done) before handing the value to the shared pipeline.
fn parse_vos_stage(content: &str) -> StatusInputState {
    let Ok(stage) = content.trim().parse::<i32>() else {
        return StatusInputState::InvalidVosStage;
    };
    if stage < 0 {
        return StatusInputState::InvalidVosStage;
    }
    let (stage, status_short) = if stage >= 1 {
        (100, None)
    } else {
        (0, Some("Installing...".to_owned()))
    };
    StatusInputState::Valid(StatusLine {
        status_short,
        stage: Some(StageValue::Int(stage)),
        level: None,
    })
}

fn read_status(jlog_path: &str, vos_path: &str) -> StatusInputState {
    if let Ok(content) = std::fs::read_to_string(jlog_path) {
        return parse_jlog(&content);
    }
    match std::fs::read_to_string(vos_path) {
        Ok(content) => parse_vos_stage(&content),
        Err(_) => StatusInputState::NoStatusFile,
    }
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
    let jlog_path = install_status_path();
    let started_at = Instant::now();
    let startup_grace = install_startup_grace();
    let mut last_valid_stage_at: Option<Instant> = None;
    let mut consecutive_parse_failures: u64 = 0;
    let mut last_warn_log_at: Option<Instant> = None;
    let mut fallback_applied = false;

    loop {
        interval.tick().await;

        match read_status(&jlog_path, VOS_INSTALL_STAGE_PATH) {
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
            StatusInputState::NoStatusFile if last_valid_stage_at.is_none() => {
                // Not an install run: no status source at all — go straight
                // to the dashboard without waiting out the grace period.
                if !fallback_applied {
                    let mut guard = state.write().await;
                    if guard.install.stage != InstallStage::Done {
                        info!(status_path = jlog_path, "No install status file found; showing dashboard");
                        guard.install.stage = InstallStage::Done;
                    }
                    fallback_applied = true;
                }
            }
            failure_state => {
                consecutive_parse_failures = consecutive_parse_failures.saturating_add(1);
                if should_emit_warn(last_warn_log_at) {
                    warn!(
                        status_path = jlog_path,
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
    use super::{StageValue, StatusInputState, parse_jlog, parse_vos_stage, should_emit_warn};
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

    #[test]
    fn parse_vos_stage_values() {
        let StatusInputState::Valid(line) = parse_vos_stage("0\n") else {
            panic!("expected valid stage 0");
        };
        assert_eq!(line.stage.as_ref().and_then(StageValue::to_i32), Some(0));
        assert_eq!(line.status_short.as_deref(), Some("Installing..."));

        // vOS rc.local writes 1 on a fully-operational node: must map to Done.
        let StatusInputState::Valid(line) = parse_vos_stage("1\n") else {
            panic!("expected valid stage 1");
        };
        let raw = line.stage.as_ref().and_then(StageValue::to_i32).unwrap();
        assert_eq!(InstallStage::from_raw(raw), InstallStage::Done);
        assert_eq!(line.status_short, None);

        let StatusInputState::Valid(line) = parse_vos_stage(" 100 ") else {
            panic!("expected valid stage 100");
        };
        assert_eq!(line.stage.as_ref().and_then(StageValue::to_i32), Some(100));
        assert_eq!(line.status_short, None);

        assert!(matches!(parse_vos_stage("abc"), StatusInputState::InvalidVosStage));
        assert!(matches!(parse_vos_stage(""), StatusInputState::InvalidVosStage));
        assert!(matches!(parse_vos_stage("-1"), StatusInputState::InvalidVosStage));
    }

    #[test]
    fn parse_jlog_last_line_wins() {
        let content = "{\"stage\": 0, \"statusShort\": \"a\"}\n{\"stage\": 1, \"statusShort\": \"b\"}\n";
        let StatusInputState::Valid(line) = parse_jlog(content) else {
            panic!("expected valid jlog");
        };
        assert_eq!(line.stage.as_ref().and_then(StageValue::to_i32), Some(1));
        assert!(matches!(parse_jlog(""), StatusInputState::EmptyFile));
        assert!(matches!(parse_jlog("not json\n"), StatusInputState::InvalidJson));
        assert!(matches!(parse_jlog("{\"level\": \"INFO\"}\n"), StatusInputState::MissingStage));
    }
}
