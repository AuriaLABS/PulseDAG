# v2.4.0 mining-submit finality contract

This document defines the external miner contract for `POST /mining/submit` in v2.4.0.

## Definitive outcomes

- `accepted=true`: the submitted block was accepted.
- `accepted=false` with an ordinary validation reason: the submission was definitively rejected.
- `submit_timeout_before_acceptance`: the node could not acquire the bounded chain-write lock and acceptance did not begin. This is a definitive non-acceptance outcome.

## Non-final outcome

`submit_finality_unknown` means the submission entered the serialized submit actor but the RPC response deadline elapsed before the actor returned. The actor is not cancelled and may subsequently accept or reject the block.

A miner receiving this code must:

1. keep the submitted block hash;
2. avoid incrementing definitive rejection totals;
3. query `GET /blocks/:hash` with bounded retries and backoff;
4. classify a matching block as `reconciled_accepted`;
5. classify a definitive rejection only when the node explicitly exposes one;
6. otherwise record `still_unknown`, fetch fresh work and never blindly resubmit the same hash.

A `NOT_FOUND` block lookup is not a definitive rejection because the actor may still be processing or its final rejected result may not be exposed by the lookup surface.

## Evidence and telemetry

Node-side evidence uses the existing submit actor timeout counter plus the `external_mining_submit_finality_unknown` runtime event. Miner logs expose separate counters for:

- `submits_finality_unknown`;
- `submits_reconciled_accepted`;
- `submits_reconciled_rejected`;
- `submits_still_unknown`.

The original submit is counted once. Reconciliation changes its final classification but does not increment `submits_total` again.

`parent_state_context_unavailable_total` must increment whenever the mining-submit route exposes `parent_state_context_unavailable`.

## Operational gate

Any unresolved `still_unknown` occurrence during the private burn-in or public launch rehearsal must be preserved in the evidence bundle and investigated. It is not automatically a consensus failure, but unexplained growth blocks a public-testnet GO decision.
