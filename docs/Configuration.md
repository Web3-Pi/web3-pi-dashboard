# Rust Dashboard Configuration

This document describes configuration values for the Rust dashboard binary `w3p-hwm`.

## Runtime Environment Variables

These values can be changed without recompiling.

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info,w3p_hwm=debug` | Log filter. Example: `RUST_LOG=debug ./w3p-hwm` |
| `W3P_INFLUX_HOST` | `localhost` | InfluxDB host used for status queries |
| `W3P_INFLUX_PORT` | `8086` | InfluxDB HTTP port |
| `W3P_INFLUX_USERNAME` | `geth` | InfluxDB username |
| `W3P_INFLUX_PASSWORD` | `geth` | InfluxDB password |
| `W3P_INFLUX_DATABASE` | `ethonrpi` | InfluxDB database name |
| `W3P_INSTALL_GRACE_SECONDS` | `25` | Startup grace window before fallback to dashboard mode if install status log is unreadable/unparseable |

## Compile-Time Constants (in `src/app/config.rs`)

These are Rust constants and require rebuild/redeploy after changes.

### Display and Hardware

| Constant | Default | Description |
|---|---|---|
| `GRID_WIDTH` | `240` | Render width in pixels |
| `GRID_HEIGHT` | `280` | Render height in pixels |
| `ST7789_WIDTH` | `240` | ST7789 panel width |
| `ST7789_HEIGHT` | `280` | ST7789 panel height |
| `SPI_BUS` | `0` | SPI bus index (`/dev/spidev{bus}.{cs}`) |
| `SPI_CS` | `0` | SPI chip-select index |
| `SPI_SPEED_HZ` | `40000000` | SPI clock speed |
| `PIN_RST` | `27` | Display reset GPIO pin |
| `PIN_DC` | `25` | Display data/command GPIO pin |
| `PIN_BL` | `18` | Display backlight GPIO pin |
| `SHOW_PER_CORE` | `false` | CPU metric mode: `false` => 0-100%; `true` => summed per-core value |

### Paths and Assets

| Constant | Default | Description |
|---|---|---|
| `FONT_PATH` | `./font/JetBrainsMono-Medium.ttf` | Main font file |
| `BG_PATH` | `./img/lcdbg.png` | Dashboard background |
| `LOGO_PATH` | `./img/web3-pi-logo-240x70.png` | Install screen logo |
| `FINAL_LOGO_PATH` | `./img/Web3Pi_logo_0.png` | Final shutdown screen logo |
| `ANIM_DIR` | `./img/3D/` | Boot animation frames directory |
| `OPENING_FLAG_PATH` | `/root/opening.flag` | Marker controlling opening animation behavior |
| `INSTALL_STATUS_PATH` | `/opt/web3pi/status.jlog` | Install status log file read by install-state task |

### Timing and Polling

| Constant | Default | Description |
|---|---|---|
| `LOOP_FPS` | `8` | Main render loop FPS |
| `INSTALL_FPS` | `2` | Install screen update FPS |
| `ANIM_FPS` | `30` | Opening animation FPS |
| `DASH_WAVE_CYCLES_PER_SEC` | `0.35` | Dashboard sine-wave animation speed in cycles per second |
| `DASH_WAVE_AMPLITUDE_PX` | `6.0` | Dashboard sine-wave vertical amplitude in pixels |
| `DASH_WAVE_BASE_Y` | `269.0` | Dashboard sine-wave baseline Y position |
| `DASH_WAVE_THICKNESS_PX` | `2` | Dashboard sine-wave stroke thickness in pixels |
| `HIGH_TASK_INTERVAL` | `1s` | CPU/temperature update interval |
| `MEDIUM_TASK_INTERVAL` | `10s` | RAM/swap update interval |
| `LOW_TASK_INTERVAL` | `30s` | Disk/IP update interval |
| `INSTALL_TASK_INTERVAL` | `500ms` | Install status parse interval |
| `INSTALL_WARN_RATE_LIMIT` | `10s` | Minimum interval between repeated install parse warnings |
| `INSTALL_STARTUP_GRACE_DEFAULT` | `25s` | Default grace before install fallback (overridden by `W3P_INSTALL_GRACE_SECONDS`) |
| `INFLUX_FETCH_INTERVAL` | `30s` | Influx status query interval |
| `INFLUX_RETRY_BASE_SECONDS` | `10` | Base delay for Influx reconnect backoff |
| `INFLUX_TIMEOUT_SECONDS` | `3` | HTTP timeout for Influx requests |

### Colors

| Constant | Default |
|---|---|
| `COLOR_BG` | `#00129A` |
| `COLOR_TEXT_MAIN` | `#FFFFFF` |
| `COLOR_TEXT_SECONDARY` | `#A1A1A1` |
| `COLOR_GREEN` | `#22C55E` |
| `COLOR_RED` | `#EF4433` |

## Example

```bash
RUST_LOG=debug \
W3P_INFLUX_HOST=localhost \
W3P_INFLUX_PORT=8086 \
W3P_INSTALL_GRACE_SECONDS=30 \
./w3p-hwm
```
