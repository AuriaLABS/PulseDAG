# PulseDAG v2.2.14 release notes

PulseDAG v2.2.14 closes as a storage, replay, snapshot, restore, pruning, and migration-policy hardening release on the path toward v3.0.0.

## Highlights

- Aligns version documentation with `VERSION = v2.2.14` and Cargo workspace version `2.2.14`.
- Documents v2.2.14 as a durability and operator-evidence release, not a smart-contract, pool, or runtime-expansion release.
- Hardens deterministic replay ordering for persisted blocks by sorting by height, timestamp, and hash.
- Applies deterministic replay ordering to full rebuild and snapshot-plus-delta rebuild paths.
- Keeps `STORAGE_SCHEMA_VERSION` explicit and documents v2.2.14 storage migration policy.
- Rejects unsupported future or corrupt schema metadata with operator-facing errors.
- Keeps the 60-second target block interval.
- Makes the testnet profile use the real `libp2p-real` networking runtime.
- Updates the private profile default chain id to a v2.2.14-named private chain.
- Adds `scripts/v2-2-14-release-evidence.sh` for repeatable release evidence.

## Operational evidence

Release closeout should include automated output from:

```bash
./scripts/v2-2-14-release-evidence.sh
```

Manual evidence should be attached for snapshot export/import, restore drill, pruning safety, startup replay from persisted blocks, and a three-node private rehearsal when available.

## Boundaries

- No smart contracts are added.
- No contract runtime is enabled.
- No pool logic is added.
- The miner remains an external standalone application.
- v2.3.0 remains a future private-testnet readiness decision, not an automatic result of v2.2.14.
