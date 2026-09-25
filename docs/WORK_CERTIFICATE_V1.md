# Work certificate v1

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-16 UTC
Parent issues: #1178, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P6
Related: miner protocol v3 in `ROADMAP_V2_5_0.md`, `POW_SPEC_FINAL.md`

Planning only. Does **not** add a pool, vardiff, share accounting, or payout logic to `pulsedagd` or the official miner.

The in-tree matcher (`crates/pulsedag-core/src/work_cert_v1.rs`) checks `hash256 <= target256` and binds `chain_id`. Default `WorkCertAdmissionV1::INACTIVE` refuses issuance so the node does not stamp shares. This file does **not** add pool software.

## Purpose

Prove PulseDAG-domain work without turning the node into a pool. A work certificate is a transferable statement that a miner produced valid kHeavyHash work bound to this chain. Third parties may buy, relay-prioritize, or score that work. Consensus does not require certificates to accept blocks.

## Domain

```text
PulseDAG:work-cert:v1
```

Bound to `chain_id`. Kaspa or other-network work is not reusable. PoW preimage remains `PulseDAG:pow:v2` + `chain_id` + header v2 as in the PoW spec.

## Certificate payload (planning)

| Field | Meaning |
|---|---|
| `chain_id` | |
| `template_id` | job/template id from miner proto v3 |
| `selected_tip` | tip the template committed to |
| `header_commitment` | canonical header bytes or hash used as PoW input |
| `nonce` | |
| `hash256` | full-width result |
| `target256` | job target (may be easier than consensus target) |
| `miner_pk` | claimant |
| `issued_pulse_height` | PulseClock at issue, if available |

A certificate is valid iff `hash256 <= target256` and the preimage matches the committed header/template for this `chain_id`. A block-valid proof (`hash256 <= consensus target`) MAY be published as a certificate but is not required.

Optional UTXO wrapper (later, only if covenants are admitted): a `work_cert_v1` output commits the certificate digest and `miner_pk`. Transfer is an ordinary signed spend. This wrapper is not part of Task31 and is not required to mine.

## Node boundary

`pulsedagd` MAY:

- issue templates and accept block submissions as today;
- optionally stamp a certificate for a valid submit that misses the consensus target.

`pulsedagd` MUST NOT:

- track workers, shares, or balances;
- run vardiff;
- pay out from subsidy.

Desktop remains first-class solo mining UX.

## Non-goals

- Changing kHeavyHash.
- Useful-work or AI-hash.
- Stratum in-tree.

## Authorization

Planning only.
