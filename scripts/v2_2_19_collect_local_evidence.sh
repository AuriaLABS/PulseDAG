#!/usr/bin/env bash
set -euo pipefail
RUN_DIR=${1:-}
[[ -n "$RUN_DIR" && -d "$RUN_DIR" ]] || { echo "Usage: $0 <run_dir>"; exit 1; }

manifest_files=(
  summary.md
  command-log.txt
  process-pids.txt
  endpoints-manifest.txt
  node-height-summary.json
  miner-submit-summary.json
  readiness-summary.json
  release-summary.json
  manifest.txt
)

for f in "${manifest_files[@]}"; do
  [[ -f "$RUN_DIR/$f" ]] || { echo "FAIL missing evidence file: $f"; exit 1; }
done

( cd "$RUN_DIR" && tar -czf evidence.tar.gz "${manifest_files[@]}" endpoints logs )
( cd "$RUN_DIR" && sha256sum evidence.tar.gz > evidence.tar.gz.sha256 )
echo "Evidence packaged in $RUN_DIR"
