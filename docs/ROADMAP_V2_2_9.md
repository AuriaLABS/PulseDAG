# Roadmap v2.2.9 — Private-Testnet Rehearsal Milestone

v2.2.9 is the **real private-testnet rehearsal release** between the v2.2.8 hardening baseline and the official private-testnet readiness milestone in v2.3.0.

## Release positioning

- **v2.2.8:** hardening baseline.
- **v2.2.9:** private-testnet rehearsal (multi-node behavior, propagation, sync, recovery, and operational visibility).
- **v2.3.0 remains:** the official complete private-testnet readiness milestone.

## Scope guardrails (must remain true)

- Keep miner architecture external (no pool logic in-node).
- Keep rehearsal scope focused on private-testnet execution readiness.
- Do not frame v2.2.9 as final private-testnet sign-off.
- Preserve v2.3.0 as the milestone for official readiness closure.

## Outcome goal for v2.2.9

Prove that core private-testnet flows work end-to-end in rehearsal conditions: multi-node propagation, external mining against a designated node, restart catch-up, missing-parent/orphan recovery, and operator-facing runtime visibility.

## PR-sized workstreams

Each workstream is intended to map to one or more focused PRs with concrete acceptance checks.

### WS1 — Rehearsal topology (2–3 nodes)

**Objective:** establish a repeatable private-testnet rehearsal topology.

- Define and document a standard 2-node and 3-node rehearsal layout.
- Identify node roles (node A mining ingress; node B/C propagation and sync peers).
- Document startup ordering and baseline readiness checks.

**Acceptance criteria**

- A reproducible 2–3 node rehearsal procedure exists.
- Node role expectations are explicit and testable.

### WS2 — P2P-by-default rehearsal profiles

**Objective:** ensure rehearsal environments start with P2P enabled by default.

- Update rehearsal profile guidance so P2P is active without manual toggles.
- Document profile deltas versus local-dev/hardening profiles.
- Include safeguards for deterministic rehearsal behavior.

**Acceptance criteria**

- Rehearsal profile docs clearly state P2P is default-on.
- Operators can boot rehearsal nodes with documented profile defaults.

### WS3 — External miner rehearsal against node A

**Objective:** validate external miner workflow in rehearsal topology.

- Run external standalone miner against node A.
- Validate template acquisition and block submission flow under rehearsal settings.
- Document expected stale/retry handling behavior at a high level.

**Acceptance criteria**

- External miner can mine against node A in rehearsal mode.
- Expected request/submit behavior is documented for operators.

### WS4 — Block propagation node A → node B/C

**Objective:** confirm mined blocks propagate across rehearsal peers.

- Validate block announcement and transfer from node A to node B/C.
- Check deterministic acceptance/rejection visibility across nodes.
- Document baseline propagation expectations and troubleshooting signals.

**Acceptance criteria**

- Blocks mined via node A are observed and accepted on node B/C.
- Propagation path has clear runtime evidence for verification.

### WS5 — Restart sync and catch-up rehearsal

**Objective:** verify initial sync/catch-up after node restart.

- Restart node B or C during active block flow.
- Validate catch-up behavior to current tip/DAG state.
- Capture expected timing/ordering behavior in docs.

**Acceptance criteria**

- Restarted node rejoins and catches up without manual state repair.
- Catch-up behavior is documented with validation steps.

### WS6 — Missing-parent and orphan recovery validation

**Objective:** validate resilience for out-of-order ancestry delivery.

- Exercise scenarios where child blocks arrive before parents.
- Confirm orphan staging and eventual reconciliation.
- Verify missing-parent request/retry behavior reaches convergence.

**Acceptance criteria**

- Missing-parent handling and orphan recovery are demonstrated in rehearsal drills.
- Recovery outcomes and failure signals are documented.

### WS7 — Runtime/metrics/status visibility

**Objective:** ensure operators can observe rehearsal health in real time.

- Define minimum runtime counters, logs, and status endpoints used in rehearsal checks.
- Provide a compact checklist for “healthy propagation/sync” signals.
- Clarify evidence capture expectations for release closure.

**Acceptance criteria**

- Rehearsal visibility checklist is documented and actionable.
- Required evidence points exist for propagation, sync, and recovery.

### WS8 — External server rehearsal documentation

**Objective:** document how to run the rehearsal outside local-only setups.

- Add operator guidance for running rehearsal on external servers.
- Include baseline networking, process, and observability expectations.
- Capture known limitations/risks that remain for v2.3.0 closure.

**Acceptance criteria**

- External server rehearsal steps are documented.
- Preconditions and caveats are explicit.

### WS9 — Release notes and closing checklist

**Objective:** close v2.2.9 with explicit rehearsal evidence and v2.3.0 handoff.

- Publish v2.2.9 release notes focused on rehearsal outcomes.
- Add a dedicated closeout checklist for rehearsal sign-off evidence.
- Enumerate deferred items that remain gated for v2.3.0.

**Acceptance criteria**

- v2.2.9 release artifacts clearly mark this as rehearsal, not final readiness.
- Deferred v2.3.0 gates are explicitly listed.

## Explicit handoff to v2.3.0

v2.2.9 validates rehearsal execution, but **does not** replace official readiness closure.

v2.3.0 still owns:

- Official complete private-testnet readiness decision.
- Extended burn-in duration and stability confidence.
- Full operational sign-off across runbooks, incident handling, and recovery depth.
- Final launch-gate validation for sustained multi-node behavior.

## Definition of done for v2.2.9 roadmap closure

- `docs/ROADMAP_V2_2_9.md` is accepted and aligned with release framing.
- Rehearsal scope is clearly defined as pre-v2.3.0 readiness validation.
- Workstreams are split into PR-sized sections with acceptance criteria.
- Closure notes explicitly state: **v2.2.9 is private-testnet rehearsal; v2.3.0 is official readiness milestone**.
