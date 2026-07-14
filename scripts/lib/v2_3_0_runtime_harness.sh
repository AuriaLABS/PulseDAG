#!/usr/bin/env bash
# Shared runtime harness helpers for v2.3.0 local five-node drills.

pulsedag_repo_root() { git rev-parse --show-toplevel; }

pulsedag_sha256_file() {
  local file="$1"
  sha256sum "$file" | awk '{print $1}'
}

pulsedag_wait_http_ok() {
  local url="$1" out="$2" timeout="${3:-60}" start
  start=$(date +%s)
  while (( $(date +%s) - start < timeout )); do
    if curl -fsS --connect-timeout 1 --max-time 3 "$url" > "$out.tmp"; then
      mv "$out.tmp" "$out"
      return 0
    fi
    sleep 1
  done
  rm -f "$out.tmp"
  return 1
}

pulsedag_wait_port_closed() {
  local port="$1" timeout="${2:-30}" start
  start=$(date +%s)
  while (( $(date +%s) - start < timeout )); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  return 1
}

pulsedag_json_txids_sorted() {
  local file="$1"
  jq -r '(.data.txids // [])[]' "$file" | sort -u
}

pulsedag_write_checksums() {
  local dir="$1"
  (cd "$dir" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
}
