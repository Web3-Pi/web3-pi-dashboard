#!/bin/bash
# Thin dev-runner: executes the prebuilt binary with assets from this directory.
# No on-device compilation; build off-device with: mise run build-aarch64
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

BIN=""
for candidate in "$SCRIPT_DIR/w3p-hwm" "$SCRIPT_DIR/target/aarch64-unknown-linux-gnu/release/w3p-hwm"; do
    if [[ -f "$candidate" ]]; then
        BIN="$candidate"
        break
    fi
done
if [[ -z "$BIN" ]]; then
    echo "Prebuilt binary not found (looked for ./w3p-hwm and target/aarch64-unknown-linux-gnu/release/w3p-hwm)."
    echo "Build it off-device with: mise run build-aarch64"
    exit 1
fi

export W3P_ASSET_DIR="$SCRIPT_DIR"
exec "$BIN"
