# Closing Checklist v2.2.13 — Consensus/DAG Safety Audit

Use this checklist to close v2.2.13 only after the consensus/DAG safety audit, documentation updates, and evidence notes are complete. v2.2.13 is an intermediate audit milestone, not the official private-testnet readiness release.

## Version and roadmap alignment

- [ ] `docs/VERSION_MATRIX.md` lists v2.2.13 as the consensus/DAG safety audit between v2.2.12 and v2.3.0.
- [ ] README roadmap references include v2.2.13 audit documentation if the README lists release-roadmap documents.
- [ ] `docs/RELEASE_NOTES_V2_2_13.md` preserves the v2.2.13 scope and guardrails.
- [ ] `docs/DAG_SAFETY_INVARIANTS_V2_2_13.md` documents the current DAG model, safety invariants, and consensus compatibility limits.
- [ ] No documentation claims that v2.2.13 or v2.2.12 is v2.3.0 readiness.
- [ ] v2.3.0 remains the private-testnet readiness decision milestone.

## Consensus/DAG safety audit evidence

- [ ] [DAG Safety Invariants v2.2.13](DAG_SAFETY_INVARIANTS_V2_2_13.md) is reviewed alongside code/test evidence.
- [ ] DAG invariant checks are reviewed, added, or explicitly documented as existing coverage.
- [ ] Deterministic tip selection tests are reviewed, added, or explicitly documented as existing coverage.
- [ ] Parent validation tests are reviewed, added, or explicitly documented as existing coverage.
- [ ] Height validation tests are reviewed, added, or explicitly documented as existing coverage.
- [ ] Timestamp validation tests are reviewed, added, or explicitly documented as existing coverage.
- [ ] Missing-parent rejection, quarantine, or orphan handling behavior is reviewed and documented.
- [ ] Orphan adoption safety tests are reviewed, added, or explicitly documented as existing coverage.
- [ ] Replay/order-independence tests are reviewed or added where practical.
- [ ] Any areas where replay/order-independence is not practical are documented with the constraint and follow-up risk.
- [ ] Links to v2.2.13 DAG safety documentation, release notes, and version matrix are checked for correct filenames.

## Compatibility and claims review

- [ ] Documentation states that PulseDAG is not claiming full Kaspa compatibility.
- [ ] Documentation states that PulseDAG is not claiming full GHOSTDAG compatibility.
- [ ] Documentation states that kHeavyHash/PoW alignment does not imply Kaspa/GHOSTDAG consensus compatibility.
- [ ] Documentation states that v2.2.13 is a safety audit milestone, not v2.3.0 readiness.
- [ ] Kaspa-informed or GHOSTDAG-informed language is limited to implementation context and does not imply network or consensus compatibility.
- [ ] Any consensus/DAG safety bugs discovered during the audit are documented with impact, fix status, and test evidence.

## Guardrails

- [ ] No smart contracts are introduced or claimed for v2.2.13.
- [ ] No pool coordination logic is introduced or claimed for v2.2.13.
- [ ] `pulsedag-miner` remains external and standalone.
- [ ] Consensus rule changes are avoided unless they fix a clear safety bug.
- [ ] Any safety-bug consensus change includes focused tests and documentation of the rationale.

## Required checks

- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.

## v2.3.0 handoff

- [ ] The closeout notes summarize consensus/DAG safety evidence gathered in v2.2.13.
- [ ] The closeout notes identify unresolved risks that must be considered by the v2.3.0 readiness decision.
- [ ] The handoff explicitly says v2.3.0 is still a decision milestone, not an automatically granted readiness state.

## Closeout note

When this checklist is complete, v2.2.13 may be closed as the consensus/DAG safety audit milestone. The official private-testnet readiness decision remains deferred to v2.3.0.
