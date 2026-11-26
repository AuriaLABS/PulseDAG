# PulseDAG v3.0.0

PulseDAG is a native proof-of-work DAG cryptocurrency under development for its **first public network**. The first network will be a public testnet, followed by an independently authorized mainnet. No earlier network has been deployed; no backward network compatibility or chain migration is required. The software candidate is v3.0.0.

The exact release/mainnet candidate is not frozen. A version bump or partial CI pass does not authorize a tag, GitHub Release, public-testnet launch, mainnet launch, or activation of disabled protocol features.

## Current state

- Repository version: `v3.0.0`.
- Cargo workspace version: `3.0.0`.
- Candidate decision: `PENDING_EXACT_CANDIDATE_EVIDENCE`.
- Active CI/readiness target: v3.0.0 only.
- Core protocol target: native-v3 GHOSTDAG, KHeavyHash PoW and deterministic UTXO monetary admission; production integration is still under validation.
- Fresh v3 chain/genesis identity is required before launch; the final identity is not frozen.
- External standalone miner remains separate from `pulsedagd`.
- `v3.0.0` tag: not created.
- GitHub Release publication: not authorized.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- Default high cadence remains disabled until its v3 evidence gate passes.
- `contracts_enabled=false`.
- Applicable prototype regression fixtures may be retained temporarily, but cannot establish native monetary acceptance.

## Start here

- [v3.0.0 roadmap](docs/ROADMAP_V3_0_0.md)
- [Version matrix](docs/VERSION_MATRIX.md)
- [Documentation index](docs/README.md)
- [Operator runbook](docs/RUNBOOK.md)
- [Release evidence policy](docs/RELEASE_EVIDENCE.md)
- [Public-testnet burn-in gate](docs/BURN_IN_GATE.md)
- [First-network policy](docs/runbooks/V3_FIRST_NETWORK_NATIVE_ONLY.md)
- [Public testnet launch procedure](docs/runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md)
- [Binary installation](docs/INSTALL_BINARIES.md)

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

All new release/readiness evidence must be bound to the exact v3.0.0 candidate SHA. Earlier development evidence may support applicable regression or provenance, but cannot establish launch readiness.
