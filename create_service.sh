#!/bin/bash
# Install the prebuilt w3p-hwm binary + assets and register the systemd service.
# Build off-device first: mise run build-aarch64 (cargo zigbuild).
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "This script must be run with sudo"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVICE_NAME="w3p-hwm"
BIN_DEST="/usr/local/bin/w3p-hwm"
ASSET_DEST="/usr/local/share/w3p-hwm"
SERVICE_FILE="/etc/systemd/system/$SERVICE_NAME.service"

BIN_SRC=""
for candidate in "$SCRIPT_DIR/w3p-hwm" "$SCRIPT_DIR/target/aarch64-unknown-linux-gnu/release/w3p-hwm"; do
    if [[ -f "$candidate" ]]; then
        BIN_SRC="$candidate"
        break
    fi
done
if [[ -z "$BIN_SRC" ]]; then
    echo "Prebuilt binary not found (looked for ./w3p-hwm and target/aarch64-unknown-linux-gnu/release/w3p-hwm)."
    echo "Build it off-device with: mise run build-aarch64"
    exit 1
fi

install -m 755 "$BIN_SRC" "$BIN_DEST"
mkdir -p "$ASSET_DEST"
cp -r "$SCRIPT_DIR/font" "$SCRIPT_DIR/img" "$ASSET_DEST/"

# Deliberately no After=network.target: the service must come up early in
# boot; the eth/IP tasks tolerate absent network.
cat <<EOF > "$SERVICE_FILE"
[Unit]
Description=Web3 Pi LCD dashboard (w3p-hwm)
After=local-fs.target

[Service]
ExecStart=$BIN_DEST
Environment=W3P_ASSET_DIR=$ASSET_DEST
StateDirectory=w3p-hwm
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now "$SERVICE_NAME.service"

echo "Installed $BIN_DEST, assets in $ASSET_DEST."
echo "The service $SERVICE_NAME has been created and started."
