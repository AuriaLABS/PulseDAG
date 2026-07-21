#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/.." && pwd)"
engine="$script_dir/v2_2_20_private_5n_4m_rehearsal.sh"
rehearsal_version="${PULSEDAG_REHEARSAL_VERSION:-v2.3.0}"
rehearsal_version_slug="${PULSEDAG_REHEARSAL_VERSION_SLUG:-v2_3_0}"

case "${MINER_COUNT:-}" in
  1) default_out="private_5n_1m_rehearsal" ;;
  2) default_out="private_5n_2m_rehearsal" ;;
  4) default_out="private_5n_4m_rehearsal" ;;
  *) echo "FATAL: MINER_COUNT must be 1, 2, or 4" >&2; exit 2 ;;
esac

[[ -f "$engine" ]] || { echo "FATAL: compatibility engine not found: $engine" >&2; exit 2; }
[[ "$rehearsal_version" == "v2.3.0" ]] || { echo "FATAL: unsupported rehearsal version: $rehearsal_version" >&2; exit 2; }
[[ "$rehearsal_version_slug" == "v2_3_0" ]] || { echo "FATAL: unsupported rehearsal version slug: $rehearsal_version_slug" >&2; exit 2; }

export PULSEDAG_REHEARSAL_VERSION="$rehearsal_version"
export PULSEDAG_REHEARSAL_VERSION_SLUG="$rehearsal_version_slug"
export OUT_DIR="${OUT_DIR:-$root_dir/artifacts/$rehearsal_version_slug/$default_out}"

runtime_script="$(mktemp "$script_dir/.v2_3_0_private_rehearsal_runtime.XXXXXX.sh")"
cleanup_runtime() {
  rm -f "$runtime_script"
}
trap cleanup_runtime EXIT

python3 - "$engine" "$runtime_script" "$rehearsal_version" "$rehearsal_version_slug" <<'PY'
from pathlib import Path
import sys

source = Path(sys.argv[1])
target = Path(sys.argv[2])
version = sys.argv[3]
version_slug = sys.argv[4]
text = source.read_text(encoding="utf-8")
text = text.replace("v2.2.20", version)
text = text.replace("artifacts/v2_2_20", f"artifacts/{version_slug}")
target.write_text(text, encoding="utf-8")
PY
chmod 700 "$runtime_script"

if [[ "${1:-}" == "--verify-template" ]]; then
  if grep -Fq 'v2.2.20' "$runtime_script"; then
    echo "FATAL: transformed rehearsal still contains visible v2.2.20 identity" >&2
    exit 1
  fi
  if grep -Fq 'artifacts/v2_2_20' "$runtime_script"; then
    echo "FATAL: transformed rehearsal still targets the v2_2_20 artifact root" >&2
    exit 1
  fi
  grep -Fq 'v2.3.0' "$runtime_script"
  grep -Fq 'artifacts/v2_3_0' "$runtime_script"
  grep -Fq 'scripts/v2_2_20_preflight_check.sh' "$runtime_script"
  echo "PASS: v2.3.0 rehearsal identity template"
  exit 0
fi

bash "$runtime_script" "$@"
