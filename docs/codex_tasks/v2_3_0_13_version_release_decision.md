# v2.3.0 Task 13 — Version and release decision

## Status

**ACTIVE — PROPOSAL PHASE**.

Task 12 produced an independently reviewed private-testnet `GO`. This document defines the separate decision process required before any version bump, release tag, or artifact publication.

## Goal

Prepare and review one explicit v2.3.0 private-testnet release proposal whose scope, candidate, evidence, rollback compatibility, unresolved risks, and required gates are fully recorded. Approval of the proposal may authorize a follow-up exact-candidate release PR; drafting or merging this document does not.

## Required inputs

- Tasks 07–11 merged and their CI contracts green.
- Task 12 accepted evidence:
  - candidate `22fa09b19da2893fa73b91b198b26675bd1e6e32`;
  - workflow run `29773225491`;
  - artifact `v2_3_0_task12_netns_22fa09b19da2893fa73b91b198b26675bd1e6e32_29773225491_1`;
  - artifact SHA-256 `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`;
  - `decision=GO`, all mandatory phases `PASS`, no failed phase;
  - `version_bump_authorized=false`, `public_testnet_ready=false`, and public-testnet clock not started.
- Repository hygiene, full workspace tests, Clippy, RPC/release, real-swarm, P2P library, lifecycle, runbook, and pre-burn-in gates green on the proposal head.
- No unresolved SEV-1 consensus, sync, storage, security, identity, credential, or release blocker.

## Proposal deliverables

1. **Candidate scope**
   - Identify the exact proposal commit.
   - List all code and operational changes since `v2.2.20`.
   - Confirm no unreviewed generated output, secrets, or local runtime state.

2. **Compatibility and rollback statement**
   - Confirm storage format and chain identity compatibility.
   - Confirm Task 09 lifecycle upgrade and rollback paths.
   - Record any operator-visible behavior changes, especially P2P peer semantics and bootstrap requirements.

3. **Release notes draft**
   - Summarize bootstrap, lifecycle, observability, runbooks, rehearsal, and peer-accounting work.
   - State known limitations and excluded scope.
   - Preserve the distinction between private-testnet release readiness and public-testnet readiness.

4. **Exact-candidate gate plan**
   - Define the workflows that must rerun after any `VERSION`/Cargo change.
   - Require binary packaging, node/miner smoke tests, full workspace tests, Clippy, repository hygiene, RPC/release validation, P2P real-swarm and complete P2P library tests, and release evidence checks.
   - Require a final decision file that binds results to the exact release candidate SHA.

5. **Maintainer decision**
   - Record one explicit outcome: `APPROVE_RELEASE_CANDIDATE`, `REQUEST_CHANGES`, or `NO_GO`.
   - A non-approval must keep `VERSION`/Cargo at `2.2.20` and preserve all readiness guardrails.

## Approval boundary

Only an explicit `APPROVE_RELEASE_CANDIDATE` decision may authorize a follow-up PR that:

- changes `VERSION` to `v2.3.0` and Cargo package versions to `2.3.0`;
- prepares final release notes and checksummed binary artifacts;
- reruns every required gate on the exact versioned candidate;
- records a final private-testnet release decision.

The follow-up PR must still fail closed. A version change alone is never sufficient evidence.

## Non-goals and guardrails

- No public-testnet launch authorization.
- No start or backdating of the 30-day public-testnet clock.
- No `public_testnet_ready=true` claim.
- No smart-contract enablement or deployment.
- No embedded mining or pool logic.
- No storage-format migration without a separate reviewed task.
- No release tag or artifact publication before exact-candidate gates pass and a final decision is recorded.

## Completion criteria

Task 13 completes only when:

1. the proposal receives explicit maintainer approval;
2. a separate versioned candidate PR updates `VERSION`/Cargo to v2.3.0;
3. all required gates pass on that exact candidate;
4. release notes and artifact checksums are complete;
5. no unresolved SEV-1 blocker exists;
6. the final private-testnet release decision is recorded;
7. `public_testnet_ready=false` and the public-testnet clock remains not started unless a later, separately authorized public-testnet launch task changes them.
