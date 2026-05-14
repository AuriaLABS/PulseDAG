# PulseDAG Version Matrix

This matrix keeps release positioning clear across the current v2.2.x hardening line, the v2.3.0 private-testnet readiness decision, and the long-lived v3.0 core roadmap.

## Current baseline

| Area | Current value |
| --- | --- |
| Workspace release | `VERSION` is `v2.2.15`; Cargo workspace version is `2.2.15`; license metadata remains `ISC` |
| Current milestone | v2.2.15 sustained P2P multi-node rehearsal |
| Previous milestone | v2.2.14 storage/replay/snapshot/restore/pruning/migration-policy hardening closure |
| Following milestone | v2.2.16 miner/node contract hardening |
| Private-testnet readiness decision | v2.3.0 |
| Private-testnet stable line | v2.4.x |
| Public-testnet preparation | v2.5.x |
| Public-testnet candidate and long soak | v2.6.x |
| Protocol freeze | v2.7.x |
| v3.0 release candidates | v2.8.x |
| Long-lived functional core | v3.0.0 |
| Miner architecture | External standalone miner |
| Smart contracts | Out of scope until after a 30-day stable testnet burn-in |
| Pool logic in miner | Out of scope / not allowed |

## Release boundaries

| Version | Purpose | Status framing |
| --- | --- | --- |
| v2.2.8 | Hardening baseline closure | Pre-private-testnet hardening |
| v2.2.9 | Private-testnet rehearsal closure | Rehearsal only |
| v2.2.10 | Final PoW completion | PoW finalized, P2P not yet complete |
| v2.2.11 | P2P completion | Networking/sync completion closure; not official readiness |
| v2.2.12 | Full private-testnet rehearsal and hardening | Multi-node/operator rehearsal, sustained validation, runbook hardening, and evidence capture |
| v2.2.13 | Consensus/DAG safety audit | Closeout checklist for DAG invariant tests, block structural validation tests, transaction validation negative-path tests, orphan adoption tests, tip selection tests, replay/order-independence tests, block acceptance taxonomy tests, required Cargo checks, [DAG safety invariants](DAG_SAFETY_INVARIANTS_V2_2_13.md), and compatibility-claim review |
| v2.2.14 | Storage/replay hardening | Closes deterministic replay ordering, snapshot/restore/pruning safety, explicit storage schema policy, migration compatibility errors, testnet real-libp2p defaults, release evidence scripting, external miner boundary, no contract runtime, and no pool logic in miner |
| v2.2.15 | Sustained P2P multi-node rehearsal | Long-running P2P churn, restart/rejoin, lag recovery, convergence, peer diagnostics, and chain-id isolation evidence |
| v2.2.16 | Miner/node contract hardening | Stable external miner/node RPC contract, submission semantics, diagnostics, and optional GPU backlog only if canonical |
| v2.2.17 | API/operator/security hardening | Public/operator/dev RPC boundary documentation, safe defaults, auth/rate-limit expectations, and operator incident workflows |
| v2.2.18 | Private-testnet RC | Release-candidate evidence bundle and go/no-go checklist for the v2.3.0 readiness decision |
| v2.3.0 | Private-testnet readiness decision only | Decision milestone; not an automatic public launch |
| v2.4.x | Private-testnet stable line | Conservative stability and evidence-driven bug-fix line for the private testnet |
| v2.5.x | Public-testnet preparation | Public operator documentation, bootstrap policy, monitoring, release reproducibility, and support readiness |
| v2.6.x | Public-testnet candidate and long soak | Long soak, incident tracking, rollback drills, and no unresolved Sev-1 consensus/sync incident before promotion |
| v2.7.x | Protocol freeze | Consensus, P2P, storage, pruning, miner contract, and RPC boundaries frozen except for documented safety fixes |
| v2.8.x | v3.0 release candidates | Reproducible artifacts, migration rehearsals, snapshot/restore evidence, multi-node/multi-miner evidence, and final operator sign-off |
| v3.0.0 | Long-lived functional core | Stable PulseDAG core intended to run for years with documented upgrade, rollback, storage, and operational policies |

## v2.2.11 closeout scope

v2.2.11 closed the P2P completion path for block announce/request/data flow, transaction relay, tip exchange, missing parent recovery, orphan handling, peer scoring/backoff, duplicate suppression, P2P diagnostics, and the reproducible three-node rehearsal scripts.

## v2.2.12 rehearsal scope

v2.2.12 consumes the v2.2.11 P2P completion outputs and performs the full private-testnet rehearsal and hardening pass. It should validate longer-running multi-node and multi-operator scenarios, restart/rejoin behavior, sync convergence, diagnostics quality, operational runbooks, and release evidence without claiming v2.3.0 readiness early.

## v2.2.13 consensus/DAG safety audit

