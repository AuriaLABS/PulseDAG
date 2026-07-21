# PulseDAG v2.3.0 release decision

## Current decision

`APPROVE_RELEASE_CANDIDATE`

This decision authorizes the exact versioned release-candidate pull request and its required validation. It does not authorize a tag, GitHub Release publication, public-testnet launch, or the start/backdating of the 30-day public-testnet clock.

## Decision record

- Maintainer: `kalekoi`.
- Decision date: `2026-07-20 UTC`.
- Validated proposal SHA: `4a3d4e3df587f9bd6f438ddd7359a5148f0cff8e`.
- Proposal merge commit: `fec0b304a2544245826e5f799d9932d157818d43`.
- Approval record: `docs/release/V2_3_0_RELEASE_APPROVAL_RECORD.md`.
- Proposal workflow run: `29775577934`.
- Proposal artifact SHA-256: `3394b467734f9064e86ea342030344938d9d1e74964d3176321ab4c6545a3b6f`.

## Rationale

Task 12 produced an independently reviewed protected private-testnet `GO`. The Task 13 proposal bound the full change inventory, compatibility and rollback statement, release-note draft, supported artifact matrix, and fail-closed exact-candidate gate plan to one validated proposal SHA. The downloaded proposal artifact and portable checksum manifest verified successfully.

No unresolved consensus, storage-format, chain-state, miner-protocol, dependency, or public-testnet-scope change was identified in the proposal. The only changed Rust runtime crate file since `v2.2.20` is `crates/pulsedag-p2p/src/lib.rs`, and that change is covered by complete P2P library and real-swarm regression gates.

## Accepted prerequisite

Task 12 protected private-testnet `GO`:

- candidate `22fa09b19da2893fa73b91b198b26675bd1e6e32`;
- workflow run `29773225491`;
- artifact SHA-256 `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`;
- all nine mandatory phases `PASS`;
- 56/56 controller checksums verified;
- independent 55-snapshot endpoint audit passed;
- `public_testnet_ready=false` preserved;
- public-testnet clock not started.

## Authorization effect

`APPROVE_RELEASE_CANDIDATE` authorizes this separate follow-up candidate to:

1. change `VERSION` from `v2.2.20` to `v2.3.0`;
2. change workspace Cargo package versions from `2.2.20` to `2.3.0`;
3. regenerate `Cargo.lock` without dependency upgrades;
4. finalize release notes and candidate metadata;
5. rerun every required CI, P2P, release, packaging, smoke, evidence, and hygiene gate on the exact versioned candidate.

## Post-cleanup exact-candidate refresh

Repository cleanup PR `#775` was merged after the first successful versioned-candidate packaging run. It changed active documentation, release checks, workflows, Docker surfaces, and operator entrypoints while declaring no protocol or dependency change. Therefore the earlier candidate run remains useful historical evidence but is not the final post-cleanup exact-candidate artifact set.

PR `#776` corrects the active v2.3.0 operations and recovery identity, records closeout progress, and requests a fresh exact-candidate workflow on its final evaluated head. The refreshed workflow must rebuild and natively smoke-test Linux, Windows, and macOS node/miner archives, regenerate checksums and manifests, produce provenance and install-verification evidence, and preserve all release/public-testnet guardrails.

This refresh request does not change the current decision and does not authorize tagging or publication. The exact candidate SHA, workflow run, artifact names, digests, and independent review result remain pending until the refreshed run completes and is recorded outside the tested candidate commit or in a later evidence-only closeout record.

## Remaining risks and required follow-up

- The post-cleanup exact versioned candidate must pass all required gates.
- Release archives, manifests, checksums, native smoke results, and provenance attestations must be generated and independently reviewed for that exact candidate.
- Snapshot, restore, rebuild, and reconciliation evidence must be bound to the exact candidate or explicitly accepted as a non-blocking limitation with owner, expiry, and exit criteria.
- Any dependency drift, smoke failure, replay/storage inconsistency, packaging mismatch, or unresolved SEV-1 incident changes the final decision to `NO_GO` or `REQUEST_CHANGES`.
- A final private-testnet release decision must be recorded before any tag or publication.

## Candidate state

- `VERSION=v2.3.0`.
- Cargo workspace version `2.3.0`.
- `version_bump_authorized=true`.
- Final release decision: `PENDING_FINAL_CANDIDATE_EVIDENCE`.
- No `v2.3.0` tag.
- No v2.3.0 artifact publication.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- Smart contracts remain out of scope.
