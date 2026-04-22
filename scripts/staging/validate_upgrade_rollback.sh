#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  $0 baseline --node <url> --out <dir>
  $0 post-upgrade --node <url> --baseline <status.json> --out <dir> [--target-version <semver>] [--target-stage <stage>]
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
TARGET_VERSION=""
TARGET_STAGE=""

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
    --target-version)
      TARGET_VERSION="$2"
      shift 2
      ;;
    --target-stage)
      TARGET_STAGE="$2"
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

export MODE TARGET_DIR BASELINE_STATUS TARGET_VERSION TARGET_STAGE
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
release_data = release["data"]
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
    baseline_release_path = Path(os.environ["BASELINE_STATUS"]).with_name("release.json")
    baseline_release = None
    with open(os.environ["BASELINE_STATUS"], "r", encoding="utf-8") as f:
        baseline = json.load(f)
    assert_ok(baseline, "baseline_status")
    if baseline_release_path.exists():
        with open(baseline_release_path, "r", encoding="utf-8") as f:
            baseline_release = json.load(f)
        assert_ok(baseline_release, "baseline_release")
    baseline_height = baseline["data"].get("best_height", 0)
    current_height = status_data.get("best_height", 0)
    if current_height < baseline_height:
        raise SystemExit(
            f"status: best_height regressed from baseline ({baseline_height} -> {current_height})"
        )

    current_version = release_data.get("version")
    current_stage = release_data.get("stage")
    target_version = os.environ.get("TARGET_VERSION") or ""
    target_stage = os.environ.get("TARGET_STAGE") or ""
    baseline_release_data = baseline_release["data"] if baseline_release else {}
    baseline_version = baseline_release_data.get("version")
    baseline_stage = baseline_release_data.get("stage")

    if mode == "post-upgrade":
        if target_version and current_version != target_version:
            raise SystemExit(
                f"release: expected upgraded version {target_version}, got {current_version}"
            )
        if target_stage and current_stage != target_stage:
            raise SystemExit(
                f"release: expected upgraded stage {target_stage}, got {current_stage}"
            )
        if not target_version:
            if baseline_release is None:
                raise SystemExit(
                    "release: baseline/release.json not found; pass --target-version to assert upgraded version"
                )
            if baseline_version == current_version and baseline_stage == current_stage:
                raise SystemExit(
                    "release: version/stage unchanged from baseline; upgrade likely did not take effect"
                )
    elif mode == "post-rollback" and baseline_release is not None:
        if baseline_version is not None and current_version != baseline_version:
            raise SystemExit(
                f"release: rollback expected baseline version {baseline_version}, got {current_version}"
            )
        if baseline_stage is not None and current_stage != baseline_stage:
            raise SystemExit(
                f"release: rollback expected baseline stage {baseline_stage}, got {current_stage}"
            )

print(f"{mode}: validation passed")
PY

echo "Evidence captured in: $TARGET_DIR"
