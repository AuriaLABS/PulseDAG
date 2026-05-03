# Rehearsal acceptance tests v2.2.9

This document tracks automated rehearsal coverage added/verified before the v2.3.0 cut.

## Core acceptance path (`pulsedag-core`)

Covered by tests in:

- `crates/pulsedag-core/src/accept.rs`
- `crates/pulsedag-core/src/orphans.rs`

Coverage includes:

- valid PoW accepted
- invalid PoW rejected
- duplicate block reported as duplicate
- unknown/missing parent reported as unknown parent
- mutation / invalid structure rejected
- central acceptance result serialized as machine-readable status
- orphan child queued before parent
- parent arrival retries queued child
- invalid orphan never accepted
- orphan capacity pruning/limit behavior

## Mining RPC (`pulsedag-rpc`)

Covered by tests in:

- `crates/pulsedag-rpc/src/handlers/mining_template.rs`
- `crates/pulsedag-rpc/src/handlers/mining_submit.rs`

Coverage includes:

- mining template lifecycle/state checks
- deterministic transaction ordering for template construction
- mining submit accepted path returns accepted payload with block hash/id and height
- mining submit goes through central acceptance outcomes (accepted / invalid block / duplicate or stale paths)
- invalid PoW submit rejected with structured diagnostics
- stale/duplicate-like resubmissions handled cleanly

## P2P rehearsal checks (`pulsedag-p2p`)

Covered by tests in:

- `crates/pulsedag-p2p/src/lib.rs`

Coverage includes:

- block announcement message roundtrip and dedup suppression
- inventory announcement observation for sync/get-block follow-up
- block data message roundtrip into inbound block handling path
- duplicate announcement ignored

## Remaining manual checks for v2.3.0

Manual rehearsal still recommended for release confidence:

- full-node multi-process/network interoperability under sustained load
- miner integration behavior with real-world stale-template churn
- long-duration orphan pressure behavior in mixed honest/adversarial traffic
- end-to-end telemetry/alerting verification in production-like deployments
