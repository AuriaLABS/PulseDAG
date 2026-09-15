# Covenant template htlc_v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1171, #794
Depends on: `PULSECLOCK_V1.md`, `ACCESS_SET_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P3

This document defines the hash-time lock template. It does **not** authorize `covenants_enabled`, `contracts_enabled=true`, Task31 identity change, PulseVM, or v3 launch.

Until an explicit activation contract admits `htlc_v1`, nodes MUST treat these outputs as unknown and fail closed.

## Purpose

Atomic-swap / bridge primitive: the receiver spends with a preimage before a pulse deadline; otherwise the sender refunds.

No VM. No wall clock. No cross-chain verifier on L1.

## Template identity

| Field | Value |
|---|---|
| Template id | `htlc_v1` |
| Domain | `PulseDAG:covenant:htlc:v1` |
| Hash function | SHA-256 |
| Activation | `covenants_enabled` AND template admitted |

## Output payload (planning)

| Field | Type | Meaning |
|---|---|---|
| `template_id` | fixed | `htlc_v1` |
| `receiver_pk` | pubkey | claim path |
| `sender_pk` | pubkey | refund path; MUST be distinct from `receiver_pk` |
| `payment_hash` | 32 bytes | SHA-256 commitment; preimage is 32 bytes |
| `refund_delay_pulses` | u32 | pulses after `created_pulse_height` before refund |
| `created_pulse_height` | u64 | PulseClock height at first confirm |
| `amount` | u64 | value |

Planning bounds:

- `refund_delay_pulses` in `[64, 1_048_576]`
- `payment_hash` MUST NOT be SHA-256 of 32 zero bytes (reject reserved preimage)
- keys required and unequal

`created_pulse_height` follows the same confirm/reorg rule as `vault_v1`.

## Spend paths

Exactly one path per spend.

### Claim path

Valid iff all hold:

1. template admitted;
2. input is `htlc_v1`;
3. `SHA-256(preimage) == payment_hash` and `preimage` is 32 bytes;
4. signature verifies under `receiver_pk` over domain + `chain_id` + outpoint + path id `claim` + `preimage`;
5. current `pulse_height < created_pulse_height + refund_delay_pulses`;
6. access set lists this outpoint in `writes`.

Claim after the refund height is invalid even with the preimage. Receiver must claim in time.

### Refund path

Valid iff all hold:

1. template admitted;
2. input is `htlc_v1`;
3. signature verifies under `sender_pk` over domain + `chain_id` + outpoint + path id `refund`;
4. current `pulse_height >= created_pulse_height + refund_delay_pulses`;
5. access set lists this outpoint in `writes`.

Refund does not require or reveal the preimage.

## PulseClock and conflicts

Delays use `pulse_height` only. Missing PulseClock metadata fails closed.

A claim and a refund of the same outpoint conflict as write-write. Canonical GHOSTDAG order plus UTXO rules decide; both cannot apply.

Revealed preimages are application data. L1 does not relay them except as spend witness bytes.

## Forbidden

- Variable-length preimages.
- Host time refunds.
- Interpreting a disabled HTLC as anyone-can-spend.
- On-L1 verification of a foreign chain header.

## Evidence before later activation

- Claim at last legal pulse succeeds; claim at refund height rejects.
- Refund at exact delay succeeds; refund one pulse early rejects.
- Wrong preimage rejects.
- Disabled flag rejects both paths.
- Replay without host time.

## Authorization

Planning only. Not Task31 protocol identity. Does not satisfy #794 Workstream B activation checks.
