mod app;
mod display;
mod platform;
mod tasks;

use std::sync::Arc;

use anyhow::{Context, Result};
use app::{
    config::{self, InfluxConfig},
    state::{AppState, InstallStage, SharedState},
};
use display::{
    render::{Renderer, blank_frame},
    st7789::{DisplayBackend, MockDisplay, St7789Display},
};
use tokio::{signal, sync::RwLock, task::JoinSet, time};
use tracing::{error, info, warn};

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,w3p_hwm=debug".into()),
        )
        .init();
}

async fn preflight_checks() -> Result<()> {
    if !platform::checks::is_raspberry_pi() {
        anyhow::bail!("Only Raspberry Pi is supported");
    }
    if !platform::checks::is_spi_enabled() && !platform::checks::is_spi_enabled_config() {
        anyhow::bail!("SPI is not enabled");
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
            continue;
        }

        if snapshot.install.needs_stage_done_animation {
            play_animation(display).await?;
            let mut guard = state.write().await;
            guard.install.needs_stage_done_animation = false;
        }

        interval.tick().await;
        if cached_tick.is_multiple_of(10) {
            cached_dashboard = renderer.render_dashboard_base(&snapshot);
        }
        let mut frame = cached_dashboard.clone();
        renderer.draw_dashboard_animation(&mut frame, snapshot.ui.animation_tick);
        display.show_image(&frame)?;
        cached_tick = cached_tick.wrapping_add(1);
        let mut guard = state.write().await;
        guard.ui.animation_tick = guard.ui.animation_tick.wrapping_add(1);
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

#[tokio::main(flavor = "multi_thread")]
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

    preflight_checks().await?;

    let state: SharedState = Arc::new(RwLock::new(AppState::default()));
    let renderer = Renderer::load()?;

    let mut display: Box<dyn DisplayBackend> = match St7789Display::new() {
        Ok(d) => Box::new(d),
        Err(err) => {
            warn!("Hardware display unavailable, using mock: {err}");
            Box::new(MockDisplay::new())
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

    let influx_cfg = InfluxConfig::from_env();
    let mut set = JoinSet::new();
    set.spawn(tasks::system_metrics::high_frequency_loop(state.clone()));
    set.spawn(tasks::system_metrics::medium_frequency_loop(state.clone()));
    set.spawn(tasks::system_metrics::low_frequency_loop(state.clone()));
    set.spawn(tasks::install_stage::install_stage_loop(state.clone()));
    set.spawn(tasks::influx::influx_loop(state.clone(), influx_cfg));

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
