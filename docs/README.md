# PulseDAG documentation

The repository's **current implementation/version surface remains v2.4.x**, while the definitive public-launch target is **v3.0.0 in Q4 2026**.

The authoritative launch model is **mainnet + a parallel public testnet in one coordinated v3.0.0 release window**. The earlier standalone-public-testnet-first sequence and pre-mainnet 30-day public-testnet clock are superseded.

## Path to v3.0.0

The approved engineering path is:

`v2.4.x -> v2.5.0 scale/resilience workstream -> v2.6.0 programmability workstream -> v3.0.0 integrated release`

The v2.5.0 and v2.6.0 roadmaps are part of the path to v3.0.0, and their technical requirements are incorporated into the final v3 launch criteria.

- [`ROADMAP_V2_5_0.md`](ROADMAP_V2_5_0.md) supplies the scale, P2P, GPU-mining, high-cadence, replay, chaos, supply-chain and large-rehearsal requirements.
- [`ROADMAP_V2_6_0.md`](ROADMAP_V2_6_0.md) supplies the programmability, smart-contract, covenant, VM, verifiable-application, asset, fee/economic and programmability-burn-in requirements.
- [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md) integrates both workstreams into the definitive release/launch contract.

The old v2.5 standalone-public-testnet canary/30-day requirement and the old v2.6 dependency on that clock are not carried forward. Their technical acceptance gates are.

## Definitive v3.0.0 launch authority

- [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md) — authoritative integrated Q4 launch roadmap.
- [`ROADMAP_V3_0_LONG_LIVED_CORE.md`](ROADMAP_V3_0_LONG_LIVED_CORE.md) — long-lived-core philosophy and v2.5/v2.6 integration summary.
- [`runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md`](runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md) — coordinated mainnet/testnet launch runbook.
- [`../configs/v3-launch/README.md`](../configs/v3-launch/README.md) — placeholder network-configuration freeze authority; not deployable until exact identities are frozen.
- Root [`../SECURITY.md`](../SECURITY.md) — v3 public/mainnet security boundary.

Issue authority:

- #781 — sole final `GO_V3_DUAL_LAUNCH` authority;
- #794 — integrated v3 implementation/release/security/wallet/infrastructure/rehearsal completion;
- #803 — v3 dependency/reachability mainnet/public security gate;
- #819 — v3 production wallet/custody gate.

## Current v2.4.x implementation authority

These remain important implementation/history references:

- [`ROADMAP_V2_4_0.md`](ROADMAP_V2_4_0.md)
- [`PROTOCOL_ACTIVATION_V2_4_0.md`](PROTOCOL_ACTIVATION_V2_4_0.md)
- [`BLOCK_HEADER_V2_CANONICALIZATION.md`](BLOCK_HEADER_V2_CANONICALIZATION.md)
- [`TRANSACTION_PROTOCOL_V2.md`](TRANSACTION_PROTOCOL_V2.md)
- [`DIFFICULTY_RETARGET_V2_4_0.md`](DIFFICULTY_RETARGET_V2_4_0.md)
- [`VERSION_MATRIX.md`](VERSION_MATRIX.md)

The published v2.4.0 release and later v2.4.x/v2.4.1 work are development, validation and regression inputs. Existing v2.4 artifacts/evidence must not be relabeled as v3.0.0.

## Operator documentation

- [`RUNBOOK.md`](RUNBOOK.md)
- [`API_V1.md`](API_V1.md)
- [`POW_SPEC_FINAL.md`](POW_SPEC_FINAL.md)
- [`POW_CURRENT_PATH.md`](POW_CURRENT_PATH.md)

Production v3 operator instructions must ultimately be bound to the exact frozen v3.0.0 artifact and separate mainnet/testnet identities.

## Evidence and launch gates

- [`RELEASE_EVIDENCE.md`](RELEASE_EVIDENCE.md)
- [`BURN_IN_GATE.md`](BURN_IN_GATE.md) — historical/private evidence policy input; final v3 burn-in requirements are defined by `ROADMAP_V3_0_0.md`.

The integrated v3 acceptance bar includes, among other gates:

- NVIDIA + AMD/ATI production GPU correctness;
- >=1,000,000-block DAG replay;
- 25-node/16-miner adversarial rehearsal;
- >=168-hour unchanged-candidate release burn-in;
- covenants, Contract Transaction v3, PulseScript and deterministic contract VM;
- programmable assets/economics/security;
- >=1,000,000 programmable-operation replay;
- 30 accepted days of programmability-enabled exact-candidate pre-launch evidence;
- production wallet/custody and final dependency/security closeout.

## Legacy v2.4 compatibility markers

Legacy tooling may still require:

- `PENDING_EXACT_CANDIDATE_EVIDENCE`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

These are compatibility state for old validation surfaces, not the v3 launch state.

## Maintenance and history

- [`REPOSITORY_STANDARDS.md`](REPOSITORY_STANDARDS.md)
- [`archive/README.md`](archive/README.md)
- [`codex_tasks/`](codex_tasks/)

Historical evidence remains immutable provenance. Consensus, network, wallet and programmability changes require fresh explicitly versioned activation boundaries and exact-candidate evidence.
