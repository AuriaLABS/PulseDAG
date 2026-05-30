# Codex task: v2.2.19 orphan parent recovery

## Context

5N/4M private rehearsal evidence after PR #532:

- nodes connect and peers are non-zero;
- miners receive templates and submit work;
- accepted blocks are non-zero;
- final tips diverge;
- readiness reports `sync_status: fail`, `sync degraded`, and many orphaned blocks with missing parents.

This PR should implement robust parent recovery for orphaned blocks. It may take inspiration from Kaspa-style DAG synchronization concepts, but must not copy Kaspa code verbatim.

Kaspa inspiration to study conceptually:

- maintain explicit missing-parent/orphan state;
- request unknown parents from peers;
- process orphan dependants when parents arrive;
- avoid unbounded orphan memory;
- prefer headers/inventory before full block flooding where applicable.

## Goal

When a node receives a block whose parents are unknown, it should actively recover the missing parents and later reprocess the orphaned block/dependants once parents arrive.

## Required changes

1. Add/verify an orphan pool keyed by block hash:
   - store orphan block;
   - store missing parent hashes;
   - store first_seen/last_requested timestamps;
   - store source peer if available;
   - bound by count and/or memory.

2. Add a reverse dependency index:
   - `missing_parent_hash -> orphan_block_hashes`.
   - when a parent arrives, find affected orphans and retry validation/import.

3. Add missing-parent request logic:
   - request missing parent hashes from source peer first;
   - then from other peers if source does not answer;
   - rate-limit duplicate requests;
   - do not spam the same peer.

4. Add retry/import loop:
   - on parent arrival, re-evaluate dependent orphans;
   - recursively drain newly unblocked orphans;
   - prevent infinite retry loops.

5. Add eviction policy:
   - max orphan count;
   - max orphan age;
   - evict stale orphans with diagnostics.

6. Add metrics exposed via `/p2p/status`, `/sync/status`, or equivalent:
   - `orphan_count`;
   - `missing_parent_count`;
   - `missing_parent_requests_sent`;
   - `missing_parent_responses_received`;
   - `orphan_reprocess_attempts`;
   - `orphan_reprocess_successes`;
   - `orphan_evictions`;
   - `last_missing_parent_hash`;
   - `last_orphan_drop_reason`.

7. Tests:
   - unit test: orphan block stored when parent missing;
   - unit test: missing parent arrival reprocesses orphan;
   - unit test: orphan chain drains in correct parent-before-child order;
   - unit test: stale orphans evict;
   - integration/smoke evidence: 5N/1M must pass; 5N/4M should show improved convergence or reduced orphan backlog.

## Acceptance criteria

- 3N/1M remains PASS.
- 5N/1M baseline passes.
- 5N/4M either passes, or fails with clear missing-parent recovery metrics proving where recovery stopped.
- No unbounded memory growth.

## Guardrails

- Do not change supply, reward, PoW, difficulty, or block validity rules.
- Do not move miner logic into the node.
- Do not mark `public_testnet_ready=true`.
- Do not copy Kaspa implementation code verbatim; adapt ideas only.