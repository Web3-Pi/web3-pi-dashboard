mod app;
mod display;
mod platform;
mod tasks;

use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use app::{
    config::{self, EthStatusConfig},
    state::{AppState, InstallStage, SharedState},
};
use display::{
    render::{Renderer, blank_frame},
    st7789::{DisplayBackend, MockDisplay, St7789Display},
};
use image::RgbImage;
use tokio::{signal, sync::RwLock, task::JoinSet, time};
use tracing::{debug, error, info, warn};

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,w3p_hwm=debug".into()),
        )
        .init();
}

fn diff_bbox(prev: &RgbImage, curr: &RgbImage) -> Option<(u16, u16, u16, u16)> {
    if prev.dimensions() != curr.dimensions() {
        return None;
    }
    let width = curr.width() as usize;
    let mut min_x = width;
    let mut min_y = curr.height() as usize;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut changed = false;

    for (idx, (a, b)) in prev.pixels().zip(curr.pixels()).enumerate() {
        if a != b {
            changed = true;
            let x = idx % width;
            let y = idx / width;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if !changed {
        return None;
    }

    Some((
        min_x as u16,
        min_y as u16,
        (max_x - min_x + 1) as u16,
        (max_y - min_y + 1) as u16,
    ))
}

fn mock_display_requested() -> bool {
    std::env::var("W3P_MOCK_DISPLAY").is_ok_and(|v| v == "1")
}

async fn preflight_checks() -> Result<()> {
    if !platform::checks::is_raspberry_pi() {
        anyhow::bail!("Only Raspberry Pi is supported (set W3P_MOCK_DISPLAY=1 for desktop development)");
    }
    if !platform::checks::is_spi_enabled() && !platform::checks::is_spi_enabled_config() {
        anyhow::bail!(
            "SPI disabled — add 'dtparam=spi=on' to /boot/firmware/config.txt ([all] section) and reboot (see scripts/enable-spi.sh)"
        );
    }
    Ok(())
}

async fn play_animation<D: DisplayBackend + ?Sized>(display: &mut D) -> Result<()> {
    for frame_path in Renderer::animation_frames() {
        let frame = Renderer::load_frame(&frame_path)?;
        if frame.width() != config::GRID_WIDTH || frame.height() != config::GRID_HEIGHT {
            continue;
        }
        display.show_image(&frame)?;
        time::sleep(Renderer::frame_delay()).await;
    }
    time::sleep(time::Duration::from_secs(1)).await;
    Ok(())
}

async fn display_final_screen<D: DisplayBackend + ?Sized>(display: &mut D) {
    match Renderer::final_screen() {
        Ok(img) => {
            if let Err(err) = display.show_image(&img) {
                error!("Display final screen error: {err}");
            }
        }
        Err(err) => error!("Prepare final screen failed: {err}"),
    }
}

async fn render_loop<D: DisplayBackend + ?Sized>(
    display: &mut D,
    renderer: &Renderer,
    state: SharedState,
) -> Result<()> {
    let mut interval = time::interval(time::Duration::from_millis(1000 / config::LOOP_FPS));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut install_interval = time::interval(time::Duration::from_millis(1000 / config::INSTALL_FPS));
    install_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    let mut cached_dashboard = blank_frame();
    let mut cached_tick = 0_u64;
    let mut blink = false;
    let animation_started_at = Instant::now();
    let mut prev_dashboard_frame: Option<RgbImage> = None;
    loop {
        let snapshot = { state.read().await.clone() };
        if snapshot.install.stage != InstallStage::Done {
            install_interval.tick().await;
            let img = renderer.render_install_screen(&snapshot, blink);
            blink = !blink;
            display.show_image(&img)?;
            let mut guard = state.write().await;
            guard.advance_spinner();
            guard.ui.animation_tick = guard.ui.animation_tick.wrapping_add(1);
            prev_dashboard_frame = None;
            continue;
        }

        if snapshot.install.needs_stage_done_animation {
            play_animation(display).await?;
            let mut guard = state.write().await;
            guard.install.needs_stage_done_animation = false;
            prev_dashboard_frame = None;
        }

        interval.tick().await;
        if cached_tick.is_multiple_of(config::LOOP_FPS) {
            cached_dashboard = renderer.render_dashboard_base(&snapshot);
        }

        let mut frame = cached_dashboard.clone();
        renderer.draw_dashboard_animation(&mut frame, animation_started_at.elapsed().as_secs_f32());

        match prev_dashboard_frame.as_ref().and_then(|prev| diff_bbox(prev, &frame)) {
            Some((x, y, width, height)) => {
                let ts = Instant::now();
                display.show_region(&frame, x, y, width, height)?;
                debug!(
                    x,
                    y,
                    width,
                    height,
                    elapsed = ?ts.elapsed(),
                    "show region"
                );
            }
            None if prev_dashboard_frame.is_some() => {}
            None => {
                let ts = Instant::now();
                display.show_image(&frame)?;
                debug!(elapsed = ?ts.elapsed(), "show image");
            }
        }
        prev_dashboard_frame = Some(frame);

        cached_tick = cached_tick.wrapping_add(1);
        let mut guard = state.write().await;
        guard.ui.animation_tick = guard.ui.animation_tick.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::diff_bbox;

    #[test]
    fn diff_bbox_none_when_equal() {
        let a = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        let b = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        assert_eq!(diff_bbox(&a, &b), None);
    }

    #[test]
    fn diff_bbox_single_pixel() {
        let a = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        let mut b = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        b.put_pixel(2, 1, Rgb([1, 2, 3]));
        assert_eq!(diff_bbox(&a, &b), Some((2, 1, 1, 1)));
    }

    #[test]
    fn diff_bbox_multi_pixel_bounds() {
        let a = ImageBuffer::from_pixel(6, 5, Rgb([0, 0, 0]));
        let mut b = ImageBuffer::from_pixel(6, 5, Rgb([0, 0, 0]));
        b.put_pixel(1, 2, Rgb([1, 2, 3]));
        b.put_pixel(4, 4, Rgb([4, 5, 6]));
        assert_eq!(diff_bbox(&a, &b), Some((1, 2, 4, 3)));
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = signal::unix::signal(signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = signal::ctrl_c() => {},
            _ = async {
                if let Some(t) = term.as_mut() {
                    t.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    init_logging();
    info!("Hardware Monitor Start");
    info!(
        status_path = config::INSTALL_STATUS_PATH,
        install_grace_seconds = config::install_startup_grace().as_secs(),
        install_warn_rate_limit_seconds = config::INSTALL_WARN_RATE_LIMIT.as_secs(),
        rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "<not-set>".to_owned()),
        "Runtime install-status diagnostics"
    );

    if !mock_display_requested() {
        preflight_checks().await?;
    }

    let state: SharedState = Arc::new(RwLock::new(AppState::default()));
    let renderer = Renderer::load()?;

    let mut display: Box<dyn DisplayBackend> = if mock_display_requested() {
        info!("W3P_MOCK_DISPLAY=1 set, using mock display");
        Box::new(MockDisplay::new())
    } else {
        match St7789Display::new() {
            Ok(d) => Box::new(d),
            Err(err) => {
                error!("Hardware display init failed: {err:#}");
                return Err(err.context("hardware display init (set W3P_MOCK_DISPLAY=1 for a mock display)"));
            }
        }
    };

    display.init().context("display init")?;
    display.clear().context("display clear")?;
    display.set_backlight(100).context("set backlight")?;

    if Renderer::opening_needed() {
        if let Err(err) = play_animation(display.as_mut()).await {
            warn!("Opening animation failed: {err}");
        }
    } else {
        Renderer::create_opening_flag();
    }

    let eth_cfg = EthStatusConfig::from_env();
    let mut set = JoinSet::new();
    set.spawn(tasks::system_metrics::high_frequency_loop(state.clone()));
    set.spawn(tasks::system_metrics::medium_frequency_loop(state.clone()));
    set.spawn(tasks::system_metrics::low_frequency_loop(state.clone()));
    set.spawn(tasks::install_stage::install_stage_loop(state.clone()));
    set.spawn(tasks::eth_status::eth_status_loop(state.clone(), eth_cfg));

    {
        let render_future = render_loop(display.as_mut(), &renderer, state.clone());
        tokio::pin!(render_future);
        tokio::select! {
            _ = shutdown_signal() => {
                info!("Shutdown signal received");
            }
            res = &mut render_future => {
                match res {
                    Ok(()) => info!("Render loop exited"),
                    Err(err) => error!("Render loop error: {err}"),
                }
            }
            joined = set.join_next() => {
                if let Some(res) = joined {
                    match res {
                        Ok(Ok(())) => info!("Task ended"),
                        Ok(Err(err)) => error!("Task error: {err}"),
                        Err(err) => error!("Task panic: {err}"),
                    }
                }
            }
        }
    }

    set.abort_all();
    display_final_screen(display.as_mut()).await;
    info!("Hardware Monitor End");
    Ok(())
}
