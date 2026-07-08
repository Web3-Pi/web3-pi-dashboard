use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use ab_glyph::{FontArc, PxScale};
use anyhow::{Context, Result};
use chrono::Local;
use image::{ImageBuffer, Rgb, RgbImage, imageops};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut, text_size};

use crate::app::{
    config,
    state::{AppState, InstallStage},
};

pub struct FontSet {
    pub l: PxScale,
    pub m: PxScale,
    pub s: PxScale,
    pub xs: PxScale,
}

pub struct Renderer {
    font: FontArc,
    fontset: FontSet,
    bg_template: RgbImage,
    logo: RgbImage,
}

impl Renderer {
    pub fn load() -> Result<Self> {
        let font_bytes = fs::read(config::FONT_PATH).context("read font")?;
        let font = FontArc::try_from_vec(font_bytes).context("parse font")?;
        let bg_template = image::open(config::BG_PATH)
            .context("load background")?
            .resize_exact(
                config::GRID_WIDTH,
                config::GRID_HEIGHT,
                imageops::FilterType::Nearest,
            )
            .to_rgb8();
        let logo = image::open(config::LOGO_PATH).context("load logo")?.to_rgb8();
        Ok(Self {
            font,
            fontset: FontSet {
                l: PxScale::from(35.0),
                m: PxScale::from(25.0),
                s: PxScale::from(20.0),
                xs: PxScale::from(18.0),
            },
            bg_template,
            logo,
        })
    }

    pub fn render_install_screen(&self, state: &AppState, blink_red: bool) -> RgbImage {
        let mut img = self.bg_template.clone();
        imageops::replace(&mut img, &self.logo, 0, 25);
        let secondary = hex(config::COLOR_TEXT_SECONDARY);
        let white = hex(config::COLOR_TEXT_MAIN);
        let green = hex(config::COLOR_GREEN);
        let red = if blink_red { hex(config::COLOR_RED) } else { white };

        match state.install.stage {
            InstallStage::Stage0 => {
                self.stage_line(
                    &mut img,
                    10,
                    95,
                    "Stage 0:",
                    if state.install.stage0_error {
                        "ERROR"
                    } else {
                        state.ui.spinner.as_str()
                    },
                    if state.install.stage0_error { red } else { secondary },
                );
                self.center_text(&mut img, 120, 135, &state.install.status_short, self.fontset.s, white);
            }
            InstallStage::Stage1 => {
                self.stage_line(
                    &mut img,
                    10,
                    95,
                    "Stage 0:",
                    if state.install.stage0_error {
                        "ERROR"
                    } else {
                        "DONE"
                    },
                    if state.install.stage0_error { red } else { green },
                );
                self.stage_line(
                    &mut img,
                    10,
                    130,
                    "Stage 1:",
                    if state.install.stage1_error {
                        "ERROR"
                    } else {
                        state.ui.spinner.as_str()
                    },
                    if state.install.stage1_error { red } else { secondary },
                );
                self.center_text(&mut img, 120, 170, &state.install.status_short, self.fontset.s, white);
            }
            InstallStage::Stage2 => {
                self.stage_line(
                    &mut img,
                    10,
                    95,
                    "Stage 0:",
                    if state.install.stage0_error {
                        "ERROR"
                    } else {
                        "DONE"
                    },
                    if state.install.stage0_error { red } else { green },
                );
                self.stage_line(
                    &mut img,
                    10,
                    130,
                    "Stage 1:",
                    if state.install.stage1_error {
                        "ERROR"
                    } else {
                        "DONE"
                    },
                    if state.install.stage1_error { red } else { green },
                );
                self.stage_line(
                    &mut img,
                    10,
                    165,
                    "Stage 2:",
                    if state.install.stage2_error {
                        "ERROR"
                    } else {
                        state.ui.spinner.as_str()
                    },
                    if state.install.stage2_error { red } else { secondary },
                );
                self.center_text(&mut img, 120, 205, &state.install.status_short, self.fontset.s, white);
            }
            _ => {
                self.center_text(&mut img, 120, 135, "Starting...", self.fontset.s, white);
            }
        }

        let dt = Local::now().format("%d.%m.%y %H:%M:%S").to_string();
        self.center_text(&mut img, 120, 10, &dt, self.fontset.xs, secondary);
        let info_color = if state.install.any_error { red } else { white };
        self.center_text(&mut img, 120, 235, "For more info visit:", self.fontset.xs, info_color);
        if let Some(ip) = &state.system.ip_local_address {
            self.center_text(&mut img, 120, 255, &format!("http://{ip}"), self.fontset.xs, white);
        }
        img
    }

