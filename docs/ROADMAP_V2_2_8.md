# Roadmap v2.2.8 — Ambitious Pre-Private-Testnet Hardening

v2.2.8 is the **major hardening release before v2.3.0**. It expands the pre-private-testnet scope substantially, but it is still **not** the official full private testnet launch release.

## Release positioning

- **Current target release:** v2.2.8.
- **Role:** ambitious hardening of PoW, mining interfaces, P2P propagation, and sync robustness.
- **Not yet:** official full private testnet launch.
- **v2.3.0 remains:** the milestone for official private testnet launch and longer burn-in.

## Scope guardrails (must remain true)

- Do **not** add smart contracts.
- Do **not** enable smart-contract runtime.
- Do **not** add pool logic inside the miner.
- Miner remains an **external standalone application**.
- Keep v2.2.8 framed as hardening/preparation, not full private-testnet sign-off.

## Outcome goal for v2.2.8

Deliver a hardened and testable foundation so v2.3.0 can focus on official private testnet rollout, sustained burn-in, and launch-gate validation rather than first-time core plumbing.

## PR-sized workstreams

Each workstream is expected to map to one or more focused PRs with explicit acceptance checks.

### WS1 — PoW hardening and canonical preimage

**Objective:** remove ambiguity and tighten consensus-critical PoW inputs.

- Define and freeze the **canonical PoW preimage** format.
- Ensure deterministic serialization across node and external miner boundaries.
- Validate rejection behavior when preimage inputs are malformed, inconsistent, or incomplete.
- Add doc-level examples for expected preimage field ordering and byte encoding.

**Acceptance criteria**

- Canonical preimage specification is documented and versioned for v2.2.8.
- Node validation path enforces canonical preimage assumptions consistently.
- External miner integration docs reference the same canonical format.

### WS2 — Target/difficulty foundation

**Objective:** establish durable target/difficulty semantics ahead of broader network burn-in.

- Document network target representation, conversion rules, and edge cases.
- Validate difficulty/target checks in block acceptance path under nominal and boundary values.
- Add regression-oriented test vectors for target parsing/normalization behavior.
- Clarify retarget-adjacent assumptions that remain deferred to v2.3.0 burn-in validation.

**Acceptance criteria**

- Target/difficulty behavior is documented as consensus-facing.
- Validation logic has deterministic pass/fail behavior for boundary cases.
- Deferred retarget stress scope is explicitly linked to v2.3.0.

### WS3 — Unified block acceptance validation

**Objective:** consolidate block acceptance checks into a clear, auditable validation flow.

- Define an explicit validation sequence covering header, PoW, ancestry linkage, and structural checks.
- Unify error classification/codes for rejection reasons.
- Ensure validation decisions are emitted through consistent logs/telemetry hooks.
- Reduce divergence between local block production acceptance and remote block acceptance.

**Acceptance criteria**

- A single documented validation pipeline exists.
- Rejection reasons are stable enough for operator/debug workflows.
- Local-vs-remote acceptance parity checks are documented.

### WS4 — Mining RPC hardening and external miner readiness

**Objective:** make mining RPC safe and predictable for external miners.

- Harden `getblocktemplate`-style flow semantics (freshness window, required fields, stale handling).
- Harden `submitblock`-style semantics (idempotency expectations, deterministic error responses).
- Add request/response schema validation guidance and example payloads.
- Document operational behavior for miner retries, stale templates, and transient node errors.

**Acceptance criteria**

- Mining RPC behavior is explicitly documented for external standalone miners.
- Error handling expectations are deterministic and integration-friendly.
- No miner-internal pool logic is introduced.

### WS5 — P2P block inventory propagation

**Objective:** establish robust block inventory announcement and fetch behavior.

- Introduce/complete explicit inventory announcement flow for newly accepted blocks.
- Define peer response behavior for unknown inventory and duplicate announcements.
- Add anti-spam/backpressure expectations for inventory broadcasts.
- Document propagation observability points (announcement, request, receive, accept/reject).

**Acceptance criteria**

- Block inventory propagation path is documented and testable.
- Duplicate/unknown inventory behavior is deterministic.
- Minimal propagation telemetry is available for smoke diagnostics.

### WS6 — DAG sync missing-parent recovery and orphan handling

**Objective:** make sync resilient when parent data is missing or arrives out of order.

- Define missing-parent detection and retry/request strategy.
- Define orphan block staging, re-check, expiration, and cleanup behavior.
- Add bounded resource controls for orphan/missing-parent queues.
- Document replay/reconciliation logic once missing parents arrive.

**Acceptance criteria**

- Missing-parent recovery strategy is documented and implemented to a stable baseline.
- Orphan handling policy is explicit and bounded.
- Recovery behavior is verifiable in multi-node drills.

### WS7 — Private testnet config profiles

**Objective:** provide repeatable node profile presets for private-testnet rehearsals.

- Define configuration profiles for local-dev, CI-smoke, and private-testnet rehearsal modes.
- Document profile deltas (networking, PoW cadence, logging verbosity, safety limits).
- Ensure profiles preserve external-miner architecture boundaries.

**Acceptance criteria**

- Config profiles are documented and selectable.
- Profile intent and safe-use guidance are clear.
- v2.3.0 rollout profile gaps are explicitly listed.

### WS8 — Multi-node local test lab

**Objective:** enable repeatable multi-node rehearsals before official v2.3.0 launch.

- Provide/update local multi-node orchestration for 3–5 node topologies.
- Include deterministic startup ordering and fault-injection hooks (restart, temporary disconnect).
- Document expected steady-state behavior and convergence checks.

**Acceptance criteria**

- A repeatable multi-node local lab procedure exists.
- Operators can run baseline topology and basic fault drills.
- Known limitations are documented for v2.3.0 follow-up.

### WS9 — Observability and smoke testing

**Objective:** ensure hardening work is measurable and quickly regressible.

- Define minimum metrics/events/log fields for PoW, validation, propagation, and sync.
- Add/update smoke-test playbooks for single-node and multi-node hardening checks.
- Add release-candidate checklist entries for mandatory smoke pass before tag.

**Acceptance criteria**

- Observability minimums are documented.
- Smoke playbooks exist and are runnable by maintainers.
- v2.2.8 closeout criteria reference these smoke checks.

### WS10 — Release notes and closure artifacts

**Objective:** close v2.2.8 with auditable evidence and clear v2.3.0 handoff.

- Publish v2.2.8 notes summarizing what hardened vs what remains open.
- Include explicit “not yet full private testnet” language.
- Provide v2.3.0 dependency handoff checklist.

**Acceptance criteria**

- Release notes are present and explicit about boundaries.
- Remaining risks and deferred items are enumerated.
- v2.3.0 dependency list is updated and reviewable.

## v2.3.0 dependency and handoff (explicit)

v2.2.8 is a prerequisite hardening layer, but **does not replace** v2.3.0 gates.

v2.3.0 still owns:

- Official private testnet launch decision.
- Longer-duration multi-node burn-in.
- Full operational readiness sign-off (runbooks, incident procedures, extended drills).
- Final confidence gates for sustained propagation/sync behavior under prolonged load.

## Definition of done for v2.2.8 roadmap closure

- `docs/ROADMAP_V2_2_8.md` is accepted and aligned with repository release framing.
- Each workstream is either completed or moved with explicit rationale to v2.3.0 backlog.
- Smoke and observability checks are run at least once on multi-node local lab.
- Closure notes clearly state: **v2.2.8 hardens preconditions; v2.3.0 is official private testnet milestone**.
