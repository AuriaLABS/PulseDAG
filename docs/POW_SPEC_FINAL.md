# PulseDAG Final PoW Spec — v2.2.10

Status: **FINAL / CANONICAL FOR v2.2.10**

## 1) Active PoW (single consensus truth)

PulseDAG v2.2.10 has one active PoW identity and path:

- Algorithm: **kHeavyHash**
- Engine lineage: **Kaspa-based PoW engine integration** (adapted for PulseDAG header model)
- Acceptance primitive: **256-bit hash vs 256-bit target comparison**
- Input mapping: **PulseDAG canonical header adapter** (deterministic serialization into PoW preimage)

There is no second active PoW path for consensus validation.

## 2) Canonical header adapter

The node and miner must construct the exact same preimage bytes from the PulseDAG block header fields. The adapter is deterministic and versioned so header mutation, field re-ordering, or encoding drift cannot silently pass validation.

Consensus-relevant requirement:

- Any mismatch in canonical header byte construction between miner and node can produce `invalid_pow` or `mutated header` rejection.

## 3) Target and acceptance rule

v2.2.10 PoW validation uses full-width comparison:

- Compute candidate PoW hash (`hash256`).
- Decode/derive the consensus target (`target256`).
- Accept block iff `hash256 <= target256` using canonical 256-bit ordering.

`u64` shortcut scoring is not the acceptance rule for final v2.2.10 consensus.

## 4) Node/miner integration contract

Mining APIs and client behavior:

1. Miner reads node metadata from `GET /pow`.
2. Miner requests work via `POST /mining/template`.
3. Miner solves template with canonical kHeavyHash path.
4. Miner submits via `POST /mining/submit`.
5. Node validates template lifecycle + PoW + block acceptance.

All PoW-critical behavior is shared/compatible across node and external miner expectations.

## 5) Explicit non-claims / boundaries

PulseDAG v2.2.10 does **not** claim:

- Full Kaspa consensus compatibility.
- Production-ready public network status.
- Smart-contract support.
- Embedded pool coordination/accounting/payout logic in `pulsedag-miner`.

## 6) Error taxonomy (submit path)

Operationally relevant rejection classes include:

- `invalid_pow`
- `stale_template`
- `duplicate`
- target mismatch (`hash > target`)
- mutated header / serialization mismatch
- miner/node version mismatch affecting PoW semantics

## 7) Milestone handoff

- **v2.2.10 closes PoW completion** (single finalized PoW truth).
- **v2.2.11 starts P2P completion** as the next milestone.
