#!/bin/bash
set -e

echo "Building hl binary for linux/amd64..."

docker build --platform linux/amd64 -t hl-builder .

# Create a temporary container
echo "Extracting binary..."
docker create --name hl-temp --platform linux/amd64 hl-builder

mkdir -p dist/linux-amd64-glibc

# Copy the binary out
docker cp hl-temp:/app/hl ./dist/linux-amd64-glibc/hl

# Clean up the temporary container
docker rm hl-temp

echo "✓ Binary built successfully: dist/linux-amd64-glibc/hl"
