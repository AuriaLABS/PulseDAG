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

## Production economy, reward settlement, finality, genesis and network freeze

The v3 launch is not defined by source defaults alone. These documents form the production freeze contract:

- [`MONETARY_POLICY_V3_0_0.md`](MONETARY_POLICY_V3_0_0.md) — approved mainnet issuance, subsidy schedule, coinbase maturity, fees/burn and supply-accounting authority;
- [`MONETARY_SCORE_V3_0_0.md`](MONETARY_SCORE_V3_0_0.md) — authoritative ordered-DAG monetary score and cadence-to-economic-time mapping;
- [`REWARD_SETTLEMENT_V3_0_0.md`](REWARD_SETTLEMENT_V3_0_0.md) — amountless chain-bound reward claims, deferred subsidy/fee settlement and synthetic reward UTXO contract;
- [`FINALITY_V3_0_0.md`](FINALITY_V3_0_0.md) — economic-time finality, selected-chain anchor, protected-prefix conflict handling and finality/pruning separation;
- [`GENESIS_V3_0_0.md`](GENESIS_V3_0_0.md) — deterministic genesis inputs, outputs, allocation and independent-reproduction contract;
- [`NETWORK_PARAMETERS_V3_0_0.md`](NETWORK_PARAMETERS_V3_0_0.md) — consensus/network identity, cadence, finality/conflict/pruning, ports, seeds, endpoints, wallet domains, checkpoints and separation matrix;
- [`V3_0_0_LAUNCH_MANIFEST.md`](V3_0_0_LAUNCH_MANIFEST.md) — single exact release/economic/finality/genesis/network/artifact/evidence authority;
- [`runbooks/V3_0_0_GENESIS_CEREMONY.md`](runbooks/V3_0_0_GENESIS_CEREMONY.md) — reproducible mainnet/testnet genesis ceremony.

The launch manifest currently remains `PRE_FREEZE`. That is a valid development state but means **`launch_ready=false`**. Final GO is prohibited until the manifest is `FROZEN`, every launch-required `TBD` is resolved, all network-separation assertions pass, and the exact candidate reproduces the recorded policy/finality/genesis/config identities.

The current v2.4-derived source includes development economic/genesis behavior. It must not become mainnet policy implicitly. In particular, the production freeze must explicitly disposition the development `genesis-treasury` allocation, genesis supply constant, current subsidy/halving constants, runtime timestamp construction and legacy amount-bearing coinbase path.

The v3 monetary/reward path is deliberately separate: monetary score is state-derived from canonical ordered-DAG position, a block commits an amountless beneficiary reward claim, subsidy/fees are derived by consensus, and a synthetic reward UTXO becomes spendable only after the frozen finality rule and 3,600-economic-second maturity are both satisfied.

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

Production v3 operator instructions must ultimately be bound to the exact frozen v3.0.0 artifact, monetary policy, finality policy, genesis and separate mainnet/testnet identities.

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
- frozen monetary policy and independent supply-accounting vectors;
- deterministic monetary-score/cadence equivalence;
- deferred reward-settlement replay and finality+maturity vectors;
- economic-time finality equivalence, persistence and protected-prefix conflict tests;
- separately frozen pruning/checkpoint semantics;
- reproducible mainnet/testnet genesis ceremony;
- complete network-parameter/bootnode/endpoint freeze;
- production wallet/custody and final dependency/security closeout.

Static consistency is checked by `scripts/validate_v3_0_0_launch_plan.py`; monetary/genesis/network freeze readiness by `scripts/validate_v3_0_0_network_freeze.py`; deferred reward settlement by `scripts/validate_v3_0_0_reward_settlement.py`; and economic-time finality by `scripts/validate_v3_0_0_finality.py`.

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

Historical evidence remains immutable provenance. Consensus, monetary, finality, genesis, network, wallet and programmability changes require fresh explicitly versioned activation boundaries and exact-candidate evidence.
