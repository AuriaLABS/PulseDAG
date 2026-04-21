# Mempool sanitize pipeline

Goal: remove invalid or stale transactions from mempool without mutating chain state incorrectly.

## Trigger paths
- startup after replay/rebuild
- manual operator call: `POST /mempool/sanitize`
- optional future periodic sanitize

## Steps
1. Snapshot current mempool transaction ids.
2. For each tx, re-run transaction validation against current UTXO state.
3. Remove tx if any of these apply:
   - referenced input no longer exists
   - duplicated input inside tx
   - zero-value output
   - insufficient funds
   - tx already confirmed in DAG
   - tx conflicts with a now-confirmed spend
4. Rebuild `spent_outpoints` from surviving mempool txs.
5. Persist snapshot if sanitize changed the mempool.
6. Emit runtime counters and journal event.

## Response shape
```json
{
  "ok": true,
  "data": {
    "before_count": 12,
    "after_count": 8,
    "removed_count": 4,
    "removed_txids": ["..."]
  }
}
```
