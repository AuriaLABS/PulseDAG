#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  $0 \
    --service <name> \
    --health-url <url> \
    --binary-current <path> \
    --binary-candidate <path> \
    --binary-active-link <path> \
    --data-path <path> \
    --logs-dir <path> \
    [--rollback-binary <path>] \
    [--start-cmd <command>] \
    [--stop-cmd <command>]

Notes:
  - Designed for a non-destructive dry run on one non-seed node.
  - Does not change storage format.
  - Does not delete data.
USAGE
}

SERVICE=""
HEALTH_URL=""
BINARY_CURRENT=""
BINARY_CANDIDATE=""
BINARY_ACTIVE_LINK=""
ROLLBACK_BINARY=""
DATA_PATH=""
LOGS_DIR=""
START_CMD=""
STOP_CMD=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --service) SERVICE="$2"; shift 2 ;;
    --health-url) HEALTH_URL="$2"; shift 2 ;;
    --binary-current) BINARY_CURRENT="$2"; shift 2 ;;
    --binary-candidate) BINARY_CANDIDATE="$2"; shift 2 ;;
    --binary-active-link) BINARY_ACTIVE_LINK="$2"; shift 2 ;;
    --rollback-binary) ROLLBACK_BINARY="$2"; shift 2 ;;
    --data-path) DATA_PATH="$2"; shift 2 ;;
    --logs-dir) LOGS_DIR="$2"; shift 2 ;;
    --start-cmd) START_CMD="$2"; shift 2 ;;
    --stop-cmd) STOP_CMD="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

required=(SERVICE HEALTH_URL BINARY_CURRENT BINARY_CANDIDATE BINARY_ACTIVE_LINK DATA_PATH LOGS_DIR)
for var in "${required[@]}"; do
  if [[ -z "${!var}" ]]; then
    echo "Missing required argument: $var" >&2
    usage
    exit 1
  fi
done

command -v curl >/dev/null 2>&1 || { echo "curl is required" >&2; exit 1; }
command -v date >/dev/null 2>&1 || { echo "date is required" >&2; exit 1; }

if [[ -z "$START_CMD" ]]; then
  START_CMD="systemctl start $SERVICE"
fi
if [[ -z "$STOP_CMD" ]]; then
  STOP_CMD="systemctl stop $SERVICE"
fi

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$LOGS_DIR/$RUN_ID"
mkdir -p "$OUT_DIR"
TIMELINE="$OUT_DIR/timeline.log"
SUMMARY="$OUT_DIR/summary.env"

log_step() {
  local msg="$1"
  printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$msg" | tee -a "$TIMELINE"
}

seconds_now() {
  date -u +%s
}

wait_health() {
  local out_file="$1"
  local attempts=30
  local sleep_s=2
  local i

  for ((i=1; i<=attempts; i++)); do
    if curl -fsS "$HEALTH_URL" > "$out_file"; then
      log_step "health check succeeded on attempt $i"
      return 0
    fi
    sleep "$sleep_s"
  done

  log_step "health check failed after ${attempts} attempts"
  return 1
}

log_step "dry run start: service=$SERVICE health_url=$HEALTH_URL"

START_TS="$(seconds_now)"

log_step "record current binary version"
if [[ -x "$BINARY_CURRENT" ]]; then
  "$BINARY_CURRENT" --version > "$OUT_DIR/current_version.txt" 2>&1 || true
else
  echo "current binary not executable: $BINARY_CURRENT" > "$OUT_DIR/current_version.txt"
fi

log_step "stop one non-seed node"
eval "$STOP_CMD"

log_step "backup binary and data path reference"
{
  echo "binary_current=$BINARY_CURRENT"
  echo "binary_candidate=$BINARY_CANDIDATE"
  echo "binary_active_link=$BINARY_ACTIVE_LINK"
  echo "data_path=$DATA_PATH"
  ls -ld "$DATA_PATH" 2>/dev/null || true
} > "$OUT_DIR/backup_references.txt"

log_step "switch active binary link to candidate"
ln -sfn "$BINARY_CANDIDATE" "$BINARY_ACTIVE_LINK"

UPGRADE_START_TS="$(seconds_now)"
log_step "restart node with candidate binary"
eval "$START_CMD"
wait_health "$OUT_DIR/health_post_upgrade.txt"
UPGRADE_END_TS="$(seconds_now)"

ROLLBACK_EXECUTED="false"
ROLLBACK_DURATION_SEC="0"
if [[ -n "$ROLLBACK_BINARY" ]]; then
  log_step "rollback configured: stopping node for rollback"
  eval "$STOP_CMD"

  log_step "switch active binary link back to rollback binary"
  ROLLBACK_START_TS="$(seconds_now)"
  ln -sfn "$ROLLBACK_BINARY" "$BINARY_ACTIVE_LINK"

  log_step "restart node with rollback binary"
  eval "$START_CMD"
  wait_health "$OUT_DIR/health_post_rollback.txt"
  ROLLBACK_END_TS="$(seconds_now)"

  ROLLBACK_EXECUTED="true"
  ROLLBACK_DURATION_SEC="$((ROLLBACK_END_TS - ROLLBACK_START_TS))"
else
  log_step "rollback skipped: --rollback-binary not provided"
fi

END_TS="$(seconds_now)"
TOTAL_DURATION_SEC="$((END_TS - START_TS))"
UPGRADE_DURATION_SEC="$((UPGRADE_END_TS - UPGRADE_START_TS))"

{
  echo "run_id=$RUN_ID"
  echo "service=$SERVICE"
  echo "health_url=$HEALTH_URL"
  echo "rollback_executed=$ROLLBACK_EXECUTED"
  echo "upgrade_duration_sec=$UPGRADE_DURATION_SEC"
  echo "rollback_duration_sec=$ROLLBACK_DURATION_SEC"
  echo "total_duration_sec=$TOTAL_DURATION_SEC"
} > "$SUMMARY"

log_step "collecting logs"
if command -v journalctl >/dev/null 2>&1; then
  journalctl -u "$SERVICE" --since "-30 min" --no-pager > "$OUT_DIR/journal.log" 2>&1 || true
else
  echo "journalctl not available" > "$OUT_DIR/journal.log"
fi

log_step "dry run completed: out_dir=$OUT_DIR"
echo "Dry run artifacts: $OUT_DIR"
