# v2.3.0 Task 03 — Lag injection and selected-segment recovery

## Goal

Prove that a genuinely lagging or temporarily offline node detects the remote selected-chain gap and recovers through the correlated `SelectedSegmentSession` path rather than accidental gossip convergence.

## Existing foundations

The repository already contains:

- selected-tip inventory message schema and runtime lifecycle counters;
- canonical network-gap fields;
- correlated GetBlock request IDs;
- selected-segment session structures;
- parent-first block requesting;
- normal 5N/4M convergence evidence.

This task must connect those components end to end in a reproducible real-node drill.

## Required lag-injection harness

Add a standalone script and evidence schema for:

1. start five real libp2p nodes with four external miners;
2. establish and prove stable 4/4 peer topology;
3. isolate n5 from block/tip propagation without killing its process or corrupting storage;
4. let n1-n4 advance by a configurable minimum selected-height gap, default `96` and never below `64`;
5. stop miners or hold load at the documented transition point;
6. reconnect n5;
7. observe remote selected-tip inventory and canonical gap detection;
8. require an actual correlated selected-segment session;
9. require complete final convergence and storage/readiness invariants.

## Mandatory transition evidence

The evidence must show this sequence with timestamps and peer/session identifiers:

```text
remote inventory accepted
→ best remote selected height > local height
→ network_selected_height_gap >= configured gap
→ sync_state=locating_common_ancestor
→ locator request sent to selected peer
→ matching locator/header response accepted
→ selected segment session active
→ parent-first block requests sent
→ blocks received and applied
→ chunks completed
→ remote selected tip selected locally
→ session completed
```

## Correlation invariants

- Locator success requires a live matching locator request.
- Headers count as session progress only when peer, request, session, and common ancestor match.
- BlockData counts as a correlated response only when request ID, peer, and hash match an outstanding request.
- Ordinary gossip/headers remain separate metrics.
- `responses <= sends`; duplicates and unknown responses use separate counters.
- Applied blocks cannot exceed correlated received blocks.
- Completed chunks require every expected hash to be resolved.

## Selected-tip inventory requirements

Each node must expose fresh per-peer inventory in `/p2p/status`, including:

- peer ID and connection generation;
- chain ID;
- selected tip and selected height;
- ordered DAG tip and state-root digest;
- observed timestamp and inventory generation;
- age, connection state, and direct-request capability.

The harness must fail evidence consistency if the observed network gap and canonical node gap disagree.

## Final invariants

After recovery, all five nodes must have:

- identical selected height and selected tip;
- identical selected chain/ordered DAG tip;
- identical ordered-DAG state root;
- identical retained accepted-hash-set digest;
- storage/memory retained-set equality;
- zero active orphans and missing-parent blockers;
- zero pending selected-segment requests;
- ready status;
- current RPC liveness healthy.

## Failure diagnostics

Capture:

- remote inventory rejection/prune reasons;
- session state and last progress timestamp;
- selected peer and peer rotation history;
- locator/header/block timeout taxonomy;
- terminal/quarantined parent reopen events;
- exact unresolved hashes and their ownership;
- canonical versus harness-observed gap.

## Tests and validation

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test -p pulsedag-p2p tips --locked
cargo test -p pulsedag-p2p getblock --locked
cargo test -p pulsedag-rpc canonical_sync --locked
cargo test -p pulsedagd selected_segment --locked
cargo test -p pulsedagd lag_injection --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

The new drill must emit:

- manifest JSON;
- per-node endpoint captures;
- transition timeline;
- final convergence table;
- logs;
- `evidence.tar.gz` and sha256.

## Guardrails

- No silent fallback to broadcast GetBlock as the primary segment path.
- No fast/high cadence enablement.
- No consensus/PoW semantic change.
- No version bump.
- Keep `public_testnet_ready=false`.

## PR report

Include the real 64–128 block recovery metrics, session transition table, peer-selection behavior, final invariants, archive path, and checksum.