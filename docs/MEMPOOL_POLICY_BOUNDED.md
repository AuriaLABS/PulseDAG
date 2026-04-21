# Bounded mempool policy

Current target: make burn-in stable, not optimize fees yet.

## Proposed defaults
- max transactions: 4096
- max spent outpoints tracked: 8192
- fee floor placeholder: 0
- eviction order: oldest first (placeholder)

## Admission rules
- reject duplicate txid
- reject already confirmed tx
- reject tx spending missing inputs
- reject tx with duplicated inputs
- reject tx with zero-value outputs

## Future extension after burn-in
- fee-based ordering
- eviction by fee/age
- sender limits
- package policies
