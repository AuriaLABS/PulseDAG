# PulseDAG Version Matrix

This matrix keeps release positioning clear across the current v2.2.x hardening line, the v2.3.0 private-testnet readiness decision, and the long-lived v3.0 core roadmap.

## Current baseline

| Area | Current value |
| --- | --- |
| Workspace release | `VERSION` is `v2.2.17`; Cargo workspace version is `2.2.17`; license metadata remains `ISC` |
| Current milestone | v2.2.17 API/operator/security hardening closeout |
| Previous milestone | v2.2.16 miner/node contract hardening, evidence bundle passed |
| Following milestone | v2.2.18 private-testnet RC |
| Private-testnet RC | v2.2.18 |
| Private-testnet readiness decision | v2.3.0 |
| Private-testnet stable line | v2.4.x |
| Public-testnet preparation | v2.5.x |
| Public-testnet candidate and long soak | v2.6.x |
| Protocol freeze | v2.7.x |
| v3.0 release candidates | v2.8.x |
| Long-lived functional core | v3.0.0 |
| Miner architecture | External standalone miner |
| GPU mining | Optional experimental external-miner backend only after the canonical PoW adapter exists; non-blocking, feature-gated if present, and every GPU-found nonce/result CPU-verified before submit |
| Smart contracts | Out of scope until after a 30-day stable testnet burn-in |
| Pool logic in miner | Pool coordination logic inside `pulsedag-miner` is out of scope / not allowed |

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
| v2.2.15 | Sustained P2P multi-node rehearsal | Evidence bundle passed for cargo checks, 3-node rehearsal, churn/rejoin, lag recovery, convergence, peer diagnostics, and chain-id isolation |
| v2.2.16 | Miner/node contract hardening | Completed milestone feeding API/operator/security hardening scope and evidence baseline |
| v2.2.17 | API/operator/security hardening | Finalized closeout: RPC endpoint inventory, public/operator/admin profiles, admin-disabled defaults, unsafe exposure blocking, optional operator auth tests, rate/request-size tests, CORS/bind-address tests, diagnostics redaction tests, `/release` and `/readiness` hardening, secure runbook completion, smoke validation, and evidence bundle generation |
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

v2.2.14 through v2.2.18 extend the hardening line before the v2.3.0 readiness decision. v2.2.14 is the storage/replay closure, v2.2.15 passed sustained P2P rehearsal evidence, v2.2.16 closed miner/node contract hardening, and v2.2.17 is finalized as the API/operator/security hardening closeout:

- v2.2.14 is the storage/replay hardening release: it closes deterministic persisted-block replay ordering, snapshot/restore/pruning safety, storage schema migration policy, testnet real-libp2p defaults, and repeatable evidence scripting while preserving the external miner boundary and the no-contract/no-pool guardrails.
- v2.2.15 is the sustained P2P multi-node rehearsal release: its evidence bundle passed `cargo fmt`, `cargo test`, `cargo build`, 3-node rehearsal, churn/rejoin, lag recovery, convergence, peer diagnostics, and chain-id isolation. It is not a v2.3.0 readiness claim by itself.
- v2.2.16 stabilized the external miner/node contract, canonical mining template semantics, submit validation, miner diagnostics, CPU miner behavior, restart/reconnect evidence, optional performance evidence under `artifacts/`, and optional GPU work as external-miner-only backlog unless it is canonical and evidence-backed.
- v2.2.17 hardens API, operator, and security boundaries, including public/operator/admin endpoint separation, lock-down defaults, optional operator auth, request/rate bounds, CORS posture, safe config validation, and diagnostics/readiness hardening.
- v2.2.18 packages the private-testnet RC evidence bundle and go/no-go checklist.

## v2.3.0 readiness decision

v2.3.0 remains the private-testnet readiness decision milestone. Evidence gathered during v2.2.12 through v2.2.18 can inform that decision, but v2.3.0 is not an automatic public launch and must publish known limitations, operator requirements, rollback plan, and an evidence index. v2.2.17 must contribute API/operator/security hardening evidence, not readiness by itself.

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
- GPU mining, if implemented, must be optional external-miner functionality, feature-gated, CPU-verified before submit, and not required for default builds or mandatory evidence when no GPU is present.
- Do not claim official private-testnet readiness before v2.3.0.
- Do not claim full Kaspa or GHOSTDAG compatibility unless that compatibility is explicitly implemented, tested, and documented.
- Avoid consensus rule changes in v2.2.x unless they fix a clear safety bug and include documented test evidence.
- v2.3.0 is a readiness decision, not an automatic public launch.
- v3.0 must prefer durability, migration safety, reproducibility, and operator evidence over feature expansion.
