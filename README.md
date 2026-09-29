# PulseDAG v3.0.0

PulseDAG is now in **v3.0.0 candidate construction**. The repository and active CI target v3.0.0; earlier v2.x releases remain only as historical, compatibility, migration, and regression inputs.

The exact release/mainnet candidate is not frozen. A version bump or partial CI pass does not authorize a tag, GitHub Release, public-testnet launch, mainnet launch, or activation of disabled protocol features.

## Current state

- Repository version: `v3.0.0`.
- Cargo workspace version: `3.0.0`.
- Candidate decision: `PENDING_EXACT_CANDIDATE_EVIDENCE`.
- Active CI/readiness target: v3.0.0 only.
- Core protocol baseline: transaction v2, block header v2, `ghostdag_v1`, plus v3 runtime/evidence work.
- Fresh v3 chain/genesis identity is required before launch; the final identity is not frozen.
- External standalone miner remains separate from `pulsedagd`.
- `v3.0.0` tag: not created.
- GitHub Release publication: not authorized.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- Default high cadence remains disabled until its v3 evidence gate passes.
- `contracts_enabled=false`.
- Historical v2.x tests may remain as compatibility fixtures, but they are not v3 release/readiness gates.

## Start here

- [v3.0.0 roadmap](docs/ROADMAP_V3_0_0.md)
- [Version matrix](docs/VERSION_MATRIX.md)
- [Documentation index](docs/README.md)
- [Operator runbook](docs/RUNBOOK.md)
- [Release evidence policy](docs/RELEASE_EVIDENCE.md)
- [Public-testnet burn-in gate](docs/BURN_IN_GATE.md)
- [Historical archive](docs/archive/README.md)

## Development

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Repository structure and version-surface checks are enforced by:

```bash
bash scripts/repository_hygiene.sh --strict
```

All new release/readiness evidence must be bound to the exact v3.0.0 candidate SHA. Earlier-version evidence can support compatibility or provenance, but cannot make the v3 candidate ready.
