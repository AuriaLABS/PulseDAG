# Covenant template vault_v1

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-15 UTC
Parent issues: #1159, #794
Depends on: `PULSECLOCK_V1.md`
Source thesis: `ROADMAP_V3_0_0.md` Task P3

This document defines the first Pulse Layer covenant template. It does **not** authorize `covenants_enabled`, `contracts_enabled=true`, Task31 identity change, PulseVM, or v3 launch.

Until an explicit activation contract exists, nodes MUST treat `vault_v1` outputs as unknown/non-standard and fail closed. Presence of this document or of inactive `covenant_v1` modules on `main` is not activation.

The in-tree matcher (`crates/pulsedag-core/src/vault_v1.rs`) cites PulseClock `pulse_height` only and verifies an Ed25519 signature over `PulseDAG:covenant:vault:v1` + `chain_id` + outpoint + path. Default `VaultAdmissionV1::INACTIVE` rejects every spend. This file still does **not** set `covenants_enabled` or admit the template into mempool/consensus.

## Purpose

Cold-storage UTXO that cannot move on the owner path until a PulseClock delay elapses, with a separate emergency path that uses a different key set and a longer delay.

No general script. No host time. No VM.

## Template identity

| Field | Value |
|---|---|
| Template id | `vault_v1` |
| Domain | `PulseDAG:covenant:vault:v1` |
| Activation flag | `covenants_enabled` AND template admitted by the frozen activation set |
| Execution engine | template matcher only |

Unknown template id, unknown version, or disabled flag → reject the spend. Do not interpret as a plain P2PK spend.

## Output payload (planning)

Canonical fields committed in the output:

| Field | Type | Meaning |
|---|---|---|
| `template_id` | fixed | `vault_v1` |
| `owner_pk` | pubkey | owner path key |
| `emergency_pk` | pubkey | emergency path key; MUST be distinct from `owner_pk` |
| `owner_delay_pulses` | u32 | pulses after `created_pulse_height` before owner may spend |
| `emergency_delay_pulses` | u32 | pulses before emergency may spend; MUST be `>` `owner_delay_pulses` |
| `created_pulse_height` | u64 | `pulse_height` of the selected tip that first confirmed this output |
| `amount` | u64 | value |

Planning bounds (not activated):

- `owner_delay_pulses` ∈ `[64, 1_048_576]`
- `emergency_delay_pulses` ∈ `[owner_delay_pulses + 64, 2_097_152]`
- both keys required and unequal

`created_pulse_height` is written at confirmation time from PulseClock of the applying selected tip. A later reorg that un-confirms the output discards it; a re-confirm recomputes the field from the new applying tip. Spends cite the committed value, not wall clock.

## Spend paths

Exactly one path per spend. Mixing fields from both paths is invalid.

### Owner path

Valid iff all hold:

1. activation flag admits `vault_v1`;
2. input is a `vault_v1` output;
3. signature verifies under `owner_pk` over domain `PulseDAG:covenant:vault:v1` + `chain_id` + `txid`/`outpoint` + spend path id `owner`;
4. current selected-tip `pulse_height >= created_pulse_height + owner_delay_pulses`;
5. successor outputs are otherwise valid transaction outputs (plain or another admitted template).

### Emergency path

Valid iff all hold:

1. activation flag admits `vault_v1`;
2. input is a `vault_v1` output;
3. signature verifies under `emergency_pk` over the same domain with spend path id `emergency`;
4. current selected-tip `pulse_height >= created_pulse_height + emergency_delay_pulses`;
5. successor outputs valid as above.

The emergency path does not cancel the owner path after the owner delay; first valid confirmed spend wins under ordinary UTXO rules. Wallets SHOULD treat owner-delay expiry as the normal spend moment and keep the emergency key offline.

## PulseClock citation

Delay checks use `pulse_height` only. `pulse_time` and `uncertainty_secs` are informational for wallets.

If PulseClock metadata for the selected tip is unavailable, the spend fails closed. Nodes MUST NOT substitute host time.

A selected-parent reorg may change current `pulse_height`. A spend valid under the old tip and invalid under the new tip is handled as any other UTXO reorg.

## State and replay

Applying a `vault_v1` create or spend MUST be deterministic given:

- the canonical output bytes;
- the applying selected tip and its PulseClock tuple;
- the activation set.

`pulsedag verify` MUST reconstruct vault creates/spends from stored selected-chain metadata without host time once this template is active. Until activation, replay MUST ignore or reject these outputs according to the frozen inactive-contract/covenant boundary in #794 Workstream B.

## Explicitly forbidden

- Recursion or calls into other templates beyond creating a successor output.
- Reading mempool, wall clock, RNG, files, or network.
- Owner and emergency keys being equal.
- Emergency delay ≤ owner delay.
- Interpreting a vault output as anyone-can-spend when the template is disabled.
- Silent migration from any future `vault_v2`.

## Wallet / explorer notes (non-consensus)

When a later user-surface issue implements this:

- show `pulses remaining` = `max(0, created_pulse_height + owner_delay_pulses - current_pulse_height)`;
- never export `emergency_pk` material on the hot owner device by default;
- explorer marks vault outputs as `vault_v1` with both delays visible.

## Evidence required before any later activation

- Golden vectors: create, owner spend at exact delay boundary, owner spend one pulse early (reject), emergency spend at exact emergency boundary, emergency early (reject).
- Distinct-key and delay-order rejections.
- Disabled-flag rejection even with valid signatures.
- Reorg: unconfirm/reconfirm recomputes `created_pulse_height`.
- Replay without host time.

## Authorization

Planning only. Does not change Transaction Protocol v2, does not admit `vault_v1` into mempool or templates on the v2.4.0 Task31 candidate, and does not satisfy #794 Workstream B activation checks.
