#!/bin/bash
# Idempotently enable SPI in the Raspberry Pi boot config.
# Appends 'dtparam=spi=on' under an [all] section if no uncommented
# occurrence exists; never duplicates the line or touches other content.
set -euo pipefail

CONFIG="${1:-/boot/firmware/config.txt}"

if [[ $EUID -ne 0 ]]; then
    echo "This script must be run with sudo"
    exit 1
fi

if [[ ! -f "$CONFIG" ]]; then
    echo "Boot config not found: $CONFIG"
    exit 1
fi

spi_dev_present() {
    # Exact header-SPI device names (kept in sync with src/platform/checks.rs).
    # A /dev/spidev* glob would false-pass on /dev/spidev10.0 — the BCM2712
    # SoC SPI that exists out of the box on vOS but is not the 40-pin header.
    local dev
    for dev in /dev/spidev0.0 /dev/spidev0.1 /dev/spidev1.0 /dev/spidev1.1; do
        if [[ -e "$dev" ]]; then
            return 0
        fi
    done
    return 1
}

# Uncommented dtparam=spi=on (spaces around '=' allowed)?
if grep -Eq '^[[:space:]]*dtparam[[:space:]]*=[[:space:]]*spi[[:space:]]*=[[:space:]]*on([[:space:]]*(,|$))' "$CONFIG"; then
    echo "SPI already enabled in $CONFIG."
    if spi_dev_present; then
        echo "Header SPI device present — no reboot needed."
    else
        echo "No header SPI device (/dev/spidev0.0) yet — reboot required."
    fi
    exit 0
fi

printf '\n[all]\ndtparam=spi=on\n' >> "$CONFIG"
echo "Added 'dtparam=spi=on' to $CONFIG — reboot required."
