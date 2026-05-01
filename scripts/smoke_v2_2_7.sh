#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
