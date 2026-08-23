# v2.4.0 public-testnet burn-in gate

This gate applies only after a separate public-testnet launch decision has been recorded and the first public-testnet launch has occurred. Task31 release/activation validation does not start this clock.

## Clock anchor

- The 30-day clock starts at the first authorized public-testnet launch.
- The clock must not start or be backdated during private release preparation or Task31 validation.
- Any reset condition must be documented with its UTC timestamp, cause, owner and new clock anchor.

## Required 30-day evidence

- Stable multi-node sync without unresolved safety divergence.
- Correct restart, rejoin and recovery behavior.
- Orphan and missing-parent pressure within accepted limits.
- Mempool behavior within accepted limits.
- External standalone miner stability and explainable rejection taxonomy.
- RPC availability, rate-limit behavior and operator observability within accepted limits.
- Snapshot, restore, rebuild and rollback procedures demonstrated.
- Incident ledger with no unresolved Sev-1 consensus, storage, replay, sync, mining, security or operator-safety issue.
- Daily evidence bound to the exact launched node/miner versions and operator ownership.

## Decision boundary

Completing v2.4.0 Task31 release validation does not satisfy this gate. Current state remains:

- `public_testnet_ready=false`
- `thirty_day_public_testnet_clock_started=false`
- `contracts_enabled=false`

Smart-contract work remains blocked until the accepted public-testnet period has completed and a separate contracts-scope decision is recorded.
