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
    compgen -G "/dev/spidev*" > /dev/null
}

# Uncommented dtparam=spi=on (spaces around '=' allowed)?
if grep -Eq '^[[:space:]]*dtparam[[:space:]]*=[[:space:]]*spi[[:space:]]*=[[:space:]]*on([[:space:]]*(,|$))' "$CONFIG"; then
    echo "SPI already enabled in $CONFIG."
    if spi_dev_present; then
        echo "SPI device present — no reboot needed."
    else
        echo "No /dev/spidev* yet — reboot required."
    fi
    exit 0
fi

printf '\n[all]\ndtparam=spi=on\n' >> "$CONFIG"
echo "Added 'dtparam=spi=on' to $CONFIG — reboot required."
