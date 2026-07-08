use std::time::Duration;

pub const GRID_WIDTH: u32 = 240;
pub const GRID_HEIGHT: u32 = 280;
pub const SHOW_PER_CORE: bool = false;

pub const FONT_PATH: &str = "./font/JetBrainsMono-Medium.ttf";
pub const BG_PATH: &str = "./img/lcdbg.png";
pub const LOGO_PATH: &str = "./img/web3-pi-logo-240x70.png";
pub const FINAL_LOGO_PATH: &str = "./img/Web3Pi_logo_0.png";
pub const ANIM_DIR: &str = "./img/3D/";
pub const OPENING_FLAG_PATH: &str = "/root/opening.flag";
pub const INSTALL_STATUS_PATH: &str = "/opt/web3pi/status.jlog";

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
            geth_rpc: std::env::var("W3P_GETH_RPC")
                .unwrap_or_else(|_| "http://127.0.0.1:8545".to_owned()),
            beacon_rest: std::env::var("W3P_BEACON_REST")
                .unwrap_or_else(|_| "http://127.0.0.1:5052".to_owned()),
        }
    }
}
