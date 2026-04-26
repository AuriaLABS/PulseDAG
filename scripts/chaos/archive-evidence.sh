#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  scripts/chaos/archive-evidence.sh --run-id <id> [--base-dir <path>] [--node-urls <csv>]
USAGE
}

RUN_ID=""
BASE_DIR=""
NODE_URLS="http://127.0.0.1:8080,http://127.0.0.1:8081,http://127.0.0.1:8082"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-id)
      RUN_ID="${2:-}"
      shift 2
      ;;
    --base-dir)
      BASE_DIR="${2:-}"
      shift 2
      ;;
    --node-urls)
      NODE_URLS="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$RUN_ID" ]]; then
  echo "--run-id is required" >&2
  usage >&2
  exit 1
fi

if [[ -z "$BASE_DIR" ]]; then
  BASE_DIR="artifacts/release-evidence/${RUN_ID}/chaos-suite"
fi

if [[ ! -d "$BASE_DIR" ]]; then
  echo "missing chaos suite directory: $BASE_DIR" >&2
  exit 1
fi

scripts/chaos/validate-evidence.sh --run-id "$RUN_ID" --base-dir "$BASE_DIR" --node-urls "$NODE_URLS"

ARCHIVE_ROOT="$(dirname "$BASE_DIR")"
ARCHIVE_NAME="chaos-suite-${RUN_ID}.tar.gz"
ARCHIVE_PATH="${ARCHIVE_ROOT}/${ARCHIVE_NAME}"
CHECKSUM_PATH="${ARCHIVE_PATH}.sha256"

(
  cd "$ARCHIVE_ROOT"
  tar -czf "$ARCHIVE_NAME" "$(basename "$BASE_DIR")"
)

sha256sum "$ARCHIVE_PATH" > "$CHECKSUM_PATH"

echo "archive created: $ARCHIVE_PATH"
echo "checksum created: $CHECKSUM_PATH"
