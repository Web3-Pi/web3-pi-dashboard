use std::{path::Path, thread, time::Duration};

use anyhow::{Context, Result};
use image::RgbImage;
use rppal::gpio::{Gpio, OutputPin};
use spidev::{SpiModeFlags, Spidev, SpidevOptions, SpidevTransfer};

use crate::app::config;

pub trait DisplayBackend {
    fn init(&mut self) -> Result<()>;
    fn clear(&mut self) -> Result<()>;
    fn set_backlight(&mut self, duty_percent: u8) -> Result<()>;
    fn show_image(&mut self, image: &RgbImage) -> Result<()>;
}

pub struct St7789Display {
    spi: Spidev,
    dc: OutputPin,
    rst: OutputPin,
    bl: OutputPin,
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

        let gpio = Gpio::new().context("initialize GPIO")?;
        let dc = gpio.get(config::PIN_DC)?.into_output_low();
        let rst = gpio.get(config::PIN_RST)?.into_output_high();
        let bl = gpio.get(config::PIN_BL)?.into_output_low();
        Ok(Self {
            spi,
            dc,
            rst,
            bl,
            width: config::ST7789_WIDTH,
            height: config::ST7789_HEIGHT,
        })
    }

    fn reset(&mut self) {
        self.rst.set_high();
        thread::sleep(Duration::from_millis(10));
        self.rst.set_low();
        thread::sleep(Duration::from_millis(10));
        self.rst.set_high();
        thread::sleep(Duration::from_millis(10));
    }

    fn write_cmd(&mut self, cmd: u8) -> Result<()> {
        self.dc.set_low();
        let tx = [cmd];
        let mut rx = [0_u8; 1];
        self.spi
            .transfer(&mut SpidevTransfer::read_write(&tx, &mut rx))?;
        Ok(())
    }

    fn write_data(&mut self, data: &[u8]) -> Result<()> {
        self.dc.set_high();
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
        self.reset();
        self.init_sequence()
    }

    fn clear(&mut self) -> Result<()> {
        let px = vec![0xFF_u8; self.width as usize * self.height as usize * 2];
        self.set_window(0, 0, self.width - 1, self.height - 1)?;
        self.dc.set_high();
        for chunk in px.chunks(4096) {
            self.spi.write_all(chunk)?;
        }
        Ok(())
    }

    fn set_backlight(&mut self, duty_percent: u8) -> Result<()> {
        if duty_percent > 0 {
            self.bl.set_high();
        } else {
            self.bl.set_low();
        }
        Ok(())
    }

    fn show_image(&mut self, image: &RgbImage) -> Result<()> {
        if image.width() != self.width as u32 || image.height() != self.height as u32 {
            anyhow::bail!("image dimensions must be {}x{}", self.width, self.height);
        }
        let mut buf = Vec::with_capacity((self.width as usize) * (self.height as usize) * 2);
        for pixel in image.pixels() {
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];
            let hi = (r & 0xF8) | (g >> 5);
            let lo = ((g << 3) & 0xE0) | (b >> 3);
            buf.push(hi);
            buf.push(lo);
        }

        self.write_cmd(0x36)?;
        self.write_data(&[0x00])?;
        self.set_window(0, 0, self.width - 1, self.height - 1)?;
        self.dc.set_high();
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
