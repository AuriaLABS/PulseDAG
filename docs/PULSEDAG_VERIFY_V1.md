# pulsedag verify v1

Status: **PLANNING SPEC**

Date: 2026-09-15 UTC
Parent issues: #1174, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P5
Related: `RELEASE_EVIDENCE.md`, `PULSECLOCK_V1.md`

Planning only. Does not authorize a public-testnet claim or a change of Task31 protocol identity.

## Purpose

Turn existing replay/evidence culture into a user-facing command:

```text
pulsedag verify --from-genesis [--snapshot <manifest>] [--to-pulse <n>]
```

The command reconstructs selected-parent, blue/red classification, canonical order, UTXO, and state digest, then emits a receipt bound to binary identity and `chain_id`.

A third party can lie about history. The receipt cannot, if the verifier binary and inputs match.

## Inputs

| Input | Rule |
|---|---|
| genesis / chain identity | must match the frozen identity under verification |
| block archive or snapshot+delta | versioned manifest; fail closed on unknown version |
| binary identity | digest of the verifier executable or attested package |

Host time MUST NOT enter the reconstructed state. PulseClock tuples, when verified, are derived from stored DAG metadata per `PULSECLOCK_V1.md`.

Until Pulse Layer templates activate, verify MUST apply the inactive-covenant/contract boundary: unknown templates fail closed or are absent from the admitted set recorded in the receipt.

## Receipt (planning)

Domain: `PulseDAG:verify-receipt:v1`

| Field | Meaning |
|---|---|
| `receipt_version` | `1` |
| `chain_id` | |
| `binary_digest` | SHA-256 of verifier artifact |
| `from` | genesis hash or snapshot root |
| `to_selected_tip` | |
| `to_pulse_height` | if PulseClock metadata present, else omitted |
| `state_digest` | canonical state root |
| `utxo_digest` | |
| `dag_digest` | selected-parent / blue-red / order commitment |
| `admitted_templates` | frozen set used (may be empty) |
| `result` | `match` or `mismatch` |

Print the receipt as canonical JSON and as a hex digest of its canonical bytes.

Non-zero exit on `mismatch` or incomplete input.

## Snapshot path

Reuse v2.5 Task 35 versioned snapshot manifests when present. Verify after snapshot MUST:

1. check manifest signatures/digests;
2. reject poisoned or incompatible snapshots;
3. replay deltas to `to_selected_tip`;
4. include snapshot root in the receipt `from` field.

## Non-goals

- Shipping a hosted explorer as part of this spec.
- Trusting RPC as a substitute for local verify.
- Embedding pool software.

## Evidence before claiming the product

- Same archive + same binary → identical receipt digest on two machines.
- Mutated block body → `mismatch`.
- Snapshot poison → fail closed.
- No host-time dependency in golden fixtures.

## Authorization

Planning only. A later CLI flag on a candidate still requires an API/CLI contract and does not start the public-testnet clock.
