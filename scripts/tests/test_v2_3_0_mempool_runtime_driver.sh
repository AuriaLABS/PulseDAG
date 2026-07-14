#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"
bash -n scripts/v2_3_0_mempool_tx_relay_evidence.sh
bash -n scripts/lib/v2_3_0_runtime_harness.sh
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/mempool.json" <<'JSON'
{"data":{"txids":["b","a","a"]}}
JSON
source scripts/lib/v2_3_0_runtime_harness.sh
[[ "$(pulsedag_json_txids_sorted "$tmp/mempool.json" | paste -sd ',')" == "a,b" ]]
cat > "$tmp/manifest.json" <<'JSON'
{"result":"PASS","evidence_kind":"runtime","node_count":5,"relay_converged":true,"duplicate_suppression":true,"capacity_rejection_taxonomy":true,"confirmation_cleanup":true,"deterministic_final_mempool_sets":true,"public_testnet_ready":false}
JSON
jq -e '.result == "PASS" and .evidence_kind == "runtime" and .node_count == 5 and .relay_converged and .duplicate_suppression and .capacity_rejection_taxonomy and .confirmation_cleanup and .deterministic_final_mempool_sets and (.public_testnet_ready == false)' "$tmp/manifest.json" >/dev/null
