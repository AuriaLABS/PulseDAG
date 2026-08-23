# PulseDAG v2.4.0 release evidence policy

## Current candidate

- Repository version: `v2.4.0`.
- Cargo workspace version: `2.4.0`.
- Candidate vehicle: Task31 PR #993 from `main@91dd8f4314cd0a0672cf3c98f00eea039e59e429`.
- Exact final release SHA: not frozen; the candidate is still moving.
- Final release/activation decision: `PENDING_EXACT_CANDIDATE_EVIDENCE`.
- Public-testnet authorization is separate and remains false.

Evidence from the historical `release/2.4.0` branch, v2.3.0 candidates, Task30 pre-release SHAs, or different Task31 heads must not be combined as if they were one final candidate.

## Required evidence classes

A v2.4.0 release/activation decision must be bound to one exact final candidate SHA and include:

1. deterministic Cargo metadata and lockfile validation;
2. workspace format, check, test and Clippy results;
3. transaction/header v2 and `ghostdag_v1` activation identity validation;
4. clean-chain genesis, P2P capability, storage restart/restore and protocol-sidecar validation;
5. Task30 deterministic/adversarial matrices rerun where affected by Task31 changes;
6. RPC, security, keyless-node, release and repository-hygiene gates;
7. native node/miner package builds and smoke verification on every approved target;
8. per-archive manifests, SHA-256 checksums and provenance;
9. packaged operator startup/recovery validation;
10. explicit tag/publication authorization and independent evidence review.

## Artifact rules

- Node and standalone miner archives are separate release assets.
- The current candidate does not include an official end-user custody wallet unless that wallet is separately ported, packaged and revalidated.
- Every archive requires a matching checksum and provenance manifest.
- Native binaries are smoke-tested only on their native runner.
- Evidence artifacts are retained independently of a GitHub Release.
- Exact source SHA, network identity, genesis/config digests and artifact digests must agree across the complete decision record.

## Guardrails

Current evidence does not authorize:

- creating the `v2.4.0` tag;
- publishing a GitHub Release;
- launching a public testnet or recording Day 0;
- setting `public_testnet_ready=true`;
- starting or backdating the 30-day public-testnet clock;
- enabling high cadence by default;
- enabling smart contracts.

Recorded candidate state:

- `public_testnet_ready=false`
- `thirty_day_public_testnet_clock_started=false`
- `contracts_enabled=false`

## Historical evidence

v2.3.x and v2.2.x evidence remains valid as historical provenance and compatibility input only. It cannot satisfy an exact-SHA v2.4.0 Task31 gate unless the gate explicitly proves that the evidence is invariant to the final candidate changes.
