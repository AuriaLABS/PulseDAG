# Access set v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1163, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P2
Related: `PULSECLOCK_V1.md`, `COVENANT_VAULT_V1.md`

This document defines declared access sets for a future programmable transaction format. It does **not** authorize Transaction Protocol v3, reinterpret Transaction Protocol v2, enable covenants/contracts, or change GHOSTDAG selection.

Existing v2 transactions have no access-set field. Nodes MUST NOT infer one. v2 conflict handling remains the frozen v2.4.0 rule (conflicts reject; no implicit RBF) until a later activation contract admits this format.

## Purpose

GHOSTDAG already admits parallel *blocks*. Application state still serializes if every transaction is assumed to touch the whole UTXO set.

An access set is a committed declaration of the state a transaction may read or write. The scheduler may apply non-overlapping transactions concurrently. Overlap is a deterministic reject, not "first arrival wins".

## Domain

```text
PulseDAG:access-set:v1
```

Bound to `chain_id` and the transaction identity of the format that carries the set.

## Declaration

Planning fields on a future versioned transaction (not tx v2):

| Field | Type | Meaning |
|---|---|---|
| `writes` | set of outpoints | UTXOs this transaction spends or replaces |
| `reads` | set of outpoints | UTXOs inspected but not spent (vault delay checks, covenant predicates) |
| `write_keys` | set of key ids | optional template/asset keys created or mutated |
| `read_keys` | set of key ids | optional template/asset keys inspected |

Empty `writes` is invalid for a value-moving transaction. `reads` may be empty. Duplicate entries are invalid. Order in the committed bytes is canonical sorted order so the set digest is stable.

Key ids are opaque 32-byte commitments defined by a later template/asset spec. Access-set v1 only requires that they compare and hash deterministically.

## Honesty rule

Validation MUST fail closed if the transaction:

- spends an outpoint not in `writes`;
- inspects template/output state not listed in `reads` or `writes`;
- mutates a key not in `write_keys`;
- lists an outpoint or key that does not exist at apply time (except creation of a new `write_keys` id, which MUST be in `write_keys` and MUST NOT already exist).

Over-declaring (listing a live outpoint that is not actually touched) is a conflict against any other transaction that lists the same outpoint, and is therefore allowed but fee-inefficient. Under-declaring is always invalid.

## Conflict

Two transactions `A` and `B` in the same apply window **conflict** iff any of:

- `A.writes` intersects `B.writes`
- `A.writes` intersects `B.reads`
- `B.writes` intersects `A.reads`
- `A.write_keys` intersects `B.write_keys`
- `A.write_keys` intersects `B.read_keys`
- `B.write_keys` intersects `A.read_keys`

Read-read overlap is not a conflict.

## Scheduler

Given a candidate set `S` already ordered by the canonical GHOSTDAG transaction order for the applying block (and its merge set, as defined by the consensus apply path):

1. Walk `S` in that order.
2. Accept the next transaction if it is internally valid and does not conflict with any already accepted transaction in this window.
3. Otherwise reject it with a stable class (`access_conflict` or `access_underdeclared`).
4. Do not retry rejected transactions in the same window.

The accepted set MUST be identical for every honest node applying the same block/DAG window. Arrival order, peer order, and local thread schedule MUST NOT change the accepted set.

Rejected transactions are not applied and do not mutate UTXO or key state. They may remain in mempool policy only if a later mempool spec says so; this document does not change mempool v3.

## Relation to vault_v1

A future `vault_v1` spend lists:

- the vault outpoint in `writes`;
- no extra UTXO reads unless the template inspects another output;
- no key fields until asset/template key ids exist.

PulseClock metadata is consensus/context input, not an access-set key.

## Non-goals

- Parallel PulseVM heap access.
- Optimistic execution with rollback as a consensus requirement (optional implementation, same accepted set).
- Making access sets a hidden field on tx v2.
- Fee-market changes.

## Evidence required before any later activation

- Golden vectors: disjoint writes apply both; write-write rejects the later in canonical order; write-read rejects the later; read-read applies both.
- Under-declare spend rejects.
- Permuted arrival of the same block body yields identical accepted set and state digest.
- Replay without host time.

## Authorization

Planning only. Not part of the v2.4.0 Task31 protocol identity (transaction v2 + header v2 + `ghostdag_v1`).
