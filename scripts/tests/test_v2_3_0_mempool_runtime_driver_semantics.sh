#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"
DRIVER="scripts/v2_3_0_mempool_tx_relay_evidence.sh"

bash -n "$DRIVER"
grep -Fq 'PULSEDAG_ADMIN_ENABLED=true' "$DRIVER"
grep -Fq '"already exists"' "$DRIVER"
grep -Fq '"fee":0' "$DRIVER"
grep -Fq 'candidate_fee:0' "$DRIVER"
grep -Fq '/blocks/$CONFIRM_BLOCK_HASH/transactions' "$DRIVER"
grep -Fq 'block-transactions-endpoint' "$DRIVER"

if grep -Fq '"$(rpc_url "$i")/txs/$TXID"' "$DRIVER"; then
  echo "confirmation evidence must not depend on the mempool-only tx lookup" >&2
  exit 1
fi
if grep -Fq '"$dupe_msg" == *duplicate* &&' "$DRIVER"; then
  echo "duplicate taxonomy must also accept the canonical already-exists response" >&2
  exit 1
fi
