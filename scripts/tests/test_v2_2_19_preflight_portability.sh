#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
SCRIPT="$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"
OUT_BASE="${OUT_BASE:-/tmp/pulsedag-preflight-portability}"

rm -rf "$OUT_BASE"
mkdir -p "$OUT_BASE"

echo "[1/2] Running preflight with default PATH"
OUT_DIR="$OUT_BASE/default" bash "$SCRIPT"

echo "[2/2] Running preflight with rg hidden to force grep fallback"
TMP_BIN=$(mktemp -d)
cat > "$TMP_BIN/rg" <<'SH'
#!/usr/bin/env bash
echo "rg intentionally unavailable for portability test" >&2
exit 127
SH
chmod +x "$TMP_BIN/rg"
PATH="$TMP_BIN:$PATH" OUT_DIR="$OUT_BASE/fallback" bash "$SCRIPT"

echo "Portability preflight test completed. Evidence written to: $OUT_BASE"
