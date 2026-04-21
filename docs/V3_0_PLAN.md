# PulseDAG v3.0 Plan

## Vision
PulseDAG v3.0 should move the project from a hardened node/miner prototype to an ecosystem-ready platform with:
- stable and observable network operation,
- a deterministic and secure execution layer,
- tooling for developers/operators,
- and governance/upgrade mechanics for long-term evolution.

## Strategic goals
1. **Protocol maturity**: define and lock v3.0 consensus/network rules.
2. **Execution readiness**: deliver contract runtime foundations with deterministic behavior.
3. **Operational excellence**: provide SLOs, dashboards, alerts, and runbooks.
4. **Security baseline**: complete audits, threat-model coverage, and release gates.
5. **Adoption enablement**: improve docs, SDK/API usability, and testnet experience.

## Scope by workstream

### 1) Core protocol and networking
- Finalize fork-choice, block admission, and anti-equivocation safeguards.
- Define wire protocol versioning and backward-compatibility windows.
- Introduce stronger peer scoring, abuse controls, and sync heuristics.
- Add replayable conformance vectors for p2p and consensus paths.

**Exit criteria**
- Protocol spec tagged `v3.0-rc` and frozen before GA.
- Mixed-version cluster upgrade tested end-to-end.

### 2) Execution layer (contracts)
- Ship initial VM/runtime abstraction with deterministic gas and metering.
- Implement transaction validation pipeline for contract calls/deploys.
- Add state transition test vectors and cross-node determinism checks.
- Define contract lifecycle APIs (deploy, call, events, receipts).

**Exit criteria**
- Determinism suite passes across all supported node profiles.
- Resource limits enforced with clear error taxonomy.

### 3) Data, state and storage
- Define state model versioning + migration paths.
- Implement snapshot/export and fast-restore workflows.
- Add corruption detection, repair strategy, and recovery drills.

**Exit criteria**
- Recovery objective validated in repeated chaos drills.
- Snapshot restore verified on clean and degraded environments.

### 4) Observability and SRE
- Publish v3.0 SLOs (availability, block latency, sync lag, tx finality proxy).
- Standardize metrics, tracing spans, and structured reason codes.
- Create production runbooks: incident triage, rollback, reindex, key rotation.

**Exit criteria**
- On-call playbook validated in game-day simulations.
- Alert noise budget and escalation paths tuned.

### 5) Security and governance
- Refresh threat model (network, runtime, keys, supply chain).
- Complete external audit cycle and remediation tracking.
- Define upgrade policy and emergency patch process.

**Exit criteria**
- All critical/high findings closed or risk-accepted with sign-off.
- Governance process documented for v3.x releases.

### 6) Developer and ecosystem tooling
- Stabilize RPC/API surface and provide versioning policy.
- Provide SDK examples and a local devnet bootstrap command.
- Publish compatibility matrix (node, miner, tooling).

**Exit criteria**
- New developer can deploy and interact with a sample contract in <30 min.
- API breaking-change policy enforced in CI.

## Delivery phases

### Phase A — Foundation hardening (Weeks 1–6)
- Protocol edge-case closure.
- Observability baseline.
- Security threat-model refresh.

### Phase B — Execution alpha (Weeks 7–12)
- Contract runtime alpha.
- Determinism and state transition suite.
- Devnet workflow for internal users.

### Phase C — Release candidate (Weeks 13–18)
- Performance tuning and soak tests.
- External audit + remediation.
- RC documentation + upgrade rehearsals.

### Phase D — GA readiness (Weeks 19–22)
- Final release checklist.
- Operational handoff and incident simulations.
- v3.0 tag + post-launch monitoring window.

## Milestones and gates
1. **M1: Protocol freeze** — specs and conformance vectors complete.
2. **M2: Contracts alpha** — deterministic execution in multi-node devnet.
3. **M3: Security gate** — audits complete, critical findings addressed.
4. **M4: RC sign-off** — soak/perf targets reached.
5. **M5: GA release** — operational SLOs monitored with active support window.

## KPIs
- Node crash-free runtime under stress tests.
- p95 sync lag in target topology.
- p95 time-to-inclusion / finality proxy.
- Contract execution determinism mismatch rate.
- Mean time to detect (MTTD) and recover (MTTR) in drills.

## Risks and mitigations
- **Runtime complexity risk** → feature-flag contracts until RC stability targets pass.
- **Performance regressions** → perf CI with threshold-based blocking.
- **Upgrade friction** → mandatory mixed-version rehearsal before each candidate.
- **Operational overload** → staged rollout + clear rollback playbooks.

## Suggested ownership model
- **Protocol lead**: consensus/network specs + conformance.
- **Runtime lead**: contracts VM, state transitions.
- **SRE lead**: observability, game days, runbooks.
- **Security lead**: threat model, audits, remediation.
- **Developer relations/tooling lead**: docs, SDK examples, onboarding.

## Immediate next actions (next 10 business days)
1. Create a v3.0 program board (workstreams × milestones × owners).
2. Break M1/M2 into epics with acceptance criteria.
3. Establish weekly architecture and release-readiness reviews.
4. Lock KPI definitions and start baseline measurement.
5. Publish a first public-facing v3.0 roadmap draft.
