# v2.3.0 Task 12 — Multi-host private-testnet rehearsal

## Status

**COMPLETE** — protected live `GO` accepted on 2026-07-20 UTC after independent artifact and endpoint review.

## Goal

Run one reproducible five-node private-testnet rehearsal on separate hosts or genuinely isolated network namespaces, exercise external mining, restart, partition, restore, and rejoin, and produce a checksummed private-testnet GO/NO-GO evidence bundle.

## Deliverables

- A strict five-node inventory contract with one seed and four ordinary nodes.
- A standard-library rehearsal controller that uses structured remote argv execution with safe SSH quoting and no local shell.
- Loopback-only RPC collection from `/health`, `/status`, `/sync/status`, `/p2p/status`, and `/checks` on every host.
- Fail-closed validation of chain identity, `libp2p-real`, real-peer semantics, response freshness, sync consistency, replay gap, peer count, and height spread.
- Proof that an external miner advances the network before fault injection.
- One ordinary-node lifecycle restart followed by convergence.
- One bounded P2P isolation and restore hook followed by rejoin and convergence.
- Immutable JSON evidence, an explicit `GO` or `NO-GO`, and SHA-256 checksums.
- Deterministic fake-transport regression coverage and a protected Actions gate for the real rehearsal.
- English comments, diagnostics, evidence fields, and operator documentation.

## Acceptance criteria

1. The inventory defines exactly five uniquely named nodes, exactly one seed, and four ordinary nodes.
2. Every node uses the same `private-testnet-v2.3.0` network profile and `pulsedag-private-v2.3.0` chain ID.
3. Every RPC URL is loopback-only and every remote path is absolute.
4. The inventory pins one exact 40-character candidate commit SHA.
5. Preflight and Task 09 lifecycle verification pass on all five hosts.
6. All nodes report fresh, non-degraded `libp2p-real` status and real-network peer semantics.
7. Sync consistency passes, storage replay gap is zero, and the healthy-node height spread stays within the configured bound.
8. External mining advances the network by the configured minimum before fault injection.
9. The selected ordinary node restarts and rejoins without a consistency or replay failure.
10. The selected ordinary node reaches zero P2P peers during bounded isolation while the remaining topology stays converged.
11. Restoring the exact isolation rule returns all five nodes to the convergence contract.
12. `GO` is impossible unless every mandatory phase passed and the evidence checksum set verifies.
13. Any failure produces `NO-GO`, exits non-zero, and preserves `public_testnet_ready=false` and the unstarted 30-day public-testnet clock.
14. Repository hygiene, syntax, unit regression, and evidence-verifier checks pass.

## Accepted evidence

- Candidate: `22fa09b19da2893fa73b91b198b26675bd1e6e32`.
- Workflow run: `29773225491`.
- Artifact: `v2_3_0_task12_netns_22fa09b19da2893fa73b91b198b26675bd1e6e32_29773225491_1`.
- Artifact SHA-256: `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`.
- Controller manifest: 56 unique entries, zero duplicate paths, 56/56 checksums verified.
- Decision: `GO`, `failure=null`, five nodes, exact candidate SHA.
- Mandatory phases: preflight, start, baseline convergence, pre-fault external mining, restart/rejoin, bounded partition, partition restore/rejoin, final convergence, and post-fault external mining all `PASS`.
- Independent endpoint audit: 55/55 snapshots passed health, checks, real-P2P, sync-consistency, replay-gap, storage/memory, freshness, and degradation assertions.
- Isolation proof: the target retained process and loopback RPC access while direct P2P sessions fell from one to zero; the exact link was restored and the node rejoined.
- Final guardrails: `version_bump_authorized=false`, `public_testnet_ready=false`, and `thirty_day_public_testnet_clock_started=false`.

## Guardrails

- The live rehearsal runs only from a protected environment with genuinely isolated network namespaces or equivalent private network reachability.
- The controller never reads or archives environment-file contents, identity keys, wallet material, authorization headers, or operator tokens.
- Fault hooks are explicit absolute argv arrays. Shell command strings are not accepted.
- Isolation affects only the target node's P2P path. Loopback RPC and out-of-band recovery access remain available.
- Mining remains external to the node. No embedded pool logic is introduced.
- No storage-format, miner protocol, smart-contract, version, or release-tag change is authorized by this task.
- A private-testnet `GO` does not authorize a public testnet and does not start or backdate the 30-day public-testnet clock.

## Follow-up

Task 13 may prepare a separate v2.3.0 version and release-decision proposal. No version bump, release tag, artifact publication, public-testnet readiness claim, or public-testnet clock action is authorized until that proposal receives explicit maintainer approval and its exact candidate reruns every required gate.
