# Wallet pending UTXO reservation and reconciliation

Status: implementation contract for #1061. This document does not authorize a wallet release, public-testnet/mainnet launch, or any #781/#794 GO decision.

## Safety model

The wallet pending journal is local, versioned, secret-free, and bound to the exact `network_profile` plus `chain_id`. Each pending record is keyed by the final signed transaction txid and stores the sender, exact selected outpoints, conservative state, and relay rejection observation metadata where applicable.

All states except `confirmed` retain the selected-outpoint reservation. Generic `TX_REJECTED`, `submission_started`, `submission_outcome_unknown`, and retained-history absence are not release evidence.

Relay rejection metadata is bounded before persistence so a remote error body cannot consume the journal budget indefinitely: `rejection_code` is capped at 128 bytes and `rejection_message` at 2048 bytes with UTF-8-safe truncation. Persisted journal validation rejects manipulated rejection metadata that exceeds those limits.

The journal is reservation state, not permanent transaction history. `confirmed` is terminal and no longer reserves outpoints; once a genuinely new pending reservation is appended, older `confirmed` entries may be pruned. Active/reserving states are never removed by that maintenance step.

## Transaction flow

1. `tx-preview` builds against the complete bounded UTXO snapshot so spend-all classification is unchanged, then checks the selected outpoints against the pending journal. Reserved inputs are not silently filtered before planning.
2. `tx-sign` acquires the pending journal before signing and holds that lock through deterministic signing and durable reservation. Before cryptographic signing it rejects any overlapping active reservation whose stored state/sender/outpoints are incompatible with exact recovery. A `signed` record with the same sender and exact selected outpoints may be treated only as a recovery candidate; the plan is then deterministically re-signed under the same lock and the resulting final txid must exactly match the stored txid. A mismatch fails closed, later states cannot be reopened, and an exact match re-emits the same signed envelope without creating a second reservation generation.
3. `tx-broadcast` completes local and remote preflight first. It verifies the signed envelope, relay origin, `/release` network identity, relay capability/version and canonical submit surface, and prepares the exact request body before crossing the submission boundary.
4. Immediately before the submit POST, `tx-broadcast` durably moves the exact journal record from `signed` to `submission_started`. That transition is intentionally non-idempotent, so a restart cannot blindly resubmit an already-started attempt.
5. Explicit relay acceptance records `relay_accepted`. Explicit generic relay rejection records `relay_rejected` but retains the reservation. Any transport, response-read or malformed-response failure after submission begins records `submission_outcome_unknown` and retains the reservation.

## Machine-readable reserved-input conflict

If `tx-preview` or `tx-sign` selects an outpoint already reserved by a different or otherwise incompatible active pending transaction, the command exits non-zero and emits one JSON line to stderr:

```json
{"ok":false,"error":{"code":"PENDING_UTXO_RESERVED","message":"selected outpoint is reserved by a pending transaction","txid":"<canonical-lowercase-32-byte-hex>","index":0}}
```

`txid` and `index` identify the reserved outpoint. An exact `signed` recovery candidate with the same sender/outpoints is not treated as a pre-sign conflict, but recovery succeeds only if deterministic re-signing reproduces the exact stored final txid. Other CLI failures retain the existing human-readable stderr format.

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

State changes are monotonic/conservative where public evidence permits. A later exact-txid mempool or confirmed observation can strengthen a rejected or unknown record. `confirmed` never downgrades while its terminal record remains in the pending journal. After a new reservation causes older `confirmed` records to be pruned, those pruned txids are no longer addressable through the pending-journal reconcile command; long-term transaction history is outside this journal's purpose.

## Reconciliation

`pulsedag-wallet-reconcile --pending-journal <dir> --txid <final-txid> --relay <origin> [--page-size <1..100>] [--max-pages <1..10>]` performs public, read-only reconciliation and never signs or submits a transaction.

The command:

- loads the exact pending txid, sender and stored network identity;
- releases the journal lock while performing HTTP reads;
- requires HTTPS except for loopback development and disables redirects;
- verifies `/release` against the stored `network_profile` and `chain_id`;
- requires `explorer_api`, the exact-occurrence semantic `authoritative_address_activity_v2` capability, and the canonical `/address/:address/activity` endpoint;
- fails closed before using activity evidence if a relay exposes only generic `explorer_api` or the older txid-level `authoritative_address_activity_v1` capability without `authoritative_address_activity_v2`;
- scans a bounded number of retained activity pages and validates pagination, canonical txids, direction/amount coherence, and mempool/confirmed state coherence;
- accepts only positive evidence for the exact final txid;
- reacquires and revalidates the journal before persisting any state change.

The public address-activity surface only labels a retained DAG transaction occurrence `confirmed` when that exact `(block_hash, txid)` occurrence is in the authoritative selected/ordered state and was actually applied. For read paths that classify many retained transactions, core builds the authoritative applied-occurrence set once per request via `authoritative_confirmed_transaction_occurrences`, rather than rescanning the selected/ordered chain for every matching retained transaction. Binding confirmation to the block hash as well as the txid prevents a replay-skipped duplicate txid in a parallel retained block from inheriting confirmation from the applied occurrence. The bulk occurrence set preserves the authoritative semantics of `transaction_is_confirmed`, including exclusion of side-DAG-only transactions and ordered-replay conflict losers, while identifying the exact canonical occurrence reported by address activity. `authoritative_address_activity_v1` predates exact-occurrence binding and is therefore insufficient release evidence for this wallet flow. The new `authoritative_address_activity_v2` capability explicitly binds reconcile to the `(block_hash, txid)` semantic during mixed-version rollout; a v1-only relay fails closed. A v2 relay may also advertise v1 for backward-compatible read clients, but reconcile requires v2.

Operationally, any relay used for pending reconciliation must be upgraded to a build advertising `authoritative_address_activity_v2` before reconciliation is attempted; a v1-only relay remains read-compatible for older clients but is intentionally incompatible with this release-evidence path.

Positive mempool evidence may promote the record to `observed_mempool`. Positive authoritative confirmed evidence promotes it to `confirmed` and releases the reservation. `not_observed`, retained-history exhaustion, and page-budget exhaustion are reported but do not mutate state or release outpoints.

The reconcile JSON result includes `network_profile`, `chain_id`, `txid`, `from`, `prior_state`, `state`, `evidence`, `pages_scanned`, `items_scanned`, `retained_history_exhausted`, `budget_exhausted`, `journal_updated`, and `reservation_retained`.

## Durable persistence guarantees

The journal store uses an advisory cross-process lock, immutable generational snapshots, bounded payloads, SHA-256-bound commit markers, stale-generation detection, and fail-closed network validation. An orphan snapshot without a commit marker is ignored; a tampered committed snapshot fails digest validation.

Regression coverage includes restart persistence, concurrent-open rejection, tamper detection, stale generation, cross-network rejection, pre-sign incompatible-reservation rejection, exact `signed` recovery after a failed result handoff, submission-started/unknown/accepted/rejected/mempool/confirmed transitions, retained-history absence, side-DAG/replay-loser non-confirmation, bulk authoritative-confirmation classification for retained activity, duplicate-retained-txid occurrence binding to the actually applied block, rejection of generic-only and v1-only legacy activity as authoritative confirmation evidence, bounded UTF-8-safe relay rejection metadata plus fail-closed oversized persisted metadata, durable reconciliation across restart, confirmed release across restart, and pruning of older terminal `confirmed` entries when a genuinely new pending reservation is appended.

No reconciliation path automatically rebroadcasts a transaction. No private key, mnemonic, password, decrypted seed, signing session, acknowledgement override, or custody RPC is introduced by this flow.