#!/usr/bin/env bash
set -euo pipefail

export SQLX_OFFLINE=true
export NEEDLE_TELEMETRY=0
export DO_NOT_TRACK=1

echo "=== [FLOW 1-4] Running Hermetic Unit & In-Memory Duplex Tests ==="
cargo test --all-targets --release

if command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "=== [FLOW 4] Generating LLVM LCOV Coverage Report ==="
    cargo llvm-cov --all-targets --workspace --lcov --output-path lcov.info
fi

echo "=== [FLOW 5] Generating SHA256 Provenance Manifest ==="
find contracts proto src Cargo.toml Dockerfile -type f -exec sha256sum {} + | sort -k 2 > SHA256SUMS.manifest
cat SHA256SUMS.manifest
