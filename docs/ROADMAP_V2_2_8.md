# Roadmap v2.2.8 — Ambitious Pre-Private-Testnet Hardening

## Positioning

v2.2.8 is the **major hardening release** that prepares PulseDAG for private testnet operations, but it is **not** the official private testnet launch.

- **v2.2.8 purpose:** aggressively de-risk PoW, validation, P2P propagation, sync recovery, operator workflows, and observability.
- **v2.3.0 purpose:** official private testnet launch with longer burn-in and launch-gate signoff.

## Explicit Boundaries (Non-Goals)

The following are out-of-scope for v2.2.8 and must not be introduced in this phase:

- No smart contracts.
- No smart-contract runtime enablement.
- No pool logic inside the miner.
- Miner remains an external, standalone application.
- No claim that v2.2.8 is the full private testnet release.

---

## Workstream A — PoW Hardening & Canonical Preimage

### PR A1: Freeze canonical PoW preimage format
- Define and document the exact serialization order and field encoding for the PoW preimage.
- Add deterministic serialization tests using fixed vectors.
- Add compatibility notes for external miners.

### PR A2: Header/preimage consistency checks in node validation
- Enforce that hashed bytes always derive from canonical preimage logic.
- Reject malformed/mismatched preimage-producing headers.
- Add negative tests for malformed byte layouts.

### PR A3: Hashing-path robustness and edge-case testing
- Add tests for boundary values (zero, max fields, unusual timestamps, and large nonce ranges).
- Add fuzz/property tests around preimage parsing/encoding.
- Ensure no panic/undefined behavior in PoW verification path.

---

## Workstream B — Target/Difficulty Foundation

### PR B1: Introduce explicit target representation and conversion utilities
- Add canonical target type + conversion helpers (compact/full target where applicable).
- Define strict validation for overflow/underflow and invalid encodings.
- Add deterministic test vectors.

### PR B2: Difficulty/target validation in block acceptance
- Validate that block PoW meets declared/effective target before acceptance.
- Ensure checks are centralized and reusable by all acceptance pathways.
- Add reject-reason coverage tests.

### PR B3: Prepare retarget evolution hooks (without enabling full retarget policy)
- Add clean interfaces for future adjustment algorithms.
- Keep current policy stable while making v2.3.0 upgrade path straightforward.

---

## Workstream C — Unified Block Acceptance Validation

### PR C1: Consolidate acceptance gates into single validation pipeline
- Ensure all block ingress paths call the same core validation sequence.
- Remove duplicated or divergent rule checks.
- Define ordered validation stages with clear error codes.

### PR C2: Rule-level reject reason taxonomy
- Introduce stable reject reason IDs/messages for operator diagnostics.
- Map all common failure modes (PoW invalid, missing parent, duplicate, malformed, etc.).
- Add unit/integration tests asserting deterministic reason mapping.

### PR C3: Validation metrics hooks
- Add counters/timers for each validation stage and rejection category.
- Expose these for local test lab observability.

---

## Workstream D — Mining RPC Hardening & External Miner Readiness

### PR D1: Harden mining RPC request/response schema
- Validate mandatory fields, ranges, and formats.
- Enforce consistent response envelopes and explicit error codes.
- Add backward-compatible documentation updates where needed.

### PR D2: External-miner interoperability contract tests
- Add black-box integration tests for external miner flows:
  - get work template,
  - construct canonical preimage,
  - submit solved block,
  - validate success/failure semantics.
- Include stale-work and invalid-share behavior expectations.

### PR D3: RPC abuse resistance and reliability controls
- Add basic safeguards (rate limits/timeouts/size limits as appropriate).
- Ensure malformed request storms do not destabilize node responsiveness.

---

## Workstream E — P2P Block Inventory Propagation

### PR E1: Inventory announcement primitives
- Introduce/standardize INV-style block announcements.
- Maintain dedupe caches to avoid repeated relay storms.

### PR E2: Block request/response flow hardening
- Ensure peers can request unknown advertised blocks.
- Add timeouts/retries and bounded queues.
- Add abuse protections for repeated invalid inventories.

### PR E3: Relay behavior instrumentation
- Track announce-to-receive latency and relay fanout effectiveness.
- Add test scenarios with delayed, duplicate, and out-of-order inventory.

---