v2.2.13 follows v2.2.12 as an intermediate consensus/DAG safety audit before the v2.3.0 readiness decision. Its closeout checklist requires DAG invariant tests, block structural validation tests, transaction validation negative-path tests, orphan adoption tests, tip selection tests, replay/order-independence tests, block acceptance taxonomy tests, `cargo fmt --check`, `cargo test -p pulsedag-core`, `cargo test --workspace`, and `cargo build --workspace`. If this release bumps versions, `VERSION` must be `v2.2.13` and `[workspace.package].version` must be `2.2.13`. The detailed audit document is [DAG Safety Invariants v2.2.13](DAG_SAFETY_INVARIANTS_V2_2_13.md), and the closeout checklist is [Closing Checklist v2.2.13](CLOSING_CHECKLIST_V2_2_13.md). It must clearly document that PulseDAG currently has a DAG structure and deterministic tip policy, but is not claiming full Kaspa or GHOSTDAG compatibility; kHeavyHash/PoW alignment does not imply consensus compatibility. No smart contracts or pool logic are added, and the miner remains external.

## v2.2.14 through v2.2.18 hardening path

v2.2.14 through v2.2.18 extend the hardening line before the v2.3.0 readiness decision. v2.2.14 is now the storage/replay closure and v2.2.15 is the current sustained P2P rehearsal milestone:

- v2.2.14 is the storage/replay hardening release: it closes deterministic persisted-block replay ordering, snapshot/restore/pruning safety, storage schema migration policy, testnet real-libp2p defaults, and repeatable evidence scripting while preserving the external miner boundary and the no-contract/no-pool guardrails.
- v2.2.15 is the current sustained P2P multi-node rehearsal release: it proves operation under churn, restart/rejoin, lag recovery, convergence, peer diagnostics, and chain-id isolation scenarios.
- v2.2.16 stabilizes the external miner/node contract and keeps optional GPU work as backlog unless it is canonical and evidence-backed.
- v2.2.17 hardens API, operator, and security boundaries, including public/operator/dev RPC separation.
- v2.2.18 packages the private-testnet RC evidence bundle and go/no-go checklist.

## v2.3.0 readiness decision

v2.3.0 remains the private-testnet readiness decision milestone. Evidence gathered during v2.2.12 through v2.2.18 can inform that decision, but v2.3.0 is not an automatic public launch and must publish known limitations, operator requirements, rollback plan, and an evidence index. The v2.2.15 checklist explicitly requires `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo build --workspace`, release evidence script output, a three-node local rehearsal, a five-node local rehearsal when practical, restart/rejoin evidence, lagging-node recovery evidence, peer churn evidence, chain-id isolation evidence, sync convergence evidence, and no unresolved Sev-1 consensus or sync defect before closeout.

## v2.4.x through v2.8.x path to v3.0

- v2.4.x is the private-testnet stable line for conservative fixes and evidence updates.
- v2.5.x prepares public-testnet operations, documentation, bootstrap policy, monitoring, support, and release reproducibility.
- v2.6.x is the public-testnet candidate and long soak line with incident tracking, rollback drills, and no unresolved Sev-1 consensus/sync incident before promotion.
- v2.7.x freezes protocol, storage, pruning, miner contract, and RPC boundaries except for documented safety fixes.
- v2.8.x produces v3.0 release candidates with reproducible artifacts, migration rehearsals, snapshot/restore evidence, multi-node/multi-miner evidence, and final operator sign-off.

## v3.0.0 stable network target gates

v3.0.0 is not a marketing milestone. It is the stable network target for the long-lived PulseDAG core, capable of running for years with stable node, PoW, external miner, P2P, sync, storage, snapshots, pruning policy, operator RPC, release evidence, and upgrade policy.

v3.0.0 must require:

- No unresolved Sev-1 consensus or sync incident.
- Completed 30-day stable testnet burn-in before smart contract implementation begins.
- Reproducible release artifacts.
- Documented upgrade and rollback policy.
- Documented storage migration policy.
- Snapshot/restore evidence.
- Multi-node and multi-miner evidence.
- Public, operator, and development RPC boundary documentation.

The detailed gate roadmap is [PulseDAG v3.0.0 roadmap and gates](ROADMAP_V3_0_0.md). The earlier long-lived core roadmap remains available as [Roadmap v3.0 — Long-Lived Functional Core](ROADMAP_V3_0_LONG_LIVED_CORE.md).

## Guardrails

- Do not add smart contracts before the 30-day stable testnet burn-in is complete.
- Do not add pool coordination logic inside `pulsedag-miner`.
- Keep miner external and standalone.
- Do not claim official private-testnet readiness before v2.3.0.
- Do not claim full Kaspa or GHOSTDAG compatibility unless that compatibility is explicitly implemented, tested, and documented.
- Avoid consensus rule changes in v2.2.x unless they fix a clear safety bug and include documented test evidence.
- v2.3.0 is a readiness decision, not an automatic public launch.
- v3.0 must prefer durability, migration safety, reproducibility, and operator evidence over feature expansion.
