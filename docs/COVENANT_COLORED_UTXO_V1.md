# Covenant template colored_utxo_v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1173, #794
Depends on: `ACCESS_SET_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P3

Planning only. Not admitted until an activation contract lists `colored_utxo_v1`.

## Purpose

One color per UTXO. Native asset units move with explicit conservation. Not PDT-20, not an account token, not an NFT standard.

## Identity

| Field | Value |
|---|---|
| Template id | `colored_utxo_v1` |
| Domain | `PulseDAG:covenant:colored:v1` |

## Color id

`color_id` is 32 bytes:

```text
SHA-256(PulseDAG:color:v1 || chain_id || genesis_outpoint)
```

`genesis_outpoint` is the first output that mints the color. Later outputs copy `color_id`; they do not re-derive it.

## Output payload

| Field | Type | Meaning |
|---|---|---|
| `color_id` | 32 bytes | |
| `units` | u64 | asset units in this output |
| `owner_pk` | pubkey | |
| `amount` | u64 | native PulseDAG value, may be dust-bounded |

A mint transaction creates the first output with `color_id` derived from its own outpoint (computed after txid is known; planning note: use a two-step commit or bind color_id to txid+index once assigned). Implementation MUST make mint replay-safe and unique.

Planning mint rule: a mint input is a plain native UTXO; the first colored successor is the genesis. `units` and metadata commitment are signed by the minter. No later mint of the same `color_id`.

## Spend

Owner signs path `transfer`. For each `color_id` in the transaction:

```text
sum(units of spent colored inputs of that color) == sum(units of created colored outputs of that color)
```

Burn is explicit: path `burn` allows output sum `<` input sum; the difference is destroyed and MUST be declared in the witness as `burned_units`.

Access set:

- writes spent colored outpoints;
- `write_keys` includes `color_id` on mint;
- `read_keys` includes `color_id` on transfer/burn.

Native `amount` conservation follows ordinary UTXO rules independently of `units`.

## Forbidden

- Two colors in one output.
- Implicit mint on transfer.
- Reinterpreting colored outputs as plain when the template is disabled (fail closed).
- NFT/enumeration semantics (that is post-3.0 PDT-721).

## Authorization

Planning only. Does not create a token standard ecosystem or indexer requirement for consensus.
