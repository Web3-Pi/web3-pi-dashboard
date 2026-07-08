use std::{
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};

pub const GRID_WIDTH: u32 = 240;
pub const GRID_HEIGHT: u32 = 280;
pub const SHOW_PER_CORE: bool = false;

pub const INSTALL_STATUS_PATH_DEFAULT: &str = "/opt/web3pi/status.jlog";
/// vOS first-boot machinery writes a plain integer stage (0, 1, 100=done).
pub const VOS_INSTALL_STAGE_PATH: &str = "/root/.install_stage";
pub const STATE_DIR_DEFAULT: &str = "/var/lib/w3p-hwm";

/// Asset root: `W3P_ASSET_DIR`, else the running executable's directory,
/// else the current working directory.
pub fn asset_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Ok(dir) = std::env::var("W3P_ASSET_DIR") {
            return PathBuf::from(dir);
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            return parent.to_owned();
        }
        PathBuf::from(".")
    })
    .clone()
}

pub fn font_path() -> PathBuf {
    asset_dir().join("font/JetBrainsMono-Medium.ttf")
}

pub fn bg_path() -> PathBuf {
    asset_dir().join("img/lcdbg.png")
}

pub fn logo_path() -> PathBuf {
    asset_dir().join("img/web3-pi-logo-240x70.png")
}

pub fn final_logo_path() -> PathBuf {
    asset_dir().join("img/Web3Pi_logo_0.png")
}

pub fn anim_dir() -> PathBuf {
    asset_dir().join("img/3D")
}

pub fn install_status_path() -> String {
    std::env::var("W3P_INSTALL_STATUS_PATH")
        .unwrap_or_else(|_| INSTALL_STATUS_PATH_DEFAULT.to_owned())
}

/// Opening-flag lives in the systemd state directory (`STATE_DIRECTORY`,
/// first entry if multiple), falling back to /var/lib/w3p-hwm.
pub fn opening_flag_path() -> PathBuf {
    let dir = std::env::var("STATE_DIRECTORY")
        .ok()
        .and_then(|v| v.split(':').next().map(str::to_owned))
        .unwrap_or_else(|| STATE_DIR_DEFAULT.to_owned());
    PathBuf::from(dir).join("opening.flag")
}

pub const ST7789_WIDTH: u16 = 240;
pub const ST7789_HEIGHT: u16 = 280;
pub const SPI_BUS: u8 = 0;
pub const SPI_CS: u8 = 0;
pub const SPI_SPEED_HZ: u32 = 40_000_000;
pub const PIN_RST: u8 = 27;
pub const PIN_DC: u8 = 25;
pub const PIN_BL: u8 = 18;

pub const COLOR_BG: &str = "#00129A";
pub const COLOR_TEXT_MAIN: &str = "#FFFFFF";
pub const COLOR_TEXT_SECONDARY: &str = "#A1A1A1";
pub const COLOR_GREEN: &str = "#22C55E";
pub const COLOR_RED: &str = "#EF4433";

pub const LOOP_FPS: u64 = 8;
pub const INSTALL_FPS: u64 = 2;
pub const ANIM_FPS: u64 = 30;
pub const DASH_WAVE_CYCLES_PER_SEC: f32 = 0.35;
pub const DASH_WAVE_AMPLITUDE_PX: f32 = 6.0;
pub const DASH_WAVE_BASE_Y: f32 = 269.0;
pub const DASH_WAVE_THICKNESS_PX: u8 = 2;
pub const HIGH_TASK_INTERVAL: Duration = Duration::from_secs(1);
pub const MEDIUM_TASK_INTERVAL: Duration = Duration::from_secs(10);
pub const LOW_TASK_INTERVAL: Duration = Duration::from_secs(30);
pub const INSTALL_TASK_INTERVAL: Duration = Duration::from_millis(500);
pub const INSTALL_WARN_RATE_LIMIT: Duration = Duration::from_secs(10);
pub const INSTALL_STARTUP_GRACE_DEFAULT: Duration = Duration::from_secs(25);
pub const ETH_POLL_SECONDS_DEFAULT: u64 = 10;
pub const ETH_HTTP_TIMEOUT_SECONDS: u64 = 3;

pub fn install_startup_grace() -> Duration {
    std::env::var("W3P_INSTALL_GRACE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(INSTALL_STARTUP_GRACE_DEFAULT)
}

#[derive(Clone)]
pub struct EthStatusConfig {
    pub poll_interval: Duration,
    pub unit_exec: String,
    pub unit_cons: String,
    pub unit_vali: String,
    pub geth_rpc: String,
    pub beacon_rest: String,
}

impl EthStatusConfig {
    pub fn from_env() -> Self {
        let poll_seconds = std::env::var("W3P_ETH_POLL_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(ETH_POLL_SECONDS_DEFAULT);
        Self {
            poll_interval: Duration::from_secs(poll_seconds),
            unit_exec: std::env::var("W3P_UNIT_EXEC").unwrap_or_else(|_| "geth.service".to_owned()),
            unit_cons: std::env::var("W3P_UNIT_CONS")
                .unwrap_or_else(|_| "nimbus-beacon-node.service".to_owned()),
            unit_vali: std::env::var("W3P_UNIT_VALI")
                .unwrap_or_else(|_| "nimbus-validator".to_owned()),
            geth_rpc: std::env::var("W3P_GETH_RPC")
                .unwrap_or_else(|_| "http://127.0.0.1:8545".to_owned()),
            beacon_rest: std::env::var("W3P_BEACON_REST")
                .unwrap_or_else(|_| "http://127.0.0.1:5052".to_owned()),
        }
    }
}
