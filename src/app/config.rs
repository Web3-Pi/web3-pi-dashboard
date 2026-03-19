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

pub const LOOP_FPS: u64 = 10;
pub const INSTALL_FPS: u64 = 2;
pub const ANIM_FPS: u64 = 30;
pub const HIGH_TASK_INTERVAL: Duration = Duration::from_secs(1);
pub const MEDIUM_TASK_INTERVAL: Duration = Duration::from_secs(10);
pub const LOW_TASK_INTERVAL: Duration = Duration::from_secs(30);
pub const INSTALL_TASK_INTERVAL: Duration = Duration::from_millis(500);
pub const INSTALL_WARN_RATE_LIMIT: Duration = Duration::from_secs(10);
pub const INSTALL_STARTUP_GRACE_DEFAULT: Duration = Duration::from_secs(25);
pub const INFLUX_FETCH_INTERVAL: Duration = Duration::from_secs(30);
pub const INFLUX_RETRY_BASE_SECONDS: u64 = 10;
pub const INFLUX_TIMEOUT_SECONDS: u64 = 3;

pub fn install_startup_grace() -> Duration {
    std::env::var("W3P_INSTALL_GRACE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(INSTALL_STARTUP_GRACE_DEFAULT)
}

#[derive(Clone)]
pub struct InfluxConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

impl InfluxConfig {
    pub fn from_env() -> Self {
        let host = std::env::var("W3P_INFLUX_HOST").unwrap_or_else(|_| "localhost".to_owned());
        let port = std::env::var("W3P_INFLUX_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8086);
        let username = std::env::var("W3P_INFLUX_USERNAME").unwrap_or_else(|_| "geth".to_owned());
        let password = std::env::var("W3P_INFLUX_PASSWORD").unwrap_or_else(|_| "geth".to_owned());
        let database = std::env::var("W3P_INFLUX_DATABASE").unwrap_or_else(|_| "ethonrpi".to_owned());
        Self {
            host,
            port,
            username,
            password,
            database,
        }
    }
}
