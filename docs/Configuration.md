# Rust Dashboard Configuration

This document describes configuration values for the Rust dashboard binary `w3p-hwm`.

## Runtime Environment Variables

These values can be changed without recompiling.

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info,w3p_hwm=debug` | Log filter. Example: `RUST_LOG=debug ./w3p-hwm` |
| `W3P_GPIOCHIP` | auto | GPIO character device override: a path (`/dev/gpiochip0`) or a chip number (`0`). Without it the chip labelled `pinctrl-rp1` is used, falling back to `/dev/gpiochip0` |
| `W3P_MOCK_DISPLAY` | unset | `1` uses a mock display instead of the ST7789 hardware (dev/desktop use; also skips the Raspberry Pi / SPI preflight checks). Without it a display init failure is fatal |
| `W3P_ASSET_DIR` | executable's directory | Root directory for `font/` and `img/` assets (the systemd unit sets `/usr/local/share/w3p-hwm`) |
| `W3P_GETH_RPC` | `http://127.0.0.1:8545` | geth JSON-RPC endpoint (EXEC tile) |
| `W3P_BEACON_REST` | `http://127.0.0.1:5052` | Beacon node REST endpoint (CONS tile) |
| `W3P_UNIT_EXEC` | `geth.service` | systemd unit checked for the EXEC tile |
| `W3P_UNIT_CONS` | `nimbus-beacon-node.service` | systemd unit checked for the CONS tile |
| `W3P_ETH_POLL_SECONDS` | `10` | Ethereum status poll interval in seconds |
| `W3P_INSTALL_STATUS_PATH` | `/opt/web3pi/status.jlog` | Install status JSON-lines log. When absent, `/root/.install_stage` (vOS plain integer: `0` = installing, `>= 1` = done) is used; when neither exists the dashboard starts immediately |
| `W3P_INSTALL_GRACE_SECONDS` | `25` | Startup grace window before fallback to dashboard mode while a status file exists but is unreadable/unparseable |
| `STATE_DIRECTORY` | `/var/lib/w3p-hwm` | Provided by systemd (`StateDirectory=w3p-hwm`); holds `opening.flag` which gates the opening animation |

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
| `PIN_RST` | `27` | Display reset GPIO line offset (BCM) |
| `PIN_DC` | `25` | Display data/command GPIO line offset (BCM) |
| `PIN_BL` | `18` | Display backlight GPIO line offset (BCM) |
| `SHOW_PER_CORE` | `false` | CPU metric mode: `false` => 0-100%; `true` => summed per-core value |

### Paths and Assets

Asset paths (font, background, logos, animation frames) derive from the asset
root — `W3P_ASSET_DIR`, else the executable's directory, else the current
working directory:

| Asset | Relative path |
|---|---|
| Font | `font/JetBrainsMono-Medium.ttf` |
| Dashboard background | `img/lcdbg.png` |
| Install screen logo | `img/web3-pi-logo-240x70.png` |
| Final shutdown screen logo | `img/Web3Pi_logo_0.png` |
| Boot animation frames | `img/3D/` (missing directory skips the animation) |

| Constant | Default | Description |
|---|---|---|
| `INSTALL_STATUS_PATH_DEFAULT` | `/opt/web3pi/status.jlog` | Default install status log (see `W3P_INSTALL_STATUS_PATH`) |
| `VOS_INSTALL_STAGE_PATH` | `/root/.install_stage` | vOS plain-integer install stage file (`0` = installing, `>= 1` = done) |
| `STATE_DIR_DEFAULT` | `/var/lib/w3p-hwm` | State directory fallback when `STATE_DIRECTORY` is unset |

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
| `ETH_POLL_SECONDS_DEFAULT` | `10` | Default eth status poll interval (overridden by `W3P_ETH_POLL_SECONDS`) |
| `ETH_HTTP_TIMEOUT_SECONDS` | `3` | HTTP timeout for geth RPC / beacon REST requests |

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
W3P_GETH_RPC=http://127.0.0.1:8545 \
W3P_ETH_POLL_SECONDS=10 \
W3P_INSTALL_GRACE_SECONDS=30 \
./w3p-hwm
```
