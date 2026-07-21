# v2.3.0 public-testnet burn-in gate

This gate applies only after a separate public-testnet launch decision has been recorded and the first public-testnet launch has occurred.

## Clock anchor

- The 30-day clock starts at the first public-testnet launch.
- The clock must not start or be backdated during private-testnet release preparation.
- Any reset condition must be documented with its UTC timestamp, cause, owner, and new clock anchor.

## Required 30-day evidence

- Stable multi-node sync without unresolved safety divergence.
- Correct restart, rejoin, and recovery behavior.
- Orphan and missing-parent pressure within accepted limits.
- Mempool behavior within accepted limits.
- External standalone miner stability and explainable rejection taxonomy.
- RPC availability, rate-limit behavior, and operator observability within accepted limits.
- Snapshot, restore, rebuild, and rollback procedures demonstrated.
- Incident ledger with no unresolved SEV-1 consensus, storage, sync, or mining safety issue.
- Daily evidence bound to exact node versions and operator ownership.

## Decision boundary

Completing private-testnet release validation does not satisfy this gate. `public_testnet_ready` remains `false` until a separate launch decision explicitly changes it.

Smart-contract work remains blocked until at least 30 contiguous days of accepted public-testnet evidence have been independently reviewed and a separate contracts-scope decision is recorded.
