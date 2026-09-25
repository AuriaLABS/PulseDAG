# Covenant template multisig_m_n_v1

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-15 UTC
Parent issues: #1173, #794
Depends on: `ACCESS_SET_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P3

Planning only. Not admitted until an activation contract lists `multisig_m_n_v1`.

The in-tree matcher (`crates/pulsedag-core/src/multisig_v1.rs`) uses Ed25519 like `vault_v1`/`htlc_v1`. Curve selection (spec Schnorr/secp256k1 vs Ed25519) is not an activation. Default `MultisigAdmissionV1::INACTIVE` rejects every spend. This file does **not** admit the template into mempool/consensus.

## Purpose

Treasury UTXO: m-of-n Schnorr/secp256k1 signatures over a fixed key list. No timelock (compose with `vault_v1` if delay is required).

## Identity

| Field | Value |
|---|---|
| Template id | `multisig_m_n_v1` |
| Domain | `PulseDAG:covenant:multisig:v1` |

## Output payload

| Field | Type | Meaning |
|---|---|---|
| `threshold_m` | u8 | required signatures |
| `keys` | pubkey[n] | n in `[1, 16]`, canonical sorted, unique |
| `amount` | u64 | value |

Constraints:

- `1 <= m <= n <= 16`
- keys strictly increasing in canonical encoding

## Spend path

Single path `spend`:

- template admitted;
- exactly `m` signatures, each under a distinct listed key;
- signed domain + `chain_id` + outpoint + path id `spend` + successor commitment;
- bitmap or index list of signing keys is part of the witness and must be strictly increasing;
- access set writes this outpoint.

Successor outputs may be plain or any admitted template.

## Forbidden

- Duplicate keys.
- Signature reuse of the same key.
- Changing the key set without spending.
- Script combinators beyond this template.

## Authorization

Planning only.
