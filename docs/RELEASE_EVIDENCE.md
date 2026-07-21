# PulseDAG v2.3.0 release evidence policy

## Current candidate

- Repository version: `v2.3.0`.
- Cargo workspace version: `2.3.0`.
- Exact validated candidate: `629b35fe2dcf27bebfa4ac9ad51458ce255221d0`.
- Candidate workflow: `29800778099`.
- Consolidated artifact: `v2_3_0_candidate_consolidated_29800778099`.
- Consolidated artifact SHA-256: `770c7fb5415ae6c6ec5c983162cc146f43cd63fd44afe22af2aa99cb0841c8f6`.
- Final private-testnet release decision: `PENDING_FINAL_CANDIDATE_EVIDENCE`.

## Required evidence classes

A release decision must be bound to one exact candidate SHA and include:

1. deterministic Cargo metadata and lockfile validation;
2. workspace format, check, test, and Clippy results;
3. P2P, lifecycle, observability, RPC, release, runbook, and repository-hygiene gates;
4. native Linux, Windows, and macOS node/miner builds;
5. native smoke verification on every target platform;
6. per-archive manifests and SHA-256 files;
7. consolidated `SHA256SUMS.txt`, install verification, and provenance summary;
8. independent review of the downloaded evidence bundle;
9. explicit tag and publication authorization state.

## Artifact rules

- Node and standalone miner archives are separate release assets.
- Every archive has a matching `.sha256` and `.json` manifest.
- Native binaries are smoke-tested only on their native runner.
- Consolidation verifies archive structure, manifests, checksums, target coverage, and provenance without executing foreign-platform binaries.
- Evidence artifacts are retained independently of a GitHub Release.

## Guardrails

Current evidence does not by itself authorize:

- creating the `v2.3.0` tag;
- publishing a GitHub Release;
- launching a public testnet;
- setting `public_testnet_ready=true`;
- starting or backdating the 30-day public-testnet clock;
- smart contracts or pool logic.

## Historical evidence

v2.2.x evidence remains valid as historical provenance and as the immutable baseline used by v2.3.0 gates. It is indexed through [`archive/README.md`](archive/README.md) and must not be presented as current operator guidance.
