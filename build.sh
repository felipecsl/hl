#!/bin/bash
set -euo pipefail

echo "Building hl in release mode..."
cargo build --release

echo ""
echo "Binary size:"
ls -lh target/release/hl

echo ""
echo "Binary location: target/release/hl"
echo ""
echo "To install locally:"
echo "  cp target/release/hl /usr/local/bin/hl"
