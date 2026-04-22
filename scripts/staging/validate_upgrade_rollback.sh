#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  $0 baseline --node <url> --out <dir>
  $0 post-upgrade --node <url> --baseline <status.json> --out <dir>
  $0 post-rollback --node <url> --baseline <status.json> --out <dir>
USAGE
}

if [[ $# -lt 1 ]]; then
  usage
  exit 1
fi

MODE="$1"
shift

NODE_URL="http://127.0.0.1:8080"
OUT_DIR=""
BASELINE_STATUS=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --node)
      NODE_URL="$2"
      shift 2
      ;;
    --out)
      OUT_DIR="$2"
      shift 2
      ;;
    --baseline)
      BASELINE_STATUS="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$OUT_DIR" ]]; then
  echo "--out is required" >&2
  exit 1
fi

if [[ "$MODE" != "baseline" && -z "$BASELINE_STATUS" ]]; then
  echo "--baseline is required for mode: $MODE" >&2
  exit 1
fi

if [[ "$MODE" != "baseline" && ! -f "$BASELINE_STATUS" ]]; then
  echo "Baseline status file not found: $BASELINE_STATUS" >&2
  exit 1
fi

command -v curl >/dev/null 2>&1 || { echo "curl is required" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 1; }

TARGET_DIR="$OUT_DIR/$MODE"
mkdir -p "$TARGET_DIR"

fetch() {
  local endpoint="$1"
  local output="$2"
  curl -fsS "$NODE_URL$endpoint" > "$output"
}

fetch "/release" "$TARGET_DIR/release.json"
fetch "/status" "$TARGET_DIR/status.json"
fetch "/snapshot" "$TARGET_DIR/snapshot.json"
fetch "/maintenance/report" "$TARGET_DIR/maintenance_report.json"
fetch "/sync/verify" "$TARGET_DIR/sync_verify.json"
fetch "/readiness" "$TARGET_DIR/readiness.json"
fetch "/runtime/events?limit=200" "$TARGET_DIR/runtime_events.json"

export MODE TARGET_DIR BASELINE_STATUS
python3 <<'PY'
import json
import os
from pathlib import Path

mode = os.environ["MODE"]
target_dir = Path(os.environ["TARGET_DIR"])


def load(name: str):
    with open(target_dir / name, "r", encoding="utf-8") as f:
        return json.load(f)


def assert_ok(payload, name: str):
    if not payload.get("ok", False):
        raise SystemExit(f"{name}: api response not ok")
    if payload.get("data") is None:
        raise SystemExit(f"{name}: missing data field")

release = load("release.json")
status = load("status.json")
sync_verify = load("sync_verify.json")
readiness = load("readiness.json")
maintenance = load("maintenance_report.json")

for name, payload in [
    ("release", release),
    ("status", status),
    ("sync_verify", sync_verify),
    ("readiness", readiness),
    ("maintenance_report", maintenance),
]:
    assert_ok(payload, name)

status_data = status["data"]
sync_data = sync_verify["data"]
readiness_data = readiness["data"]
maintenance_data = maintenance["data"]

if not status_data.get("chain_id"):
    raise SystemExit("status: chain_id must be non-empty")
if not sync_data.get("consistent", False):
    raise SystemExit("sync_verify: expected consistent=true")
if not readiness_data.get("ready_for_release", False):
    raise SystemExit(f"readiness: not ready_for_release, blockers={readiness_data.get('blockers')}")
if not maintenance_data.get("consistent", False):
    raise SystemExit("maintenance_report: expected consistent=true")

if mode != "baseline":
    with open(os.environ["BASELINE_STATUS"], "r", encoding="utf-8") as f:
        baseline = json.load(f)
    assert_ok(baseline, "baseline_status")
    baseline_height = baseline["data"].get("best_height", 0)
    current_height = status_data.get("best_height", 0)
    if current_height < baseline_height:
        raise SystemExit(
            f"status: best_height regressed from baseline ({baseline_height} -> {current_height})"
        )

print(f"{mode}: validation passed")
PY

echo "Evidence captured in: $TARGET_DIR"
