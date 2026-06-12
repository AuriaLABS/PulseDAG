#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT_DIR"

bash -n \
  scripts/v2_2_20_preflight_check.sh \
  scripts/v2_2_20_private_5n_1m_rehearsal.sh \
  scripts/v2_2_20_private_5n_2m_rehearsal.sh \
  scripts/v2_2_20_private_5n_4m_rehearsal.sh \
  scripts/docker_v2_2_20_rehearsal.sh

rg -q 'ENV_FAIL: missing dependency' scripts/v2_2_20_preflight_check.sh
rg -q 'failure_class: \$FAILURE_CLASS' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'extract_bootnode_peer_id' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'jq missing; cannot parse n1 /p2p/status JSON' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'JSON schema mismatch; expected \.data\.peer_id or \.data\.local_node_id' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'PowerShell does not support Bash-style inline environment assignment' docs/DOCKER_REHEARSALS_V2_2_20.md
rg -q 'evidence_manifest.json' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'RPC_ALIVE_LISTENER_TIMEOUT count' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'orphan_recovery_classification_counters' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'submit_busy' scripts/v2_2_20_private_5n_4m_rehearsal.sh
rg -q 'Interpreting self-classifying evidence bundles' docs/DOCKER_REHEARSALS_V2_2_20.md
rg -q 'Docker Compose from PowerShell' docs/DOCKER_REHEARSALS_V2_2_20.md

TMP_BIN=$(mktemp -d)
TMP_OUT=$(mktemp -d)
cleanup(){ rm -rf "$TMP_BIN" "$TMP_OUT"; }
trap cleanup EXIT

for dep in bash curl tar gzip mkdir cat date; do
  dep_path=$(command -v "$dep")
  ln -s "$dep_path" "$TMP_BIN/$dep"
done

set +e
PATH="$TMP_BIN" OUT_DIR="$TMP_OUT/missing-jq" bash scripts/v2_2_20_preflight_check.sh >"$TMP_OUT/missing-jq.out" 2>"$TMP_OUT/missing-jq.err"
rc=$?
set -e
[[ $rc -eq 2 ]]
rg -q 'ENV_FAIL: missing dependency: jq JSON parser' "$TMP_OUT/missing-jq.err"
rg -q 'failure_class: environment' "$TMP_OUT/missing-jq/preflight-summary.md"

for dep in jq; do
  dep_path=$(command -v "$dep")
  ln -s "$dep_path" "$TMP_BIN/$dep"
done
set +e
PATH="$TMP_BIN" OUT_DIR="$TMP_OUT/missing-docker" bash scripts/v2_2_20_preflight_check.sh --docker-mode >"$TMP_OUT/missing-docker.out" 2>"$TMP_OUT/missing-docker.err"
rc=$?
set -e
[[ $rc -eq 2 ]]
rg -q 'ENV_FAIL: missing dependency: Docker CLI for Docker-mode rehearsal' "$TMP_OUT/missing-docker.err"
rg -q 'failure_class: environment' "$TMP_OUT/missing-docker/preflight-summary.md"

echo "v2.2.20 rehearsal environment preflight validation passed"