## Workstream F — DAG Sync Missing-Parent Recovery & Orphan Handling

### PR F1: Missing-parent detection and targeted fetch scheduling
- Detect missing ancestors during block intake.
- Queue targeted parent fetches with dedupe and backoff.

### PR F2: Orphan pool lifecycle
- Add bounded orphan storage with TTL/eviction policy.
- Re-attempt orphan attachment when parents arrive.
- Ensure deterministic behavior under orphan floods.

### PR F3: Deep-gap and long-chain recovery testing
- Add integration tests for long missing-parent chains.
- Verify eventual convergence and bounded memory behavior.

---

## Workstream G — Private Testnet Config Profiles

### PR G1: Configuration profile set for pre-private-testnet rehearsal
- Add clearly named configs (e.g., single-node-dev, local-multinode, private-testnet-candidate).
- Define safe defaults for PoW, P2P, RPC, and sync behavior.

### PR G2: Operator-facing config documentation
- Document profile intent, expected topology, and tuning knobs.
- Include migration notes from existing default config.

### PR G3: Config validation and startup diagnostics
- Validate critical config invariants at startup.
- Emit actionable diagnostics for invalid combinations.

---

## Workstream H — Multi-Node Local Test Lab

### PR H1: Reproducible multi-node orchestration
- Provide scripts/compose/dev tooling to launch N-node local DAG network + external miner.
- Include deterministic bootstrap steps and teardown workflow.

### PR H2: Scenario suite for adversarial and recovery conditions
- Add test scenarios:
  - temporary partition,
  - delayed block relay,
  - orphan bursts,
  - invalid block injection,
  - miner restarts.

### PR H3: Convergence and stability checks
- Add pass/fail checks for chain/DAG convergence, peer health, and block propagation latency.

---

## Workstream I — Observability & Smoke Testing

### PR I1: Node observability baseline
- Add/standardize logs, metrics, and optional tracing for PoW, validation, P2P, sync, RPC.
- Ensure structured fields support fast triage.

### PR I2: End-to-end smoke suite
- Add pre-release smoke checks covering:
  - node startup,
  - peer connectivity,
  - external miner submission path,
  - propagation,
  - missing-parent recovery,
  - orphan reattachment.

### PR I3: Release-candidate hardening checklist
- Create v2.2.8 RC checklist with explicit stop/go criteria.
- Track known-risk items and mitigation ownership.

---

## Workstream J — Release Notes & Handover

### PR J1: v2.2.8 release notes draft
- Summarize hardening changes by subsystem.
- Highlight operator-impacting changes and any migration steps.
- Capture known limitations intentionally deferred to v2.3.0.

### PR J2: v2.3.0 dependency handoff section
- Explicitly document which private-testnet launch gates depend on v2.2.8 outcomes.
- List unresolved items that remain blockers for v2.3.0 GA private testnet launch.

---

## v2.3.0 Dependency Map (Must be Explicitly Satisfied)

v2.3.0 private testnet launch is contingent on v2.2.8 delivering evidence for the following dependencies:

1. **PoW correctness confidence**
   - Canonical preimage frozen and validated.
   - External miner interoperability proven via integration tests.

2. **Acceptance-path determinism**
   - Unified validation path across all ingress routes.
   - Stable reject reason taxonomy and observability coverage.

3. **Network propagation and sync resilience**
   - INV/request/relay flow operational under load.
   - Missing-parent recovery and orphan lifecycle proven in multi-node scenarios.

4. **Operational readiness**
   - Pre-private-testnet config profiles finalized.
   - Multi-node local lab and smoke suite runnable by operators/developers.

5. **Launch governance inputs**
   - RC checklist outcomes and risk ledger feed directly into v2.3.0 go/no-go.

If these dependencies are only partially met, v2.3.0 should proceed only as a continued hardening cycle, not as official private testnet launch.

---

## Suggested Execution Order

1. A (PoW/preimage) + B (target/difficulty foundation)
2. C (unified acceptance validation)
3. D (mining RPC + external miner readiness)
4. E/F (P2P inventory + sync/orphan resilience)
5. G/H (profiles + multi-node lab)
6. I (observability + smoke)
7. J (release notes + v2.3.0 handoff)

This ordering maximizes early correctness guarantees before scaling to networked failure-mode testing.
