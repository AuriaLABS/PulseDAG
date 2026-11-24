#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
for dep in bash dirname mkdir curl tar gzip tr awk grep cat; do
  ln -s "$(command -v "$dep")" "$tmp/bin/$dep"
done
expect_missing() {
  local name="$1"; shift
  local rc=0
  PATH="$tmp/bin" OUT_DIR="$tmp/$name" bash scripts/v3_0_0_preflight_check.sh "$@" >"$tmp/$name.log" 2>&1 || rc=$?
  [[ "$rc" == 2 ]]
  grep -Fq "ENV_FAIL: missing dependency: $name" "$tmp/$name.log"
  grep -Fq 'failure_class: environment' "$tmp/$name/preflight-summary.md"
  grep -Fq 'result: FAIL' "$tmp/$name/preflight-summary.md"
}
expect_missing jq
ln -s "$(command -v jq)" "$tmp/bin/jq"
expect_missing docker --docker-mode
OUT_DIR="$tmp/current" PULSEDAG_REHEARSAL_VERSION=v3.0.0 PULSEDAG_REHEARSAL_VERSION_SLUG=v3_0_0 bash scripts/v3_0_0_preflight_check.sh
rc=0
OUT_DIR="$tmp/obsolete" PULSEDAG_REHEARSAL_VERSION=v2.4.0 PULSEDAG_REHEARSAL_VERSION_SLUG=v3_0_0 bash scripts/v3_0_0_preflight_check.sh >"$tmp/obsolete.log" 2>&1 || rc=$?
[[ "$rc" == 1 ]]
grep -Fq 'result: FAIL' "$tmp/obsolete/preflight-summary.md"
echo "PASS: v3-only identity and environment failure regressions"
