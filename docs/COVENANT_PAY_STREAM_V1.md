# Covenant template pay_stream_v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1173, #794
Depends on: `PULSECLOCK_V1.md`, `ACCESS_SET_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P3

Planning only. Not admitted until an activation contract lists `pay_stream_v1`.

## Purpose

Vesting / payroll: a locked amount releases in equal pulse buckets to a recipient. The payer cannot claw back released buckets. Unreleased remainder stays in the stream output until withdrawn or the stream is closed after the last bucket.

## Identity

| Field | Value |
|---|---|
| Template id | `pay_stream_v1` |
| Domain | `PulseDAG:covenant:pay-stream:v1` |

## Output payload

| Field | Type | Meaning |
|---|---|---|
| `payer_pk` | pubkey | may close leftover after end |
| `recipient_pk` | pubkey | withdraws released buckets |
| `start_pulse` | u64 | first pulse that accrues |
| `bucket_pulses` | u32 | width of each bucket |
| `bucket_count` | u32 | number of buckets |
| `amount_per_bucket` | u64 | exact value per bucket |
| `withdrawn_buckets` | u32 | committed on successor stream output |
| `amount` | u64 | remaining locked value |

Constraints:

- `recipient_pk != payer_pk`
- `bucket_pulses` in `[16, 65536]`
- `bucket_count` in `[1, 4096]`
- `amount == amount_per_bucket * (bucket_count - withdrawn_buckets)` at every live output
- `withdrawn_buckets <= bucket_count`
- `start_pulse` set at create (payer-chosen, must be `>= created_pulse_height`)

## Paths

### Withdraw

Recipient withdraws `k >= 1` newly matured buckets.

Matured buckets = `min(bucket_count, floor((pulse_height - start_pulse) / bucket_pulses))` when `pulse_height >= start_pulse`, else 0.

Valid iff:

- template admitted;
- signature under `recipient_pk`, path id `withdraw`;
- `k = matured - withdrawn_buckets` and `k >= 1`;
- one successor stream output with `withdrawn_buckets' = withdrawn_buckets + k` and reduced `amount`, same other fields;
- one plain (or admitted) output of value `k * amount_per_bucket` to the recipient;
- access set writes the stream outpoint.

If `withdrawn_buckets' == bucket_count`, no successor stream output is created.

### Close leftover

After `pulse_height >= start_pulse + bucket_pulses * bucket_count` and `withdrawn_buckets == bucket_count`, no close is needed.

If the recipient never withdraws, funds stay locked until the recipient withdraws. Payer has no clawback of matured buckets. There is no payer-cancel path.

## Forbidden

- Changing recipient, rate, or start on a successor.
- Withdrawing unmatured buckets.
- Host time.

## Authorization

Planning only.
