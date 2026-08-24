# PulseDAG v2.4.0

PulseDAG is in the **v2.4.0 Task31 release/activation candidate construction** stage. The repository version has advanced to `v2.4.0`, but the final exact candidate is not frozen and no release or public-testnet launch is authorized.

## Current state

- Repository version: `v2.4.0`.
- Cargo workspace version: `2.4.0`.
- Tasks 22–29: completed.
- Task30 replay/adversarial validation: must pass again on the final exact Task31 candidate where affected by release-freeze changes.
- Task31 release/activation decision: `PENDING_EXACT_CANDIDATE_EVIDENCE`.
- Target protocol identity: transaction v2, block header v2, `ghostdag_v1`, fresh chain/genesis identity.
- Activated-v2 startup/storage/P2P release wiring: under validation in the Task31 candidate; not yet approved.
- External standalone miner: supported and packaged separately from `pulsedagd`.
- Current release scope: node + standalone miner. No official end-user custody wallet is included in this candidate unless separately ported and revalidated.
- `v2.4.0` tag: not created.
- GitHub Release publication: not authorized.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- Default high cadence remains experimental/disabled.
- `contracts_enabled=false`.

## Start here

- [Documentation index](docs/README.md)
- [v2.4.0 roadmap](docs/ROADMAP_V2_4_0.md)
- [v2.4.0 protocol activation contract](docs/PROTOCOL_ACTIVATION_V2_4_0.md)
- [Version matrix](docs/VERSION_MATRIX.md)
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

Historical v2.3.x and v2.2.x material is retained for compatibility, evidence and provenance only. It must not be presented as the active v2.4.0 release identity or as authorization to launch a public testnet.
