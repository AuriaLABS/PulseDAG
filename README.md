# PulseDAG

PulseDAG is currently developed from a **v2.4.x repository/version surface**, while the definitive public-launch target is **v3.0.0 in Q4 2026**.

The active launch strategy is:

- launch **mainnet and a parallel public testnet in one coordinated v3.0.0 release window**;
- no standalone public-testnet launch first;
- no pre-mainnet 30-day public-testnet acceptance clock;
- freeze independent mainnet/testnet chain identities, genesis, bootnodes and endpoints on one exact v3.0.0 release candidate;
- final launch authorization only through issue #781 with `GO_V3_DUAL_LAUNCH`.

Existing v2.4.x releases, branches and operational evidence remain development, validation and regression inputs. They must not be relabeled as v3.0.0.

## Current repository state

- Repository `VERSION`: `v2.4.0`.
- Cargo workspace version: `2.4.0`.
- Published v2.4.0 node/miner release: historical exact-release evidence; not the definitive public-launch identity.
- Current development continues through v2.4.x/v2.4.1 work toward the final v3.0.0 candidate.
- External standalone miner remains supported separately from `pulsedagd`.
- Production wallet/custody readiness is controlled by #819.
- Mainnet/public dependency-security readiness is controlled by #803.
- Full launch completion is controlled by #794.
- Final coordinated launch decision is controlled only by #781.
- v3.0.0 mainnet chain ID/genesis/bootnodes/endpoints: `TBD` until exact freeze.
- v3.0.0 parallel-testnet chain ID/genesis/bootnodes/endpoints: `TBD` until exact freeze.

### Legacy v2.4 validation markers

The following strings are retained because existing v2.4.x validation/hygiene surfaces still check them. They are **legacy compatibility state**, not the v3 launch model:

- Task31 compatibility marker: `PENDING_EXACT_CANDIDATE_EVIDENCE`.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- `contracts_enabled=false`.

Default high cadence remains separately gated/disabled unless explicitly included in a future frozen release decision.

## Start here

### Definitive v3 launch authority

- [v3.0.0 authoritative roadmap](docs/ROADMAP_V3_0_0.md)
- [v3.0.0 dual-network launch runbook](docs/runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md)
- [v3 launch configuration authority](configs/v3-launch/README.md)
- [Security policy](SECURITY.md)

### Current implementation and protocol history

- [Documentation index](docs/README.md)
- [v2.4.0 roadmap](docs/ROADMAP_V2_4_0.md)
- [v2.4.0 protocol activation contract](docs/PROTOCOL_ACTIVATION_V2_4_0.md)
- [Version matrix](docs/VERSION_MATRIX.md)
- [Operator runbook](docs/RUNBOOK.md)
- [Release evidence policy](docs/RELEASE_EVIDENCE.md)
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

The v3 launch-plan consistency contract is enforced by:

```bash
python scripts/validate_v3_0_0_launch_plan.py
```

Historical v2.x material remains provenance and regression evidence only. No historical release, public-testnet plan or old readiness flag can substitute for the exact v3.0.0 release, security, wallet, infrastructure and dual-network launch evidence required by #781/#794/#803/#819.
