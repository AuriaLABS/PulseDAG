#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/artifacts/v3/compact_relay_5n4m}"
RUN_ID="${RUN_ID:-compact-relay-$(date -u +%Y%m%dT%H%M%SZ)}"
EXPECTED_NODES="${EXPECTED_NODES:-5}"

if ! git -C "$ROOT_DIR" diff --quiet --ignore-submodules -- ||    ! git -C "$ROOT_DIR" diff --cached --quiet --ignore-submodules --; then
  echo "compact-relay evidence requires a clean tracked workspace" >&2
  exit 2
fi

CANDIDATE_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD)"
CANDIDATE_TREE="$(git -C "$ROOT_DIR" rev-parse 'HEAD^{tree}')"

export OUT_DIR RUN_ID
set +e
PULSEDAG_REHEARSAL_VERSION="v2.4.0" \
PULSEDAG_REHEARSAL_VERSION_SLUG="v2_4_0" \
PULSEDAG_CONSENSUS_MODE="legacy" \
PULSEDAG_PROTOCOL_CONSENSUS_MODE="ghostdag_v1" \
MINER_COUNT=4 \
STAGE_NAME="5N/4M compact-relay acceptance" \
bash "$ROOT_DIR/scripts/v2_2_20_private_5n_4m_rehearsal.sh"
rehearsal_rc=$?
set -e

observed_commit="$(git -C "$ROOT_DIR" rev-parse HEAD)"
observed_tree="$(git -C "$ROOT_DIR" rev-parse 'HEAD^{tree}')"
if [[ "$observed_commit" != "$CANDIDATE_COMMIT" || "$observed_tree" != "$CANDIDATE_TREE" ]]; then
  echo "compact-relay rehearsal changed the exact candidate checkout" >&2
  rehearsal_rc=97
fi
if ! git -C "$ROOT_DIR" diff --quiet --ignore-submodules -- ||    ! git -C "$ROOT_DIR" diff --cached --quiet --ignore-submodules --; then
  echo "compact-relay rehearsal modified tracked candidate files" >&2
  rehearsal_rc=98
fi

EVIDENCE_DIR="$OUT_DIR/$RUN_ID/compact-relay-evidence"
mkdir -p "$EVIDENCE_DIR"

python3 "$ROOT_DIR/scripts/v3_compact_relay_multinode_evidence.py" \
  --artifact-root "$OUT_DIR/$RUN_ID" \
  --output-json "$EVIDENCE_DIR/evidence.json" \
  --output-md "$EVIDENCE_DIR/evidence.md" \
  --expected-nodes "$EXPECTED_NODES" \
  --rehearsal-exit-code "$rehearsal_rc" \
  --candidate-commit "$CANDIDATE_COMMIT" \
  --candidate-tree "$CANDIDATE_TREE"
