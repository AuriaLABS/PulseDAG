# PulseDAG v3.0.0 deferred reward settlement

Status: **ARCHITECTURE APPROVED / PRODUCTION FINALITY + STATE-INTEGRATION FREEZE PENDING**

This document defines how v3 converts the canonical ordered-DAG monetary schedule into miner rewards without making a block's immutable transaction commitment depend on an ordered position that can still move before finality.

## The consensus problem

The v3 monetary score is the ordinal of a block in the authoritative deterministic ordered DAG. Before finality, a later DAG update can change a block's ordinal.

A normal coinbase output cannot safely contain `subsidy(monetary_score)` at mining time because the output amount is part of the transaction id and Merkle root. If the canonical position changes, changing the amount would change the already-mined block.

Therefore v3 MUST NOT treat a miner-declared coinbase amount as the authoritative subsidy.

## v3 reward claim

Every non-genesis v3 reward-bearing block carries one reward claim as its first transaction.

The reward claim:

- uses the dedicated v3 reward-claim transaction domain;
- is chain-bound;
- has zero inputs;
- has exactly one beneficiary output;
- requires the beneficiary output amount to be exactly `0`;
- has zero fee;
- commits the beneficiary and nonce but does **not** commit a subsidy amount;
- is not itself a spendable reward UTXO.

The zero amount is a consensus sentinel: it means the actual reward is derived later from canonical state. A non-zero amount in a v3 reward claim is invalid.

Implementation: `crates/pulsedag-core/src/reward_settlement_v3.rs`.

## Canonical settlement amount

For a finalized canonical block at monetary score `M`:

`settlement_amount(M) = subsidy_atoms_for_score(M) + canonical_block_fees(M)`

The subsidy comes only from `crates/pulsedag-core/src/monetary_v3.rs`. Fees are transferred to the eligible miner/reward recipient under the approved v3.0.0 fee rule and do not increase issued supply.

No floating-point arithmetic is permitted.

## Synthetic settlement outpoint

When a reward becomes spendable, consensus materializes a synthetic reward UTXO rather than mutating the amountless claim transaction.

Its identity is domain-separated and binds:

- chain ID;
- canonical block hash;
- reward-claim txid.

The synthetic settlement outpoint is therefore network-specific and cannot be replayed silently across mainnet/testnet.

## Finality binding

Reward settlement MUST NOT trust a naked integer `finalized_score` supplied by a caller.

The finality engine must bind its decision to:

- finality policy version;
- finalized-through monetary score;
- exact block hash at that score;
- deterministic digest of the complete ordered-DAG prefix through that score.

If a DAG reordering changes any position inside that prefix, the old finality binding fails validation and cannot authorize settlement.

`bind_reward_finality_boundary_v3()` only constructs this binding around a score selected by an external finality engine. It does **not** decide which score is final and therefore is not production finality by itself.

## Spendability rule

A reward has three states:

1. `provisional` — ordered position is not protected by the frozen finality policy; no reward UTXO exists;
2. `finalized_immature` — position is final but 3,600 economic seconds have not elapsed; no spendable reward UTXO exists;
3. `spendable` — both finality and the approved 3,600-economic-second maturity are satisfied; the deterministic settlement UTXO may enter authoritative state.

Formally:

`spendable(M) = finality_protected(M) && economic_maturity_reached(M, current_score)`

Finality alone is insufficient. Maturity alone is insufficient.

## Reorg behavior

Before finality:

- reward claim remains immutable;
- its monetary score may move;
- its derived subsidy may be recalculated;
- no spendable reward UTXO exists, so no UTXO needs to be mutated or clawed back.

After a valid finality binding protects the ordered prefix, production consensus must reject any state transition that would rewrite that protected prefix under the frozen finality policy.

This is the key invariant that prevents reward duplication, reward loss and post-spend monetary rewrites.

## State integration still required

The current module derives settlement snapshots and materializable reward UTXOs but deliberately does not activate them in the live v2 ledger path.

Before v3 GO, implementation must still:

- define and freeze the production finality algorithm/version;
- make finality derive the prefix-bound `RewardFinalityBoundaryV3` from consensus state, never RPC/user input;
- integrate v3 reward claims into the activated mining-template and block-validation path;
- reject legacy amount-bearing coinbase subsidy semantics after the v3 activation boundary;
- integrate materialized settlement UTXOs into the authoritative v3 state transition/state root;
- ensure already-materialized settlement outpoints cannot be duplicated on replay/restart/reorg;
- enforce that spends cannot reference provisional or finalized-immature claims;
- persist finality/settlement metadata through snapshots, pruning and cold restart;
- expose pending/finalized/spendable reward status to miner/wallet/RPC tooling without treating pending rewards as balance;
- include settlement state in >=1,000,000-block deterministic replay and adversarial reordering tests.

Until those items and the production finality digest are frozen, `launch_ready` remains false.

## Mandatory vectors

At minimum the exact v3 candidate must prove:

- reward claim amount `0` is accepted by the v3 claim validator and non-zero is rejected;
- otherwise identical reward claims on different chain IDs produce different claim txids;
- settlement outpoints are chain-bound;
- the same accepted DAG produces the same monetary positions and settlement snapshot independent of arrival order;
- a reordered prefix invalidates an old finality binding;
- no finality => no materializable reward UTXO;
- finality without 3,600 economic seconds => no materializable reward UTXO;
- finality + maturity => exactly one deterministic reward UTXO;
- subsidy + fees is exact and overflow-safe;
- cadence transitions preserve the approved economic emission curve;
- replay/restart produces byte-identical settlement state;
- a protected finalized prefix cannot be rewritten without consensus rejection.

## Launch boundary

This architecture resolves the circularity between mutable DAG order and immutable coinbase commitments, but it does not authorize a production finality depth or algorithm.

Overall launch state remains `PRE_FREEZE` and `launch_ready=false` until the finality/settlement implementation, network parameters, genesis identities and exact release evidence are frozen and #781 authorizes GO.
