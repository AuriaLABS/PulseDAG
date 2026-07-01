# PulseDAG baseline cleanup authority map

This document is the pre-Kaspa/GHOSTDAG cleanup ledger. It lists paths that are removed from the authoritative baseline, paths that remain authoritative, and guardrails for future sync work.

## Removed or deprecated paths

- **Final selected-sync as a recovery mechanism is deprecated.** Final quiescence may request ordinary tips and ordinary blocks only after P2P has usable peers and orphan/missing-parent pressure is clear. It must not promote a selected-sync result during zero-peer recovery, orphan recovery, missing-parent recovery, or RPC degraded state.
- **Terminal missing-parent history as an active readiness blocker is deprecated.** Readiness blockers come from active orphan/missing-parent indexes. Historical terminal counters remain metrics only.
- **Ad hoc peer counts are deprecated.** Endpoint-specific peer math must use the P2P status peer-accounting summary instead of recomputing connected, inbound, outbound, bootnode/root, and zero-peer recovery semantics independently.
- **Free-form sync state strings are deprecated for new code.** New code must use the canonical state names listed below and avoid introducing additional spellings.
- **Heavy-lock status reads are deprecated.** Public status endpoints should prefer the cached node RPC snapshot or bounded `try_read`/timeout behavior and return a degraded snapshot rather than waiting indefinitely on runtime locks.

## Authoritative remaining paths

- **Final quiescence:** `apps/pulsedagd/src/main.rs` owns the single final-quiescence cleanup and ordinary tip/block request path. It may clean stale orphan state and request tips only when cleanup is complete and at least one connected peer is present.
- **Orphan and missing-parent recovery:** `crates/pulsedag-core/src/orphans.rs` owns active missing-parent indexes, terminal/quarantined missing-parent classification, and historical terminal pruning.
- **Readiness:** `crates/pulsedag-rpc/src/handlers/readiness.rs` owns operator-facing readiness categories. Active blockers are missing-parent/orphan/request pressure; historical terminal counters stay in metrics.
- **P2P peer accounting:** `crates/pulsedag-p2p/src/lib.rs` owns `peer_accounting_snapshot`, including connected peer count, inbound peer count, outbound peer count, lifecycle-connected peer count, bootnode/root topology, and zero-peer recovery state.
- **P2P status surface:** `crates/pulsedag-rpc/src/handlers/p2p.rs` owns JSON rendering of the authoritative P2P status and must use `peer_accounting_snapshot` for peer counts.
- **Sync state names:** The authoritative set is `idle`, `requesting_blocks`, `orphan_recovery`, `missing_parent_recovery`, `degraded`, and `synced`.
- **Status snapshots:** `crates/pulsedag-rpc/src/api.rs` owns cached/degraded node snapshots for status-style endpoints.

## Risks and follow-up constraints

- Existing metrics retain deprecated final selected-sync counter names for compatibility; they must be treated as legacy observability, not authority for new recovery behavior.
- This cleanup does not implement Kaspa, GHOSTDAG, selected-parent scoring, high-cadence blocks, PoW changes, or emission changes.
- Future DAG sync work must prove zero-peer startup, orphan recovery, missing-parent recovery, and RPC degraded responses cannot finalize incomplete selected-sync state.
