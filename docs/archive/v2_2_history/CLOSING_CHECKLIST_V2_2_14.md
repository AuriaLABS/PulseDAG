# PulseDAG v2.2.14 closing checklist

v2.2.14 closes PulseDAG as a storage/replay hardening release on the path to v3.0.0. This checklist is a release gate, not a v2.3.0 readiness claim.

## Automated evidence gate

Run the evidence bundle script from the repository root:

```bash
./scripts/v2-2-14-release-evidence.sh
```

The script prints PASS/FAIL sections for:

- `cargo fmt --check`
- `cargo test -p pulsedag-core`
- `cargo test -p pulsedag-storage`
- `cargo test --workspace`
- `cargo build --workspace`

All failures must be fixed or explicitly documented as environment limitations before release sign-off.

## Version and metadata gate

- [ ] `VERSION` is `v2.2.14`.
- [ ] Cargo workspace version is `2.2.14`.
- [ ] Cargo workspace license remains `ISC`.
- [ ] `README.md` and `docs/VERSION_MATRIX.md` describe v2.2.14 consistently.

## Storage and migration-policy gate

- [ ] `STORAGE_SCHEMA_VERSION` is explicit.
- [ ] Missing schema metadata is initialized as current compatible metadata.
- [ ] Valid current schema metadata is accepted.
- [ ] Future schema metadata is rejected with an operator-facing error.
- [ ] Corrupt schema metadata is rejected when practical.
- [ ] `docs/STORAGE_MIGRATION_POLICY_V2_2_14.md` is reviewed.

## Deterministic replay gate

- [ ] Persisted/replayed block order is deterministic by height, timestamp, then hash.
- [ ] Full rebuild replay uses the same deterministic ordering.
- [ ] Snapshot-plus-delta replay uses the same deterministic ordering.
- [ ] Startup recovery paths that load persisted blocks use deterministic storage helpers.
- [ ] Regression tests cover equal-height blocks inserted/replayed in different orders.

## Manual operational evidence expected

Attach evidence or mark unavailable with an owner/follow-up for:

- [ ] Snapshot export/import.
- [ ] Restore drill from snapshot plus delta.
- [ ] Pruning safety check and retained restore anchor.
- [ ] Startup replay from persisted blocks.
- [ ] Three-node private rehearsal using real `libp2p-real`, if available.

## Testnet/private profile gate

- [ ] Testnet profile uses real libp2p networking (`libp2p-real`), not the dev loopback/skeleton runtime.
- [ ] Private profile chain_id is reviewed and aligned with v2.2.14 defaults or documented continuity requirements.
- [ ] 60-second target block interval remains preserved.

## Scope guardrails

- [ ] No smart contracts are added.
- [ ] No contract runtime is enabled.
- [ ] No pool logic is added.
- [ ] Miner remains a standalone external application.
- [ ] v2.2.14 is documented as storage/replay/snapshot/restore/pruning/migration hardening, not v2.3.0 readiness.
