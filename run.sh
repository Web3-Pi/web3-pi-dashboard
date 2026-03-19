#!/bin/bash

echo "The script you are running has:"
echo "basename: [$(basename "$0")]"
echo "dirname : [$(dirname "$0")]"
echo "pwd     : [$(pwd)]"

DIRNAME="$(dirname "$0")"
APPLICATION="w3p-hwm"

cd $DIRNAME

# Check if Cargo.toml exists
if [ ! -f "Cargo.toml" ]; then
    echo "Cargo.toml not found in the current directory."
    exit 1
fi

echo "Building Rust application..."
cargo build --release --bin $APPLICATION || exit 1

echo "Running application $APPLICATION..."
./target/release/$APPLICATION

echo 0
