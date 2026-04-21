# PulseDAG v3.0 Program Board

## Planning window
- **Program start:** April 22, 2026
- **Target v3.0 GA:** November 20, 2026
- **Cadence:** 2-week execution sprints, milestone reviews every 6 weeks

## Milestones
- **M1 — Architecture freeze (May 29, 2026)**
- **M2 — Feature complete (July 24, 2026)**
- **M3 — Scale + security hardening (September 18, 2026)**
- **M4 — Release readiness + GA (November 20, 2026)**

## Workstreams × milestones × owners

| Workstream | Owner | M1 — Architecture freeze | M2 — Feature complete | M3 — Scale + security hardening | M4 — Release readiness + GA |
|---|---|---|---|---|---|
| Node core performance | Core Protocol Lead | Profile hot paths; lock v3 data-flow design | Parallelize validation/apply stages; reduce block processing latency | Sustained-load tuning under 3–5 node testnet | Final perf sign-off and production defaults |
| Consensus + DAG correctness | Consensus Lead | Formalize acceptance/reorg invariants | Implement finalized DAG/selection rule updates | Fault-injection and long-reorg resilience tests | Consensus audit closeout + release checklist |
| P2P + sync reliability | Networking Lead | Finalize protocol/message compatibility matrix | Deliver fast-sync + gossip convergence improvements | Churn/partition soak tests with recovery SLOs | Mainnet-like bootstrap validation |
| Storage + snapshot/pruning | Storage Lead | Freeze snapshot format and prune policy | Complete incremental snapshot + restore path | Corruption/recovery drills; large-state benchmarks | Snapshot operator guide + retention defaults |
| Mining + PoW controls | Mining Lead | Lock PoW policy knobs and miner/runtime API | Land autotune-safe controls + policy guardrails | Multi-miner stability burn-in and anti-spike protections | Production mining profile + rollback playbook |
| RPC/API + tooling | Platform/API Lead | Version v3 API surface and deprecation map | Finish endpoint parity, docs, and client compatibility tests | Rate-limit/abuse hardening + observability hooks | GA API contract published and tagged |
| Security + compliance | Security Lead | Threat model refresh and prioritized mitigations | Complete critical mitigation backlog | External review + dependency/license sweep | Release security report + sign-off |
| SRE/observability + operations | SRE Lead | Define SLOs, alert taxonomy, and runbook gaps | Ship dashboards + alerting + on-call rehearsal | 30-day pre-GA burn-in against SLO targets | Go-live readiness review and handoff |
| QA/release engineering | QA/Release Lead | Freeze test strategy and quality gates | CI matrix complete (unit/integration/e2e/replay) | Release candidate train + regression burn-down | GA candidate promotion and post-release watch |
| Ecosystem/devrel | Ecosystem Lead | Publish v3 migration scope for partners | SDK/examples updated to feature-complete API | Partner testnet validation and issue closure | Migration guide + launch communications |

## Ownership model
- **Single-threaded ownership:** each workstream has one directly accountable owner.
- **Cross-functional support:** each owner maps contributors from Core, SRE, QA, and Security.
- **Escalation path:** blockers >3 business days escalate to Program Lead in weekly steering.

## Exit criteria for v3.0 GA
1. All M4 rows marked complete with owner sign-off.
2. No Sev-1/Sev-2 defects open for consensus, sync, storage, or mining.
3. Burn-in stability target met for at least 30 consecutive days.
4. Operator runbook and rollback drill completed successfully.
