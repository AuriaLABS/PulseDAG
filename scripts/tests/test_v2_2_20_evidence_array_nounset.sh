#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT_DIR/scripts/v2_2_20_private_5n_4m_rehearsal.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

rg -q '^declare -a EVIDENCE_CONSISTENCY_FAILURES=\(\)' "$SCRIPT"
rg -q '^MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON=' "$SCRIPT"
rg -q '^LOCAL_MINER_SUBMITS_REJECTED_BY_REASON_JSON=' "$SCRIPT"
! rg -q '=\[\]' "$SCRIPT"

(
  set -euo pipefail
  declare -a EVIDENCE_CONSISTENCY_FAILURES=()
  MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON='[]'
  [[ "${#EVIDENCE_CONSISTENCY_FAILURES[@]}" == "0" ]]
  printf '%s' "$MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON" | jq -e 'type == "array" and length == 0' >/dev/null
)

OUT_DIR="$TMP_DIR/run"
mkdir -p "$OUT_DIR"/{endpoints,logs,miners,nodes,samples,summaries}
for i in 1 2 3 4 5; do
  printf 'node %s log\n' "$i" > "$OUT_DIR/logs/n${i}.log"
  cp "$OUT_DIR/logs/n${i}.log" "$OUT_DIR/nodes/n${i}.log"
done
printf 'miner 1 log\n' > "$OUT_DIR/logs/miner-1.log"
printf '# summary\n- warnings:\n  - none\n- failure reasons:\n  - none\n' > "$OUT_DIR/evidence-summary.md"
printf '{"nodes":[],"result":"PASS","overall_result":"PASS","failure_class":"none","primary_failure_class":"none","failure_classes":[],"mining_semantics":{"result":"NOT_EXECUTED"},"rich_node_state":[],"gates":{"baseline_5n_1m":"PASS","intermediate_5n_2m":"NOT_PROVIDED","stress_5n_4m":"OBSERVE"}}\n' > "$OUT_DIR/evidence_manifest.json"
printf '{"nodes":[]}\n' > "$OUT_DIR/p2p_convergence.json"
cp "$OUT_DIR/p2p_convergence.json" "$OUT_DIR/final-convergence-table.json"
printf '{"pre":{},"post":{}}\n' > "$OUT_DIR/quiescence-metrics.json"
printf 'restart_rejoin_status=NOT_EXECUTED\n' > "$OUT_DIR/restart_rejoin.log"
printf 'command log\nFINAL_RESULT=PASS\n' > "$OUT_DIR/command-log.txt"
printf 'metadata\n' > "$OUT_DIR/summaries/package-metadata.txt"
(
  cd "$OUT_DIR"
  tar -czf evidence.tar.gz endpoints logs miners nodes samples summaries evidence-summary.md evidence_manifest.json command-log.txt p2p_convergence.json final-convergence-table.json quiescence-metrics.json restart_rejoin.log
  sha256sum evidence.tar.gz > evidence.tar.gz.sha256
  tar -tzf evidence.tar.gz > archive.list
)
for item in evidence_manifest.json evidence-summary.md p2p_convergence.json final-convergence-table.json quiescence-metrics.json restart_rejoin.log endpoints/ logs/ nodes/; do
  rg -q "^${item}" "$OUT_DIR/archive.list"
done
jq -e '.result == "PASS" and .failure_class == "none" and (.failure_classes | length == 0)' "$OUT_DIR/evidence_manifest.json" >/dev/null
(
  cd "$OUT_DIR"
  sha256sum -c evidence.tar.gz.sha256 >/dev/null
)

bash -euo pipefail -c 'declare -a EVIDENCE_CONSISTENCY_FAILURES=(); MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON="[]"; echo FINAL_RESULT=PASS; echo "count=${#EVIDENCE_CONSISTENCY_FAILURES[@]}"' >/dev/null
if bash -euo pipefail -c 'declare -a EVIDENCE_CONSISTENCY_FAILURES=("real failure"); (( ${#EVIDENCE_CONSISTENCY_FAILURES[@]} == 0 ))'; then
  echo "expected real evidence-consistency failure to return non-zero" >&2
  exit 1
fi
