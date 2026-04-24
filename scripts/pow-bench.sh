#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "==> core microbenchmarks (criterion)"
cargo bench -p pulsedag-core --bench pow_core -- --sample-size 20 --warm-up-time 1 --measurement-time 2

echo
echo "==> thread scaling baseline"
cargo run -p pulsedag-core --release --example pow_thread_baseline
