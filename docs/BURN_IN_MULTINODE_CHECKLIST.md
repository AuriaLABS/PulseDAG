# Burn-in Multinode Checklist

## Before 30-day run
- at least 2 nodes connected
- external miner producing blocks through template/submit path
- runtime journal enabled
- mempool sanitize counters visible
- orphan queue bounded
- sync lag endpoint visible

## Daily checks
- peer_count > 0
- selected_tip stable and advancing
- orphan_count bounded
- mempool not stuck
- no repeated startup rebuild loops
- no persistent sync lag

## Weekly checks
- restart one node and verify catch-up
- compare snapshot height vs persisted max height
- inspect runtime event summary
- prune runtime journal if needed
