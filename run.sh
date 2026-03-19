#!/bin/bash

echo "The script you are running has:"
echo "basename: [$(basename "$0")]"
echo "dirname : [$(dirname "$0")]"
echo "pwd     : [$(pwd)]"

DIRNAME="$(dirname "$0")"
APPLICATION="w3p-hwm"
TARGET="aarch64-unknown-linux-gnu"
BINARY_PATH="./target/$TARGET/release/$APPLICATION"

cd $DIRNAME

# Check if Cargo.toml exists
if [ ! -f "Cargo.toml" ]; then
    echo "Cargo.toml not found in the current directory."
    exit 1
fi

if ! command -v mise >/dev/null 2>&1; then
    echo "mise is required but not found in PATH."
    exit 1
fi

echo "Building Rust application..."
mise run build-aarch64 || exit 1

echo "Running application $APPLICATION..."
if [ ! -f "$BINARY_PATH" ]; then
    echo "Expected binary not found: $BINARY_PATH"
    exit 1
fi
$BINARY_PATH

echo 0
