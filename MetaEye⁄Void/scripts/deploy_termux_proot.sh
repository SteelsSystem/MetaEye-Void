#!/usr/bin/env bash
set -euo pipefail

# Effortless 0-Cost On-Device Deployment for Android Termux / PRoot Ubuntu ARM64
unset ANDROID_ROOT ANDROID_DATA
export NEEDLE_TELEMETRY=0
export DO_NOT_TRACK=1
export SQLX_OFFLINE=true

if [ -d "/data/data/com.termux/files/usr/bin" ]; then
    export PATH="/data/data/com.termux/files/usr/bin:$PATH"
fi

echo "[1/3] Validating QSS-U contracts & running in-memory duplex tests..."
cargo test --release

echo "[2/3] Building stripped ARM64 binary (opt-level=z, LTO=fat)..."
cargo build --release

echo "[3/3] Starting Cactus Needle Metacognition & LiveVoice Engine on 127.0.0.1:8080..."
exec ./target/release/needle-discord-metacog
