use std::{
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use gpiocdev::{line::Value, request::Request};
use image::RgbImage;
use spidev::{SpiModeFlags, Spidev, SpidevOptions, SpidevTransfer};
use tracing::{info, warn};

use crate::app::config;

pub trait DisplayBackend {
    fn init(&mut self) -> Result<()>;
    fn clear(&mut self) -> Result<()>;
    fn set_backlight(&mut self, duty_percent: u8) -> Result<()>;
    fn show_image(&mut self, image: &RgbImage) -> Result<()>;
    fn show_region(&mut self, image: &RgbImage, x: u16, y: u16, width: u16, height: u16) -> Result<()>;
}

const GPIO_CONSUMER: &str = "w3p-hwm";
const GPIOCHIP_LABEL_PREFIX: &str = "pinctrl-rp1";
const GPIOCHIP_FALLBACK: &str = "/dev/gpiochip0";

fn chip_label(path: &Path) -> Option<String> {
    let chip = gpiocdev::Chip::from_path(path).ok()?;
    chip.info().ok().map(|info| info.label)
}

/// Resolve the GPIO character device for the Pi header bank.
///
/// Order: `W3P_GPIOCHIP` env override (path or chip number), then the chip
/// labelled `pinctrl-rp1` (Pi 5 header bank; index moves across kernels but
/// the label is stable), then literal `/dev/gpiochip0`.
fn resolve_gpiochip() -> PathBuf {
    if let Ok(over) = std::env::var("W3P_GPIOCHIP") {
        let over = over.trim().to_owned();
        let path = if over.chars().all(|c| c.is_ascii_digit()) && !over.is_empty() {
            PathBuf::from(format!("/dev/gpiochip{over}"))
        } else {
            PathBuf::from(over)
        };
        let label = chip_label(&path);
        info!(
            path = %path.display(),
            label = label.as_deref().unwrap_or("<unknown>"),
            "GPIO chip overridden via W3P_GPIOCHIP"
        );
        if !label.as_deref().is_some_and(|l| l.starts_with(GPIOCHIP_LABEL_PREFIX)) {
            warn!(
                "W3P_GPIOCHIP chip label does not start with '{GPIOCHIP_LABEL_PREFIX}' — this may not be the 40-pin header bank"
            );
        }
        return path;
    }
    if let Ok(chips) = gpiocdev::chip::chips() {
        for path in chips {
            if let Some(label) = chip_label(&path)
                && label.starts_with(GPIOCHIP_LABEL_PREFIX)
            {
                info!(path = %path.display(), label, "Resolved GPIO chip by label");
                return path;
            }
        }
    }
    info!(
        path = GPIOCHIP_FALLBACK,
        label = chip_label(Path::new(GPIOCHIP_FALLBACK)).as_deref().unwrap_or("<unknown>"),
        "No '{GPIOCHIP_LABEL_PREFIX}' chip found, falling back"
    );
    PathBuf::from(GPIOCHIP_FALLBACK)
}

pub struct St7789Display {
    spi: Spidev,
    gpio: Request,
    width: u16,
    height: u16,
}

impl St7789Display {
    pub fn new() -> Result<Self> {
        let spi_path = format!("/dev/spidev{}.{}", config::SPI_BUS, config::SPI_CS);
        let mut spi = Spidev::open(&spi_path).with_context(|| format!("open {spi_path}"))?;
        let options = SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(config::SPI_SPEED_HZ)
            .mode(SpiModeFlags::SPI_MODE_0)
            .build();
        spi.configure(&options)?;

        let chip_path = resolve_gpiochip();
        // Plain active-high outputs; initial values match the previous rppal init:
        // RST high (panel out of reset), DC low (command), BL low (backlight off).
        let gpio = Request::builder()
            .on_chip(&chip_path)
            .with_consumer(GPIO_CONSUMER)
            .with_line(u32::from(config::PIN_DC))
            .as_output(Value::Inactive)
            .with_line(u32::from(config::PIN_RST))
            .as_output(Value::Active)
            .with_line(u32::from(config::PIN_BL))
            .as_output(Value::Inactive)
            .request()
            .with_context(|| format!("request GPIO lines on {}", chip_path.display()))?;
        Ok(Self {
            spi,
            gpio,
            width: config::ST7789_WIDTH,
            height: config::ST7789_HEIGHT,
        })
    }

    fn set_line(&mut self, pin: u8, high: bool) -> Result<()> {
        let value = if high { Value::Active } else { Value::Inactive };
        self.gpio
            .set_value(u32::from(pin), value)
            .with_context(|| format!("set GPIO line {pin}"))?;
        Ok(())
    }

    fn set_dc(&mut self, high: bool) -> Result<()> {
        self.set_line(config::PIN_DC, high)
    }

    fn set_rst(&mut self, high: bool) -> Result<()> {
        self.set_line(config::PIN_RST, high)
    }

    fn reset(&mut self) -> Result<()> {
        self.set_rst(true)?;
        thread::sleep(Duration::from_millis(10));
        self.set_rst(false)?;
        thread::sleep(Duration::from_millis(10));
        self.set_rst(true)?;
        thread::sleep(Duration::from_millis(10));
        Ok(())
    }

    fn write_cmd(&mut self, cmd: u8) -> Result<()> {
        self.set_dc(false)?;
        let tx = [cmd];
        let mut rx = [0_u8; 1];
        self.spi
            .transfer(&mut SpidevTransfer::read_write(&tx, &mut rx))?;
        Ok(())
    }

    fn write_data(&mut self, data: &[u8]) -> Result<()> {
        self.set_dc(true)?;
        self.spi.write_all(data)?;
        Ok(())
    }

    fn set_window(&mut self, xs: u16, ys: u16, xe: u16, ye: u16) -> Result<()> {
        self.write_cmd(0x2A)?;
        self.write_data(&[(xs >> 8) as u8, (xs & 0xFF) as u8, (xe >> 8) as u8, (xe & 0xFF) as u8])?;

        self.write_cmd(0x2B)?;
        let ys = ys + 20;
        let ye = ye + 20;
        self.write_data(&[(ys >> 8) as u8, (ys & 0xFF) as u8, (ye >> 8) as u8, (ye & 0xFF) as u8])?;
        self.write_cmd(0x2C)?;
        Ok(())
    }

    fn init_sequence(&mut self) -> Result<()> {
        self.write_cmd(0x36)?;
        self.write_data(&[0x00])?;
        self.write_cmd(0x3A)?;
        self.write_data(&[0x05])?;
        self.write_cmd(0xB2)?;
        self.write_data(&[0x0B, 0x0B, 0x00, 0x33, 0x35])?;
        self.write_cmd(0xB7)?;
        self.write_data(&[0x11])?;
        self.write_cmd(0xBB)?;
        self.write_data(&[0x35])?;
        self.write_cmd(0xC0)?;
        self.write_data(&[0x2C])?;
        self.write_cmd(0xC2)?;
        self.write_data(&[0x01])?;
        self.write_cmd(0xC3)?;
        self.write_data(&[0x0D])?;
        self.write_cmd(0xC4)?;
        self.write_data(&[0x20])?;
        self.write_cmd(0xC6)?;
        self.write_data(&[0x13])?;
        self.write_cmd(0xD0)?;
        self.write_data(&[0xA4, 0xA1])?;
        self.write_cmd(0xD6)?;
        self.write_data(&[0xA1])?;
        self.write_cmd(0xE0)?;
        self.write_data(&[0xF0, 0x06, 0x0B, 0x0A, 0x09, 0x26, 0x29, 0x33, 0x41, 0x18, 0x16, 0x15, 0x29, 0x2D])?;
        self.write_cmd(0xE1)?;
        self.write_data(&[0xF0, 0x04, 0x08, 0x08, 0x07, 0x03, 0x28, 0x32, 0x40, 0x3B, 0x19, 0x18, 0x2A, 0x2E])?;
        self.write_cmd(0xE4)?;
        self.write_data(&[0x25, 0x00, 0x00])?;
        self.write_cmd(0x21)?;
        self.write_cmd(0x11)?;
        thread::sleep(Duration::from_millis(100));
        self.write_cmd(0x29)?;
        Ok(())
    }
}

impl DisplayBackend for St7789Display {
    fn init(&mut self) -> Result<()> {
        self.reset()?;
        self.init_sequence()
    }

    fn clear(&mut self) -> Result<()> {
        let px = vec![0xFF_u8; self.width as usize * self.height as usize * 2];
        self.set_window(0, 0, self.width - 1, self.height - 1)?;
        self.set_dc(true)?;
        for chunk in px.chunks(4096) {
            self.spi.write_all(chunk)?;
        }
        Ok(())
    }

    fn set_backlight(&mut self, duty_percent: u8) -> Result<()> {
        self.set_line(config::PIN_BL, duty_percent > 0)
    }

    fn show_image(&mut self, image: &RgbImage) -> Result<()> {
        self.show_region(image, 0, 0, self.width, self.height)
    }

    fn show_region(&mut self, image: &RgbImage, x: u16, y: u16, width: u16, height: u16) -> Result<()> {
        if image.width() != self.width as u32 || image.height() != self.height as u32 {
            anyhow::bail!("image dimensions must be {}x{}", self.width, self.height);
        }
        if width == 0 || height == 0 {
            return Ok(());
        }

        let xe = x
            .checked_add(width - 1)
            .ok_or_else(|| anyhow::anyhow!("region x overflow"))?;
        let ye = y
            .checked_add(height - 1)
            .ok_or_else(|| anyhow::anyhow!("region y overflow"))?;
        if xe >= self.width || ye >= self.height {
            anyhow::bail!("region out of bounds");
        }

        let mut buf = Vec::with_capacity(width as usize * height as usize * 2);
        for py in y..=ye {
            for px in x..=xe {
                let pixel = image.get_pixel(px as u32, py as u32);
                let r = pixel[0];
                let g = pixel[1];
                let b = pixel[2];
                let hi = (r & 0xF8) | (g >> 5);
                let lo = ((g << 3) & 0xE0) | (b >> 3);
                buf.push(hi);
                buf.push(lo);
            }
        }

        self.write_cmd(0x36)?;
        self.write_data(&[0x00])?;
        self.set_window(x, y, xe, ye)?;
        self.set_dc(true)?;
        for chunk in buf.chunks(4096) {
            self.spi.write_all(chunk)?;
        }
        Ok(())
    }
}

pub struct MockDisplay {
    width: u32,
    height: u32,
}

impl MockDisplay {
    pub fn new() -> Self {
        Self {
            width: config::GRID_WIDTH,
            height: config::GRID_HEIGHT,
        }
    }
}

impl DisplayBackend for MockDisplay {
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        Ok(())
    }

    fn set_backlight(&mut self, _duty_percent: u8) -> Result<()> {
        Ok(())
    }

    fn show_image(&mut self, image: &RgbImage) -> Result<()> {
        if image.width() != self.width || image.height() != self.height {
            anyhow::bail!("mock image dimensions mismatch");
        }
        if !Path::new("/tmp").exists() {
            return Ok(());
        }
        Ok(())
    }

    fn show_region(&mut self, image: &RgbImage, x: u16, y: u16, width: u16, height: u16) -> Result<()> {
        if image.width() != self.width || image.height() != self.height {
            anyhow::bail!("mock image dimensions mismatch");
        }
        if width == 0 || height == 0 {
            return Ok(());
        }
        let xe = x
            .checked_add(width - 1)
            .ok_or_else(|| anyhow::anyhow!("region x overflow"))?;
        let ye = y
            .checked_add(height - 1)
            .ok_or_else(|| anyhow::anyhow!("region y overflow"))?;
        if xe as u32 >= self.width || ye as u32 >= self.height {
            anyhow::bail!("mock region out of bounds");
        }
        Ok(())
    }
}

trait SpiWriteAll {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()>;
}

impl SpiWriteAll for Spidev {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut offset = 0;
        while offset < data.len() {
            let end = (offset + 4096).min(data.len());
            let slice = &data[offset..end];
            std::io::Write::write_all(self, slice)?;
            offset = end;
        }
        Ok(())
    }
}
