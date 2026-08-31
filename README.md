# PulseDAG

PulseDAG is currently developed from a **v2.4.x repository/version surface**, while the definitive public-launch target is **v3.0.0 in Q4 2026**.

The development path to v3 is:

`v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release`

The approved v2.5.0 and v2.6.0 technical scopes are incorporated into the final v3.0.0 acceptance criteria. They are not skipped.

The active launch strategy is:

- launch **mainnet and a parallel public testnet in one coordinated v3.0.0 release window**;
- no standalone public-testnet launch first;
- no pre-mainnet 30-day public-testnet acceptance clock;
- v2.5 scale/P2P/GPU/high-cadence/replay/resilience gates must be complete inside the v3 evidence set;
- v2.6 programmability/smart-contract/VM/assets/economics/replay gates must be complete inside the v3 evidence set;
- freeze independent mainnet/testnet chain identities, genesis, bootnodes and endpoints on one exact integrated v3.0.0 release candidate;
- final launch authorization only through issue #781 with `GO_V3_DUAL_LAUNCH`.

Existing v2.4.x releases, branches and operational evidence remain development, validation and regression inputs. They must not be relabeled as v3.0.0.

## Current repository state

- Repository `VERSION`: `v2.4.0`.
- Cargo workspace version: `2.4.0`.
- Published v2.4.0 node/miner release: historical exact-release evidence; not the definitive public-launch identity.
- Current implementation work continues from v2.4.x/v2.4.1 through the incorporated v2.5 and v2.6 workstreams toward the final v3.0.0 candidate.
- v2.5 scope includes P2P scale, compact DAG relay, fast sync/pruning, deterministic mempool/fees, Mining Protocol v3, NVIDIA+AMD GPU mining, high cadence, replay, rolling upgrade, chaos and large-scale rehearsal/burn-in.
- v2.6 scope includes covenants, Contract Transaction v3, PulseScript, deterministic VM, parallel contract execution, based/verifiable applications, proof verification, native assets, contract state/RPC/events, programmable fees/economics, contract security and programmability replay/burn-in.
- External standalone miner remains supported separately from `pulsedagd`.
- Production wallet/custody readiness is controlled by #819.
- Mainnet/public dependency-security readiness is controlled by #803.
- Full integrated launch completion is controlled by #794.
- Final coordinated launch decision is controlled only by #781.
- v3.0.0 mainnet chain ID/genesis/bootnodes/endpoints: `TBD` until exact freeze.
- v3.0.0 parallel-testnet chain ID/genesis/bootnodes/endpoints: `TBD` until exact freeze.

### Legacy v2.4 validation markers

The following strings are retained because existing v2.4.x validation/hygiene surfaces still check them. They are **legacy compatibility state**, not the v3 launch model:

- Task31 compatibility marker: `PENDING_EXACT_CANDIDATE_EVIDENCE`.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- `contracts_enabled=false`.

These markers do not mean programmability is excluded from v3. The incorporated v2.6 programmability workstream is mandatory for the integrated v3.0.0 target.

## Start here

### Definitive v3 launch authority

- [v3.0.0 integrated authoritative roadmap](docs/ROADMAP_V3_0_0.md)
- [v2.5.0 scale/resilience source roadmap](docs/ROADMAP_V2_5_0.md)
- [v2.6.0 programmability source roadmap](docs/ROADMAP_V2_6_0.md)
- [v3.0 long-lived-core integration](docs/ROADMAP_V3_0_LONG_LIVED_CORE.md)
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

No historical release, public-testnet plan or old readiness flag can substitute for the exact integrated v3.0.0 scale/GPU, programmability, security, wallet, infrastructure, replay, burn-in and dual-network launch evidence required by #781/#794/#803/#819.
