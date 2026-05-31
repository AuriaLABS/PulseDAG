# Codex task: v2.2.19 header/inventory based block sync

## Context

5N/4M evidence shows many accepted blocks but divergent tips and large orphan sets. Full-block propagation alone is not enough under multi-miner pressure if peers receive children before parents and do not recover the missing ancestors quickly.

Kaspa-style systems commonly separate block announcement, header/metadata exchange, and full block retrieval. Use that as design inspiration only. Do not copy code verbatim.

## Goal

Introduce a lightweight inventory/header synchronization layer so peers can learn block hashes/parents cheaply, identify missing ancestors, and request full blocks in dependency order.

## Required design

Add P2P message types or equivalent protocol handlers for:

1. `InvBlocks` / block hash inventory
   - announces one or more block hashes;
   - small, bounded batch size;
   - deduplicate already-known hashes.

2. `GetBlockHeaders` / `BlockHeaders`
   - request headers for unknown hashes or locator-style traversal;
   - response includes block hash, parent hashes, height/work metadata if available, timestamp, and any existing DAG ordering metadata.

3. `GetBlocks` / `Blocks`
   - request full blocks only after header dependency order is known;
   - batch and rate-limit.

4. Dependency-aware fetch scheduler
   - headers first;
   - parents before children;
   - retry missing ancestors;
   - avoid duplicate requests across peers;
   - backoff misbehaving or non-responsive peers.

5. Integration with orphan recovery
   - orphan insert should trigger header/parent fetch;
   - received headers should populate missing-parent dependency information;
   - received full blocks should drain orphan dependants.

## Metrics / endpoints

Expose enough evidence to debug convergence:

- `inv_blocks_received`;
- `inv_blocks_sent`;
- `headers_requested`;
- `headers_received`;
- `blocks_requested`;
- `blocks_received_by_request`;
- `duplicate_inv_ignored`;
- `fetch_scheduler_pending`;
- `fetch_scheduler_inflight`;
- `fetch_scheduler_completed`;
- `fetch_scheduler_failed`;
- `last_fetch_peer`;
- `last_fetch_error`.

## Tests

1. Unit tests for inventory dedup.
2. Unit tests for parent-before-child scheduling.
3. Unit tests for duplicate inflight request suppression.
4. Integration test where node receives child before parent and recovers via headers/full block fetch.
5. Local evidence:
   - 5N/1M baseline PASS;
   - 5N/4M stress records lower orphan backlog and improved final convergence versus current evidence.

## Acceptance criteria

- Existing block propagation continues to work.
- Peers do not flood each other with full blocks unnecessarily.
- Missing parent recovery becomes observable and bounded.
- 3N/1M remains PASS.

## Guardrails

- Do not change consensus validity rules in this PR.
- No smart-contract runtime.
- Miner remains external.
- Do not claim public testnet readiness.
- Do not copy Kaspa code verbatim; concepts only.