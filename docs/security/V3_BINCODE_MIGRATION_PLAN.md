# PulseDAG runtime bincode 1.x migration plan

Status: **planned security/storage migration; Phase 0 inventory started; not yet implemented**

Authority: #1127 / #1139. Launch authority: #781. This plan does not grant public-testnet or mainnet GO.

## Why this exists

The current lock contains `bincode 1.3.3`, covered by `RUSTSEC-2025-0141`
(unmaintained). Unlike the historical `linkme`/`lru` findings, this is a
first-party migration problem: runtime PulseDAG crates depend on bincode and
persist or exchange encoded data that must remain recoverable across an upgrade.

A blind dependency bump is forbidden. The migration must preserve deterministic
state recovery, snapshot/fast-sync compatibility, rollback safety and exact-SHA
evidence.

## Phase 0 call-site inventory (current main)

Direct `bincode` crate declarations:

| Manifest | Kind | Notes |
| --- | --- | --- |
| `crates/pulsedag-storage/Cargo.toml` | runtime `bincode = "1.3"` | production encode/decode authority |
| `apps/pulsedagd/Cargo.toml` | runtime `bincode = "1"` | **no first-party `bincode::` calls** in `apps/pulsedagd`; leftover direct dep |
| `crates/pulsedag-core/Cargo.toml` | **dev-dependency** `bincode = "1.3"` | tests / golden comparisons only |

`pulsedag-p2p` has no direct `bincode` dependency. Fast-sync P2P carriers consume storage-produced bytes; they are not a second encode authority.

### Production encode/decode entrypoints (`pulsedag-storage`)

| File | Call | Record / payload | Class | Bound observed |
| --- | --- | --- | --- | --- |
| `src/lib.rs` | `serialize` / `deserialize` | `ChainState` and snapshot payloads | on-disk persistent + snapshot | snapshot metadata written alongside payload |
| `src/lib.rs` | `serialize` | mempool admission height (`MEMPOOL_ADMISSION_HEIGHT_V1_KEY`) | on-disk persistent | key-scoped; size ceiling still to freeze |
| `src/fast_sync_resume.rs` | `serialize` / `deserialize` | `FastSyncSnapshotTransferPlanV1` | fast-sync/resume | `MAX_FAST_SYNC_RESUME_PLAN_BYTES_V1` before persist |
| `src/fast_sync_network_resume.rs` | `serialize` / `deserialize` | `FastSyncNetworkTransferPlanV1` | fast-sync/resume | `MAX_FAST_SYNC_NETWORK_RESUME_PLAN_BYTES_V1` before persist |
| `src/fast_sync_transfer.rs` | `serialize` / `deserialize` | `FastSyncSnapshotBundleV1` payload | snapshot / fast-sync transfer | payload length recorded as `u64`; bundle/manifest cross-check on decode |

### Test-only / non-authority uses

| File | Class |
| --- | --- |
| `crates/pulsedag-core/src/{acceptance,mined_block,network_*,mining_state,mempool_*}_v2.rs` and similar | in-crate test/helper comparisons (core is dev-dep only) |
| `crates/pulsedag-core/tests/*` | test |
| `crates/pulsedag-storage/tests/mempool_expiry_persistence.rs` | test + legacy-bytes equality |
| `crates/pulsedag-storage/tests/retarget_recovery_agreement.rs` | test restart round-trip |

Still required before Phase 1 implementation:

- machine-readable list of every non-test call with line numbers on the exact candidate SHA;
- golden fixtures from real DBs / snapshots / resume plans;
- freeze of exact byte ceilings and old-format identifier (do not infer format from payload only);
- decision on whether `apps/pulsedagd`'s unused direct `bincode` dep can be removed in a separate hygiene PR without changing storage format.

This inventory does **not** choose a replacement codec and does **not** change any reader/writer.

## Required target-codec contract

The replacement codec/version must be selected in a dedicated implementation PR
and must satisfy all of the following before it becomes write-authoritative:

1. maintained upstream with no known security advisory that would merely replace
   one public-GO blocker with another;
