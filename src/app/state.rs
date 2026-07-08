use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Inactive,
    Waiting,
    Syncing,
    Synced,
    Unknown,
}

impl SyncStatus {
    /// Severity ordering used for the aggregate NODE tile:
    /// Unknown > Inactive > Waiting > Syncing > Synced.
    fn severity(self) -> u8 {
        match self {
            Self::Unknown => 4,
            Self::Inactive => 3,
            Self::Waiting => 2,
            Self::Syncing => 1,
            Self::Synced => 0,
        }
    }

    pub fn worst_of(self, other: Self) -> Self {
        if self.severity() >= other.severity() { self } else { other }
    }

    pub fn as_label(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Waiting => "waiting",
            Self::Syncing => "syncing",
            Self::Synced => "synced",
            Self::Unknown => "unknown",
        }
    }

    pub fn as_color(self) -> [u8; 3] {
        match self {
            Self::Inactive => [255, 0, 0],
            Self::Waiting => [255, 255, 0],
            Self::Syncing => [255, 165, 0],
            Self::Synced => [0, 255, 0],
            Self::Unknown => [255, 255, 255],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStage {
    Unknown,
    Stage0,
    Stage1,
    Stage2,
    Done,
}

impl InstallStage {
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::Stage0,
            1 => Self::Stage1,
            2 => Self::Stage2,
            100 => Self::Done,
            _ => Self::Unknown,
        }
    }

}

#[derive(Debug, Clone)]
pub struct SystemState {
    pub cpu_percent: f32,
    pub cpu_temp: f32,
    pub mem_percent: f32,
    pub swap_percent: f32,
    pub disk_percent: f32,
    pub disk_used_tb: f64,
    pub ip_local_address: Option<String>,
    pub hostname: String,
}

#[derive(Debug, Clone)]
pub struct ChainState {
    pub exec: SyncStatus,
    pub node: SyncStatus,
    pub cons: SyncStatus,
}

#[derive(Debug, Clone)]
pub struct InstallState {
    pub stage: InstallStage,
    pub status_short: String,
    pub stage0_error: bool,
    pub stage1_error: bool,
    pub stage2_error: bool,
    pub any_error: bool,
    pub needs_stage_done_animation: bool,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub animation_tick: u64,
    pub spinner: String,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub show_per_core: bool,
    pub system: SystemState,
    pub chain: ChainState,
    pub install: InstallState,
    pub ui: UiState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            show_per_core: crate::app::config::SHOW_PER_CORE,
            system: SystemState {
                cpu_percent: 0.0,
                cpu_temp: 0.0,
                mem_percent: 0.0,
                swap_percent: 0.0,
                disk_percent: 0.0,
                disk_used_tb: 0.0,
                ip_local_address: None,
                hostname: "unknown".to_owned(),
            },
            chain: ChainState {
                exec: SyncStatus::Unknown,
                node: SyncStatus::Unknown,
                cons: SyncStatus::Unknown,
            },
            install: InstallState {
                stage: InstallStage::Unknown,
                status_short: String::new(),
                stage0_error: false,
                stage1_error: false,
                stage2_error: false,
                any_error: false,
                needs_stage_done_animation: false,
            },
            ui: UiState {
                animation_tick: 0,
                spinner: "   ".to_owned(),
            },
        }
    }
}

impl AppState {
    pub fn advance_spinner(&mut self) {
        let dots = (self.ui.spinner.chars().filter(|c| *c == '.').count() + 1) % 4;
        self.ui.spinner = format!("{}{}", ".".repeat(dots), " ".repeat(3 - dots));
    }
}

pub type SharedState = Arc<RwLock<AppState>>;
