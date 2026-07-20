# v2.3.0 Task 12 — Multi-host private-testnet rehearsal

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
- Deterministic fake-transport regression coverage and a manual protected Actions gate for the real rehearsal.
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

## Guardrails

- The live rehearsal is manual and runs only from a protected self-hosted environment with private network reachability.
- The controller never reads or archives environment-file contents, identity keys, wallet material, authorization headers, or operator tokens.
- Fault hooks are explicit absolute argv arrays. Shell command strings are not accepted.
- Isolation must affect only the target node's P2P path. SSH, loopback RPC, and out-of-band recovery access must remain available.
- Mining remains external to the node. No embedded pool logic is introduced.
- No consensus, storage-format, node RPC, miner protocol, smart-contract, version, or release-tag change.
- A private-testnet `GO` does not authorize a public testnet and does not start or backdate the 30-day public-testnet clock.

## Follow-up

Task 13 may propose the v2.3.0 version and release decision only after a real Task 12 evidence bundle records `GO` for the exact candidate SHA and a maintainer separately approves the release proposal.