    pub fn render_dashboard_base(&self, state: &AppState) -> RgbImage {
        let mut img = self.bg_template.clone();
        let secondary = hex(config::COLOR_TEXT_SECONDARY);
        let white = hex(config::COLOR_TEXT_MAIN);

        draw_line_segment_mut(&mut img, (80.0, 0.0), (80.0, 186.0), Rgb([0, 0, 0]));
        draw_line_segment_mut(&mut img, (160.0, 0.0), (160.0, 186.0), Rgb([0, 0, 0]));
        draw_line_segment_mut(&mut img, (0.0, 93.0), (240.0, 93.0), Rgb([0, 0, 0]));
        draw_line_segment_mut(&mut img, (0.0, 186.0), (240.0, 186.0), Rgb([0, 0, 0]));

        self.center_text(&mut img, 120, 108, "CPU", self.fontset.m, secondary);
        let cpu_label = format!("{}", state.system.cpu_percent as i32);
        let cpu_color = if state.show_per_core {
            value_to_hex_color_cpu_usage_400(state.system.cpu_percent as i32)
        } else {
            value_to_hex_color_cpu_usage(state.system.cpu_percent as i32)
        };
        self.center_text(&mut img, 120, 140, &cpu_label, self.fontset.l, cpu_color);
        self.center_text(&mut img, 150, if state.show_per_core { 108 } else { 145 }, "%", self.fontset.s, secondary);
        self.center_text(
            &mut img,
            122,
            170,
            &format!("{}°C", state.system.cpu_temp as i32),
            self.fontset.m,
            secondary,
        );

        self.center_text(&mut img, 40, 108, "DISK", self.fontset.m, secondary);
        self.center_text(
            &mut img,
            40,
            140,
            &format!("{}%", state.system.disk_percent as i32),
            self.fontset.l,
            white,
        );
        self.center_text(
            &mut img,
            40,
            170,
            &format!("{:.1}T used", state.system.disk_used_tb),
            self.fontset.xs,
            secondary,
        );

        self.center_text(&mut img, 40, 18, "EXEC", self.fontset.m, secondary);
        let exec_s = state.chain.exec;
        self.center_text(&mut img, 40, 50, exec_s.as_label(), self.fontset.xs, Rgb(exec_s.as_color()));
        self.center_text(&mut img, 120, 18, "NODE", self.fontset.m, secondary);
        let node_s = state.chain.node;
        self.center_text(&mut img, 120, 50, node_s.as_label(), self.fontset.xs, Rgb(node_s.as_color()));
        self.center_text(&mut img, 200, 18, "CONS", self.fontset.m, secondary);
        let cons_s = state.chain.cons;
        self.center_text(&mut img, 200, 50, cons_s.as_label(), self.fontset.xs, Rgb(cons_s.as_color()));

        self.center_text(&mut img, 200, 108, "RAM", self.fontset.m, secondary);
        self.center_text(
            &mut img,
            200,
            140,
            &format!("{}", state.system.mem_percent as i32),
            self.fontset.l,
            white,
        );
        self.center_text(&mut img, 225, 170, "%", self.fontset.m, secondary);

        self.center_text(&mut img, 120, 203, "IP / HOSTNAME", self.fontset.m, secondary);
        self.center_text(
            &mut img,
            120,
            230,
            state
                .system
                .ip_local_address
                .as_deref()
                .unwrap_or(""),
            self.fontset.s,
            white,
        );
        self.center_text(
            &mut img,
            120,
            255,
            &format!("{}.local", state.system.hostname),
            self.fontset.s,
            white,
        );

        img
    }

    pub fn draw_dashboard_animation(&self, image: &mut RgbImage, elapsed_secs: f32) {
        let width = config::GRID_WIDTH as i32;
        let base_y = config::DASH_WAVE_BASE_Y;
        let phase = elapsed_secs * std::f32::consts::TAU * config::DASH_WAVE_CYCLES_PER_SEC;
        for x in (0..width).step_by(4) {
            let x1 = x as f32;
            let x2 = (x + 4).min(width - 1) as f32;
            let y1 = base_y + ((x as f32 * 0.05 + phase).sin() * config::DASH_WAVE_AMPLITUDE_PX);
            let y2 =
                base_y + (((x + 4) as f32 * 0.05 + phase).sin() * config::DASH_WAVE_AMPLITUDE_PX);
            for offset in 0..config::DASH_WAVE_THICKNESS_PX {
                let dy = offset as f32;
                draw_line_segment_mut(image, (x1, y1 + dy), (x2, y2 + dy), Rgb([0, 255, 0]));
            }
        }
    }

