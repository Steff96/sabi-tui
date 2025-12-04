#!/bin/bash
# Dev install - build and install sabi from source

set -e

cargo build --release
cp target/release/sabi ~/.local/bin/

echo "✓ Installed $(./target/release/sabi --version 2>/dev/null || echo 'dev') to ~/.local/bin/"
