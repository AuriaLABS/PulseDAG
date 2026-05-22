#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 --archive <archive> [--checksum-file <archive.sha256>] [--health-check] [--timeout-secs 10]"
}

archive=""
checksum_file=""
health_check="false"
timeout_secs=10

while [[ $# -gt 0 ]]; do
  case "$1" in
    --archive) archive="$2"; shift 2 ;;
    --checksum-file) checksum_file="$2"; shift 2 ;;
    --health-check) health_check="true"; shift ;;
    --timeout-secs) timeout_secs="$2"; shift 2 ;;
    *) usage; exit 1 ;;
  esac
done

[[ -n "$archive" ]] || { usage; exit 1; }
[[ -f "$archive" ]] || { echo "archive not found: $archive"; exit 1; }
if [[ -z "$checksum_file" ]]; then checksum_file="${archive}.sha256"; fi
[[ -f "$checksum_file" ]] || { echo "checksum file not found: $checksum_file"; exit 1; }

sha256sum -c "$checksum_file"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

if [[ "$archive" == *.zip ]]; then
  unzip -q "$archive" -d "$tmpdir"
else
  tar -xzf "$archive" -C "$tmpdir"
fi

root_dir="$(find "$tmpdir" -mindepth 1 -maxdepth 1 -type d | head -n1)"
[[ -n "$root_dir" ]] || { echo "archive missing root dir"; exit 1; }

binary=""
for candidate in pulsedagd pulsedagd.exe pulsedag-miner pulsedag-miner.exe; do
  if [[ -f "$root_dir/$candidate" ]]; then binary="$root_dir/$candidate"; break; fi
done
[[ -n "$binary" ]] || { echo "binary not found in archive"; exit 1; }

timeout "$timeout_secs" "$binary" --version || { echo "timed out or failed: --version"; exit 1; }
timeout "$timeout_secs" "$binary" --help || { echo "timed out or failed: --help"; exit 1; }

if [[ "$health_check" == "true" && "$binary" == *pulsedagd* ]]; then
  "$binary" --help >/dev/null
fi

echo "install verification passed for $archive"
