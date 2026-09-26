# Based Apps v0

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-15 UTC
Parent issues: #1174, #794
Depends on: `PULSECLOCK_V1.md`, `ACCESS_SET_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P4

Planning only. Does not authorize PulseVM, PulseProgs, generic ZK, a hidden sequencer, or Task31 identity change.

The in-tree matcher (`crates/pulsedag-core/src/based_app_v0.rs`) cites PulseClock for the challenge window. Default `BasedAppAdmissionV0::INACTIVE` rejects open/challenge/settle. This file does **not** enable PulseVM or a hidden sequencer.

## Purpose

L1 is ordering + data/commitment availability + settlement. Complex execution stays off-L1. v0 is commit-and-challenge measured in pulses, not a validity-proof VM.

## Domain

```text
PulseDAG:based-app:v0
```

Bound to `chain_id` and a versioned `app_id` (32-byte namespace).

## Objects

### App profile (declared, not implicit)

| Field | Meaning |
|---|---|
| `app_id` | SHA-256 of domain + chain_id + canonical profile bytes |
| `operator_set` | 0 or more unique pubkeys; empty means anyone may post commits; identity hashing sorts the set canonically |
| `challenge_pulses` | u32 in `[64, 65536]` |
| `max_blob_bytes` | bound for a single commitment payload |
| `da_mode` | `inline` (payload on L1) or `commit_only` (hash + locator, locator is not consensus-critical) |

If `operator_set` is non-empty, only those keys may open a round. An undeclared extra operator is invalid. There is no hidden sequencer: if an operator exists, the profile names it.

### Round commit output

A based-app output (planning template family `based_commit_v0`) carries:

| Field | Meaning |
|---|---|---|
| `app_id` | |
| `round` | u64, strictly increasing per app |
| `prev_state_root` | 32 bytes |
| `next_state_root` | 32 bytes |
| `payload_hash` | 32 bytes |
| `payload` | optional, present iff `da_mode = inline` |
| `opened_pulse_height` | PulseClock at first confirm |

`payload` length MUST be `<= max_blob_bytes`. `SHA-256(payload) == payload_hash` when inline. In `commit_only` mode, inline payload bytes are forbidden; only the commitment remains consensus-visible.

### Challenge

During `opened_pulse_height + challenge_pulses`, any party may post a `based_challenge_v0` spending the commit (write of that outpoint) with:

- a conflicting `next_state_root` or fraud statement hash;
- bond output (native value; planning minimum is a consensus dust multiple, exact amount later).

v0 does not require L1 to re-execute the app. A challenge freezes settlement until either:

- the operator (or original committer) posts a `resolve` signed over the same `round` that the challenger accepts by timeout without a second challenge, or
- `challenge_pulses` elapse after the latest challenge with no resolve, and the original commit is rejected (state stays `prev_state_root`).

This is intentionally coarse. Validity proofs are post-v0.

### Settle

If no challenge remains when `pulse_height >= opened_pulse_height + challenge_pulses`, anyone may spend path `settle`. Canonical app state for `app_id` becomes `next_state_root`. Successor may open `round+1`.

## Canonical state/read model

The in-tree read model lives in `crates/pulsedag-core/src/based_app_state_v0.rs`. Checkpoint and canonical-event schemas are explicitly versioned; unknown schema versions fail closed.

It consumes **already accepted canonical events**; it is not an admission path and does not activate Based Apps or contracts. Each event carries a total `canonical_position` supplied by the DAG/apply layer. Folding sorts by that position before applying transitions, so network arrival order and local thread scheduling cannot change the resulting view for the same canonical event set.

The v0 view tracks only:

- latest settled round and state root;
- at most one pending round;
- the PulseClock height of the pending challenge, when present;
- the last applied canonical position.

State continuity rules are fail-closed: after a settled round, the next open must use `round + 1` and its `prev_state_root` must equal the settled root. A challenged pending round cannot settle directly. Rejection is valid only after `challenge_pulses` measured from the canonical challenge pulse; a valid reject leaves the previous state root unchanged.

Challenge/settle timing is checked with PulseClock heights. The observation must be PulseClock v1 in domain `PulseDAG:pulse:v1` and carry the same `chain_id` as the Based App profile; foreign-chain or wrong-version observations fail closed. Host wall-clock time is not part of the state transition.

### Bounded event feed

Consensus nodes are not required to maintain an unbounded application history index.

The helper `based_app_event_page_v0` accepts at most **4096 retained canonical events** and returns at most **256 events per page**, ordered by `canonical_position` and cursorable with `after_canonical_position`. The fold can resume transactionally from a persisted `BasedAppStateViewV0`: replay operates on a copy, so an invalid reordered window cannot mutate the checkpoint. A node can retain the latest bounded state/checkpoint plus a recent event window instead of replaying or indexing all historical application events. External indexers may persist full history outside the consensus node.

The public RPC routes are now wired as `GET /api/v1/based-apps/:app_id/state` and `GET /api/v1/based-apps/:app_id/events?after=<canonical_position>&limit=<1..256>`. They remain **fail-closed/inactive** on the current candidate: route identity, canonical lowercase app-id parsing, cursor semantics, JSON shape, and page limits can be tested without claiming that a bounded runtime state/event store is activated.

## Ordering

Canonical order of commits and challenges is GHOSTDAG apply order plus access-set rules:

- `write_keys` includes `app_id` on commit, challenge, resolve, settle;
- `based_app_write_key_v0` maps the domain/chain-separated app identity directly to the access-set key;
- transitions for the same app therefore conflict deterministically under `access_sets_conflict_v1`, while independent app keys do not conflict;
- two opens of the same `round` conflict;
- PulseClock is context, not a key.

## Fail closed

Unknown `app_id` version, missing PulseClock, oversized blob, or empty `payload` when `da_mode = inline` rejects. Disabled based-apps flag treats these outputs as unknown templates.

## Non-goals

- PulseVM execution of the payload.
- Trusted sequencer unless named in the profile.
- Cross-domain replay (`chain_id` bound).
- Full DA sampling network.

## Evidence before later activation

- Open, timeout settle, challenge-then-reject golden vectors.
- Duplicate round write-write conflict.
- Replay without host time.

## Authorization

Planning only. Does not satisfy #794 Workstream B activation checks.