    pub fn final_screen() -> Result<RgbImage> {
        let mut img = image::open(config::FINAL_LOGO_PATH)
            .context("load final logo")?
            .to_rgb8();
        for pixel in img.pixels_mut() {
            let inv = [255 - pixel[0], 255 - pixel[1], 255 - pixel[2]];
            let gray = ((inv[0] as f32 * 0.299 + inv[1] as f32 * 0.587 + inv[2] as f32 * 0.114)
                * 0.15) as u8;
            *pixel = Rgb([gray, gray, gray]);
        }
        Ok(img)
    }

    pub fn opening_needed() -> bool {
        Path::new(config::OPENING_FLAG_PATH).exists()
    }

    pub fn create_opening_flag() {
        let _ = fs::write(config::OPENING_FLAG_PATH, []);
    }

    pub fn animation_frames() -> Vec<PathBuf> {
        let mut files = fs::read_dir(config::ANIM_DIR)
            .ok()
            .into_iter()
            .flat_map(|v| v.flatten())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("png"))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    pub fn load_frame(path: &Path) -> Result<RgbImage> {
        Ok(image::open(path)?.to_rgb8())
    }

    pub fn frame_delay() -> Duration {
        Duration::from_millis(1000 / config::ANIM_FPS)
    }

    fn stage_line(
        &self,
        img: &mut RgbImage,
        x: i32,
        y: i32,
        prefix: &str,
        value: &str,
        color: Rgb<u8>,
    ) {
        let text = format!("{prefix} {value}");
        draw_text_mut(img, color, x, y, self.fontset.m, &self.font, &text);
    }

    fn center_text(
        &self,
        img: &mut RgbImage,
        x: i32,
        y: i32,
        text: &str,
        scale: PxScale,
        color: Rgb<u8>,
    ) {
        let (w, h) = text_size(scale, &self.font, text);
        let tx = x - (w as i32 / 2);
        let ty = y - (h as i32 / 2);
        draw_text_mut(img, color, tx, ty, scale, &self.font, text);
    }
}

pub fn hex(value: &str) -> Rgb<u8> {
    let c = value.trim_start_matches('#');
    if c.len() != 6 {
        return Rgb([255, 255, 255]);
    }
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(255);
    Rgb([parse(&c[0..2]), parse(&c[2..4]), parse(&c[4..6])])
}

pub fn value_to_hex_color_cpu_usage(value: i32) -> Rgb<u8> {
    if !(0..=100).contains(&value) {
        return hex(config::COLOR_BG);
    }
    lerp_rgb(value, 50, 100)
}

pub fn value_to_hex_color_cpu_usage_400(value: i32) -> Rgb<u8> {
    if !(0..=400).contains(&value) {
        return hex(config::COLOR_BG);
    }
    let scaled = value / 4;
    lerp_rgb(scaled, 50, 100)
}

fn lerp_rgb(value: i32, yellow_at: i32, red_at: i32) -> Rgb<u8> {
    let green = [0_f32, 255_f32, 0_f32];
    let yellow = [255_f32, 255_f32, 0_f32];
    let red = [255_f32, 0_f32, 0_f32];
    let (a, b, ratio) = if value <= yellow_at {
        (
            green,
            yellow,
            value as f32 / yellow_at.max(1) as f32,
        )
    } else {
        (
            yellow,
            red,
            (value - yellow_at) as f32 / (red_at - yellow_at).max(1) as f32,
        )
    };
    let r = a[0] + (b[0] - a[0]) * ratio;
    let g = a[1] + (b[1] - a[1]) * ratio;
    let b = a[2] + (b[2] - a[2]) * ratio;
    Rgb([r as u8, g as u8, b as u8])
}

pub fn blank_frame() -> RgbImage {
    ImageBuffer::from_pixel(config::GRID_WIDTH, config::GRID_HEIGHT, hex(config::COLOR_BG))
}

#[cfg(test)]
mod tests {
    use super::{value_to_hex_color_cpu_usage, value_to_hex_color_cpu_usage_400};

    #[test]
    fn cpu_color_ranges() {
        let low = value_to_hex_color_cpu_usage(0);
        let high = value_to_hex_color_cpu_usage(100);
        assert!(low[1] >= low[0]);
        assert!(high[0] >= high[1]);
    }

    #[test]
    fn cpu_color_400_ranges() {
        let low = value_to_hex_color_cpu_usage_400(0);
        let high = value_to_hex_color_cpu_usage_400(400);
        assert!(low[1] >= low[0]);
        assert!(high[0] >= high[1]);
    }
}