2. deterministic encoding with explicit integer/endianness/length behavior;
3. bounded decoding with caller-defined maximum sizes before allocation;
4. no host time, randomness, floating-point normalization or platform-dependent
   representation in canonical persisted fields;
5. explicit `codec_id` and `schema_version` at every persisted/transfer boundary;
6. stable golden vectors committed for each production record type;
7. unknown codec/schema versions fail closed without mutating state.

The plan intentionally does not pre-authorize bincode 2, postcard, CBOR or a
custom codec. The implementation PR must justify the chosen maintained format
against these requirements.

## Migration phases

### Phase 0 — freeze the old readers

- enumerate every production bincode 1.x record shape and exact decode entrypoint;
- capture representative golden fixtures from valid existing databases,
  snapshots, fast-sync bundles and resume plans;
- record size ceilings and corruption/tamper rejection behavior;
- freeze an exact old-format identifier; do not infer format only from payload
  contents.

### Phase 1 — versioned envelope

Introduce a small deterministic envelope containing at minimum:

- magic/domain identifier;
- `codec_id`;
- `schema_version`;
- bounded payload length;
- payload commitment/checksum where the containing protocol does not already
  authenticate the bytes.

Envelope parsing must fail before state mutation on unknown versions, oversized
lengths, truncation or commitment mismatch.

### Phase 2 — dual-read, new-write

For one compatibility window:

- read legacy bincode 1.x records through the frozen legacy reader;
- read the new versioned format through the new reader;
- write only the new format after an object is successfully loaded/validated;
- never silently reinterpret failed new-format bytes as legacy bytes;
- record which format was read so operator/recovery evidence can prove migration
  progress.

### Phase 3 — snapshot and fast-sync compatibility

Exact-candidate tests must prove all supported combinations required during the
upgrade window:

- old snapshot -> new node restore;
- old resume plan -> new node resume or explicit safe restart;
- new snapshot -> new node restore;
- new fast-sync bundle/resume -> new node;
- corrupted/oversized/unknown-version old and new payloads fail closed;
- restored selected tip, ordered DAG/state root, UTXO/state commitments and
  replay results match the canonical source evidence.

A new node must not emit a format that the documented rollback target cannot
safely ignore or recover from unless the operator has crossed an explicit
one-way migration checkpoint.

### Phase 4 — operator migration and rollback

- take a verified backup/snapshot before any write migration;
- never destructively rewrite the sole copy in place;
- write migrated data to a new generation/column/temporary file and atomically
  promote only after verification;
- define disk-space requirements for coexistence of old/new generations;
- define the exact rollback point and what data, if any, cannot be downgraded;
- make interrupted migration restartable and idempotent;
- expose operator-readable migration status and last verified generation.

### Phase 5 — retire bincode 1.x runtime use

Runtime bincode may be removed only when:

- no production storage/node manifest directly depends on bincode 1.x;
- every production persisted/transfer record has a versioned replacement;
- golden vectors, corruption tests, restore, replay, prune, fast-sync and
  restart/rejoin tests pass on one exact candidate;
- upgrade + rollback rehearsal passes with real pre-migration fixtures;
- `cargo audit`/dependency evidence confirms `bincode 1.3.3` is absent from
  launch compile roots;
- release/runbook documentation identifies the migration boundary and recovery
  procedure.

Test-only bincode use in `pulsedag-core` must either be removed too or be clearly
isolated as a dev-only dependency before final security review.

## Evidence invalidation

Any change to the selected codec, schema, envelope, size bounds, migration
algorithm, snapshot representation, fast-sync representation or rollback rule
invalidates the affected compatibility evidence. Evidence from different SHAs
must not be combined to claim the migration complete.

## Non-claims

This plan does not remove `bincode 1.3.3`, does not resolve `atty 0.2.14`, does
not waive Hickory lock-only evidence, does not add advisory ignores, and does not
set dependency security or launch readiness to PASS. Those decisions remain
under #1127/#1139 and #781.

`RUSTSEC-2026-0285` (`rustls 0.23.39`) was closed by #1145/#1146 (`rustls 0.23.45`)
and remains out of scope for this storage-format plan.
