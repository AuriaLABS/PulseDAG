# Wallet pending UTXO reservation and reconciliation

Status: implementation contract for #1061. This document does not authorize a wallet release, public-testnet/mainnet launch, or any #781/#794 GO decision.

## Safety model

The wallet pending journal is local, versioned, secret-free, and bound to the exact `network_profile` plus `chain_id`. Each pending record is keyed by the final signed transaction txid and stores the sender, exact selected outpoints, conservative state, and relay rejection observation metadata where applicable.

All states except `confirmed` retain the selected-outpoint reservation. Generic `TX_REJECTED`, `submission_started`, `submission_outcome_unknown`, and retained-history absence are not release evidence.

## Transaction flow

1. `tx-preview` builds against the complete bounded UTXO snapshot so spend-all classification is unchanged, then checks the selected outpoints against the pending journal. Reserved inputs are not silently filtered before planning.
2. `tx-sign` acquires the pending journal before signing and holds that lock through deterministic signing. A different active pending tx using any selected outpoint is rejected. If the same deterministic plan already produced the same final txid and an exact `signed` reservation was durably saved before the prior invocation failed to return its envelope, rerunning `tx-sign` may recover that same signed envelope without creating a second reservation generation. Recovery is refused once that record has advanced past `signed`, and any sender/outpoint mismatch fails closed.
3. `tx-broadcast` completes local and remote preflight first. It verifies the signed envelope, relay origin, `/release` network identity, relay capability/version and canonical submit surface, and prepares the exact request body before crossing the submission boundary.
4. Immediately before the submit POST, `tx-broadcast` durably moves the exact journal record from `signed` to `submission_started`. That transition is intentionally non-idempotent, so a restart cannot blindly resubmit an already-started attempt.
5. Explicit relay acceptance records `relay_accepted`. Explicit generic relay rejection records `relay_rejected` but retains the reservation. Any transport, response-read or malformed-response failure after submission begins records `submission_outcome_unknown` and retains the reservation.

## Machine-readable reserved-input conflict

If `tx-preview` or `tx-sign` selects an outpoint already reserved by a different active pending transaction, the command exits non-zero and emits one JSON line to stderr:

```json
{"ok":false,"error":{"code":"PENDING_UTXO_RESERVED","message":"selected outpoint is reserved by a pending transaction","txid":"<canonical-lowercase-32-byte-hex>","index":0}}
```

`txid` and `index` identify the reserved outpoint. Exact `signed` recovery of the same deterministic final txid is not treated as a conflicting reservation. Other CLI failures retain the existing human-readable stderr format.

## Stable states

| State | Reservation retained | Meaning |
| --- | --- | --- |
| `signed` | yes | Signed locally; no submission attempt is durably known to have started. |
| `submission_started` | yes | Submit boundary crossed durably; submission may follow or may already have occurred. |
| `submission_outcome_unknown` | yes | Submission began but the client cannot prove acceptance or rejection. |
| `relay_accepted` | yes | Relay explicitly reported acceptance; confirmation is still pending. |
| `observed_mempool` | yes | Exact txid is positively observed in public mempool activity. |
| `relay_rejected` | yes | Relay explicitly reported a generic rejection; current public evidence is not terminal release proof. |
| `confirmed` | no | Exact txid is positively observed as canonically confirmed; selected outpoints may be released. |

State changes are monotonic/conservative where public evidence permits. A later exact-txid mempool or confirmed observation can strengthen a rejected or unknown record. `confirmed` never downgrades.

## Reconciliation

`pulsedag-wallet-reconcile --pending-journal <dir> --txid <final-txid> --relay <origin> [--page-size <1..100>] [--max-pages <1..10>]` performs public, read-only reconciliation and never signs or submits a transaction.

The command:

- loads the exact pending txid, sender and stored network identity;
- releases the journal lock while performing HTTP reads;
- requires HTTPS except for loopback development and disables redirects;
- verifies `/release` against the stored `network_profile` and `chain_id`;
- requires `explorer_api` and the canonical `/address/:address/activity` endpoint;
- scans a bounded number of retained activity pages and validates pagination, canonical txids, direction/amount coherence, and mempool/confirmed state coherence;
- accepts only positive evidence for the exact final txid;
- reacquires and revalidates the journal before persisting any state change.

The public address-activity surface only labels retained DAG transactions `confirmed` when the core `transaction_is_confirmed` predicate says the txid is present in the authoritative selected/ordered state and was actually applied. Side-DAG membership alone and ordered-replay conflict losers are therefore not confirmation evidence and cannot release a wallet reservation.

Positive mempool evidence may promote the record to `observed_mempool`. Positive authoritative confirmed evidence promotes it to `confirmed` and releases the reservation. `not_observed`, retained-history exhaustion, and page-budget exhaustion are reported but do not mutate state or release outpoints.

The reconcile JSON result includes `network_profile`, `chain_id`, `txid`, `from`, `prior_state`, `state`, `evidence`, `pages_scanned`, `items_scanned`, `retained_history_exhausted`, `budget_exhausted`, `journal_updated`, and `reservation_retained`.

## Durable persistence guarantees

The journal store uses an advisory cross-process lock, immutable generational snapshots, bounded payloads, SHA-256-bound commit markers, stale-generation detection, and fail-closed network validation. An orphan snapshot without a commit marker is ignored; a tampered committed snapshot fails digest validation.

Regression coverage includes restart persistence, concurrent-open rejection, tamper detection, stale generation, cross-network rejection, reservation conflicts, exact `signed` recovery after a failed result handoff, submission-started/unknown/accepted/rejected/mempool/confirmed transitions, retained-history absence, side-DAG/replay-loser non-confirmation, durable reconciliation across restart, and confirmed release across restart.

No reconciliation path automatically rebroadcasts a transaction. No private key, mnemonic, password, decrypted seed, signing session, acknowledgement override, or custody RPC is introduced by this flow.
