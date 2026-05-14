#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVIDENCE_ROOT="${PULSEDAG_REHEARSAL_EVIDENCE_ROOT:-$ROOT_DIR/evidence/v2.2.15/p2p-rehearsal}"
RUNTIME_ROOT_OVERRIDE="${PULSEDAG_REHEARSAL_RUNTIME_ROOT:-}"

echo "========== PulseDAG v2.2.15 P2P rehearsal cleanup =========="

stop_pid() {
  local file="$1" pid
  [[ -f "$file" ]] || return 0
  pid="$(cat "$file")"
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    echo "[info] stopping pid=$pid from $file"
    kill "$pid" 2>/dev/null || true
    for _ in {1..20}; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then
      echo "[warn] pid=$pid did not stop gracefully; sending SIGKILL"
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$file"
}

clean_runtime_root() {
  local runtime_root="$1"
  [[ -d "$runtime_root" ]] || return 0
  if [[ ! -f "$runtime_root/.pulsedag-v2-2-15-rehearsal-root" ]]; then
    echo "[warn] skipping unmarked runtime root: $runtime_root"
    return 0
  fi
  find "$runtime_root" -maxdepth 1 -name 'node-*.pid' -type f -print | while read -r pid_file; do
    stop_pid "$pid_file"
  done
  echo "[info] removing marked runtime root: $runtime_root"
  rm -rf "$runtime_root"
}

if [[ -n "$RUNTIME_ROOT_OVERRIDE" ]]; then
  clean_runtime_root "$RUNTIME_ROOT_OVERRIDE"
else
  if [[ -d "$EVIDENCE_ROOT" ]]; then
    find "$EVIDENCE_ROOT" -path '*/runtime/.pulsedag-v2-2-15-rehearsal-root' -type f -print | while read -r marker; do
      clean_runtime_root "$(dirname "$marker")"
    done
  fi
fi

echo "PASS: cleanup complete"
