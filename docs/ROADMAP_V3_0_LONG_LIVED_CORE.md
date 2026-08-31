# Roadmap v3.0 — Long-Lived Functional Core

Status: **SUPPLEMENTAL / INTEGRATED WITH `ROADMAP_V3_0_0.md`**

This document preserves the long-lived-core engineering philosophy behind v3.0. The authoritative launch sequence and complete integrated acceptance scope are defined in [`ROADMAP_V3_0_0.md`](ROADMAP_V3_0_0.md).

PulseDAG targets **v3.0.0 in Q4 2026**, with **mainnet and a parallel public testnet launched in one coordinated release window** after the final decision in #781.

## Path to v3.0.0

The engineering path includes the approved v2.5.0 and v2.6.0 workstreams:

`v2.4.x -> v2.5.0 scale/resilience -> v2.6.0 programmability -> v3.0.0 integrated release`

The v2.5.0 and v2.6.0 requirements are not discarded or bypassed. Their technical acceptance criteria are incorporated directly into `ROADMAP_V3_0_0.md` and the #794 completion program.

What is superseded is only the old public-launch sequencing that required a standalone public-testnet canary/30-day clock before later work and before mainnet.

## Long-lived-core philosophy retained

- v3.0 is earned by exact, reviewable evidence rather than by a version-number declaration;
- durability, migration safety, reproducibility and operator recovery take priority over feature expansion;
- consensus, storage, P2P, sync, mining, wallet, programmability and release boundaries must be documented and tested;
- release decisions must be tied to exact source/artifact identities;
- incompatible evidence from different SHAs, dependency graphs, chain identities, signing domains or protocol/contract activation contracts must not be combined;
- the external miner remains separate from the node;
- pool coordination/share accounting/payout logic remains separate infrastructure;
- unsupported compatibility claims are prohibited.

## Milestone context

### v2.4.x foundation

Provides the current protocol/node/miner/wallet/security implementation base and historical exact-release evidence.

### v2.5.0 workstream — mandatory input to v3

The following become part of the integrated v3 acceptance bar:

- P2P v3 and eclipse resistance;
- compact DAG relay;
- fast sync, pruning v2 and state bootstrap;
- deterministic mempool v3 and fee market;
- Mining Protocol v3;
- production NVIDIA and AMD/ATI GPU mining;
- multi-GPU/device runtime and GPU hardening;
- measured high-cadence operating envelope;
- >=1,000,000-block deterministic DAG replay;
- rolling upgrades/live activation;
- public RPC/API v3 and event streaming;
- automated chaos testing;
- reproducible supply-chain/release security;
- 25-node/16-miner adversarial rehearsal;
- exact-candidate 168-hour burn-in.

The old v2.5 standalone-public-testnet canary + 30-day acceptance requirement is replaced for the initial v3 launch by the exact-candidate pre-launch evidence model and the coordinated mainnet/testnet launch.

### v2.6.0 workstream — mandatory input to v3

The following also become part of the integrated v3 acceptance bar:

- programmability activation contract;
- UTXO Covenants v1;
- Contract Transaction v3;
- PulseScript;
- deterministic bounded Contract VM;
- parallel contract execution on the DAG;
- Based Applications;
- PulseProgs / Verifiable Programs;
- versioned ZK verification where included in the frozen scope;
- native asset/token standards;
- contract state/events/indexing/RPC;
- deterministic programmable resource/fee economy;
- contract security, fuzzing and adversarial corpus;
- programmable-fee integration with the external miner;
- final production monetary/economic policy;
- smart-contract validation workloads;
- >=1,000,000 programmable-operation deterministic replay;
- 30 accepted days of programmability-enabled exact-candidate burn-in evidence.

The old rule requiring completion of a prior standalone public-testnet clock before programmability implementation is removed. The technical/security/determinism requirements remain mandatory.

## v3.0.0 long-lived-core gates

v3.0.0 must not receive `GO_V3_DUAL_LAUNCH` unless the exact integrated candidate demonstrates:

- all mandatory incorporated v2.5 scale/resilience/GPU gates;
- all mandatory incorporated v2.6 programmability/smart-contract/economic gates;
- no unresolved Sev-1 consensus, state, storage, replay, sync, mining, wallet, contract, proof-system or operator-safety issue;
- deterministic replay/order/state reconstruction on the final protocol and contract semantics;
- restart, snapshot, restore, pruning and clean-bootstrap recovery;
- multi-node/multi-miner convergence under normal and adversarial conditions;
- CPU/NVIDIA/AMD PoW correctness equivalence;
- reproducible node/miner/wallet release artifacts with checksums/provenance;
- documented storage migration and rollback boundaries;
- public/operator/development RPC boundaries and fail-closed public-safe exposure;
- exact dependency/security/reachability review under #803;
- production wallet/custody acceptance under #819;
- production infrastructure, monitoring, incident response and rollback readiness under #794;
- independent frozen mainnet and parallel-testnet chain/genesis/network identities;
- final launch review and `GO_V3_DUAL_LAUNCH` only in #781.

## Parallel public testnet role

The parallel public testnet remains essential:

- it launches alongside mainnet in the same v3.0.0 release window;
- it remains the permanent public validation network for future upgrades, contract/proof changes and application testing;
- future consensus/network/programmability upgrades should rehearse there before separately authorized mainnet activation;
- it is not a prerequisite standalone 30-day launch phase before the initial v3 mainnet start.

## Smart-contract boundary

Smart contracts and programmability are part of the integrated v3.0.0 target through the incorporated v2.6 workstream. They therefore require the complete deterministic execution, resource, state, replay, security, wallet and recovery evidence defined in `ROADMAP_V3_0_0.md` before final launch GO.

No version number alone enables contract semantics. The final protocol/contract/VM/proof versions and activation contract must be frozen on the exact v3 candidate.

## Miner and pool boundary

The miner remains external for v3.0.0. The node provides mining jobs/templates, deterministic fee economics and submission validation; the miner performs work and returns submissions.

Pool membership, share accounting, vardiff, authentication and payouts do not belong inside the canonical standalone miner or node.

## Promotion rule

A candidate may be promoted to v3.0.0 only when the complete integrated v2.5 + v2.6 + v3 launch evidence is exact, current and reviewable under `ROADMAP_V3_0_0.md`, #794, #803, #819 and #781.

Missing, stale or contradicted evidence requires delay/rebaseline rather than weakening the gates.
