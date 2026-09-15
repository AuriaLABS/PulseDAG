# Covenant template channel_v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1173, #794
Depends on: `PULSECLOCK_V1.md`, `ACCESS_SET_V1.md`, `COVENANT_MULTISIG_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P3

Planning only. Not admitted until an activation contract lists `channel_v1`.

## Purpose

Two-party off-chain balance with on-DAG settle. L1 stores the funding output and accepts either a cooperative close or a unilateral close that starts a pulse challenge window.

L1 does not store the off-chain transcript.

## Identity

| Field | Value |
|---|---|
| Template id | `channel_v1` |
| Domain | `PulseDAG:covenant:channel:v1` |

## Funding output

| Field | Type | Meaning |
|---|---|---|
| `party_a_pk` | pubkey | |
| `party_b_pk` | pubkey | distinct from A |
| `challenge_pulses` | u32 | unilateral challenge window |
| `amount` | u64 | total capacity |

`challenge_pulses` in `[64, 65536]`.

## Paths

### Cooperative close

Both parties sign path `coop` over domain + `chain_id` + outpoint + exact successor amounts `(amt_a, amt_b)` with `amt_a + amt_b == amount`. No challenge window. Access set writes the channel outpoint.

### Unilateral close

One party signs path `uni` and posts a state commitment:

| Field | Meaning |
|---|---|
| `state_seq` | u64, strictly increasing per channel |
| `amt_a`, `amt_b` | `amt_a + amt_b == amount` |
| `closing_pk` | the signer, must be A or B |

Creates a `channel_close_v1` output (same template family, stage `closing`) with `posted_pulse_height = current pulse_height`.

### Challenge

Counterparty spends the closing output with path `challenge` before `posted_pulse_height + challenge_pulses` if it presents `state_seq' > state_seq` and a signature from the other party over that higher state. Replaces the closing output with a new closing output (timer restarts) or, if the posted state is terminal and signed by both, settles immediately as coop.

### Timeout settle

After `pulse_height >= posted_pulse_height + challenge_pulses`, either party spends path `timeout` and splits to `amt_a` / `amt_b` as posted. No further challenge.

## Access set

Funding and closing outpoints are writes. PulseClock is context, not a key.

## Forbidden

- Third-party funding keys.
- Settling without coop signatures or an expired challenge.
- Host time.
- Hiding a sequencer: both parties are explicit.

## Authorization

Planning only. This is not a Lightning clone claim and not an activation of payment-channel software.
