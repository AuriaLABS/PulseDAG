# PulseDAG v3.0.0 canonical monetary score

Status: **ALGORITHM APPROVED / FINALITY + NETWORK CADENCE FREEZE PENDING**

This document resolves the v3 monetary-index architecture used by `docs/MONETARY_POLICY_V3_0_0.md`. It defines how PulseDAG converts its deterministic DAG order into economic time without using local block arrival, ordinary height, wall-clock time or raw header `blue_score` as the monetary authority.

The exact production ordering digest/version, finality policy and cadence activation table remain launch-blocking freeze inputs. The algorithm below is the required v3 behavior.

## 1. Canonical monetary score

For v3.0.0:

- genesis has `monetary_score = 0`;
- every non-genesis block in the authoritative deterministic ordered DAG has exactly one canonical ordinal;
- the first non-genesis ordered block has `monetary_score = 1`, the next has `2`, and so on;
- no accepted block may occupy two monetary-score positions;
- no two blocks may occupy the same monetary-score position in one canonical consensus state;
- the score is derived from consensus state and is not a caller/miner-provided field.

Normatively:

`monetary_score(block, state) = canonical_order_index(block, state)`

where the genesis entry is index 0 and `canonical_order_index` comes from the frozen v3 successor of `derive_ordered_dag_v2` / `ghostdag-v1-topological-v1`.

The current GHOSTDAG `blue_score` remains an input to block classification/selection/order. It is **not** the v3 monetary score. Ordinary `height` is also **not** the v3 monetary score.

This distinction is mandatory because parallel DAG blocks can share a height or blue-score neighborhood while the deterministic ordered DAG can still place each block exactly once.

## 2. Reward eligibility and one-position rule

The v3 candidate monetary model assigns one subsidy settlement position to every non-genesis block retained by the authoritative ordered DAG. Selected-chain, merge-set blue and merge-set red classifications may affect deterministic ordering, but they may not create duplicate monetary positions.

A block that disappears from the authoritative state before final settlement does not retain an immutable monetary position merely because it was observed earlier.

Therefore the network must not count:

- local arrival order;
- orphan arrival order;
- a miner-declared score;
- a stale local selected-chain height;
- duplicate/replayed block identities.

The ordered-DAG digest is part of monetary settlement evidence.

## 3. Score to economic time

Economic time is a deterministic function of monetary score and a versioned cadence segment table.

Each segment is:

`(activation_score, target_interval_ns)`

with these rules:

1. the first segment activates at score `0`;
2. `target_interval_ns > 0`;
3. activation scores are strictly increasing;
4. a segment activating at score `S` governs the transition `S -> S+1` and subsequent transitions until the next segment;
5. the previous segment owns all score transitions before `S`;
6. cadence transitions never rewrite economic time accumulated before their activation score.

For a score `M`, economic time is the checked integer sum of the target interval assigned to every score transition from `0` through `M`.

Conceptually:

`economic_time_ns(M) = sum(interval_ns_for_transition(i -> i+1), i=0..M-1)`

No block timestamp, local clock, floating point or calendar date participates in this calculation.

## 4. Cadence equivalence

The same economic second must schedule the same cumulative issuance at every supported cadence.

Reference mappings:

| Target cadence | `target_interval_ns` | Score transitions per economic second |
|---|---:|---:|
| 1 BPS / ~1 s | `1_000_000_000` | 1 |
| 2 BPS / ~500 ms | `500_000_000` | 2 |
| 4 BPS / ~250 ms | `250_000_000` | 4 |

For example, one economic second after a cadence segment begins is represented by score delta 1 at 1 BPS, delta 2 at 2 BPS or delta 4 at 4 BPS. All three must produce identical cumulative scheduled issuance.

The final mainnet and parallel-testnet starting cadence and every future activation score remain explicit network-parameter freeze values. A cadence change and its monetary interval change MUST activate at the same consensus boundary.

## 5. Exact cumulative issuance

Approved constants:

- `MAX_SUPPLY_ATOMS = 100_000_000_000_000_000`;
- `YEAR1_MINING_BUDGET_ATOMS = 50_000_000_000_000_000`;
- `ECONOMIC_YEAR_SECONDS = 31_536_000`;
- `ECONOMIC_YEAR_NS = 31_536_000_000_000_000`;
- atomic precision = 8 decimals.

For economic year epoch `e` and nanoseconds `r` elapsed inside that year, the implementation uses exact integer/rational arithmetic equivalent to the approved annual geometric schedule. `crates/pulsedag-core/src/monetary_v3.rs` computes the remaining mathematical supply and derives cumulative issuance as the floor of the exact curve.

No rounded decimal reward rate is consensus authority.

Per-position subsidy is:

`subsidy(M) = target_issuance(economic_time(M)) - target_issuance(economic_time(M-1))`

for `M > 0`; genesis subsidy is zero.

This cumulative-difference rule distributes atomic-unit remainders deterministically and prevents the sum of subsidies from crossing the hard cap.

## 6. Terminal atom rule

At atomic precision, the geometric remainder becomes smaller than one atom before an infinite mathematical tail can be represented.

The v3 candidate rule is:

- at the start of economic year 57, settle the final residual atom required to reach `MAX_SUPPLY_ATOMS` exactly;
- after the hard cap is reached, subsidy is permanently zero;
- eligible fees remain payable and are not new issuance.

The terminal vectors must include the last position below the cap, the position that reaches the cap and all positions after it.

## 7. Reordering and settlement

A crucial DAG rule is that monetary position is **state-derived**, not permanently self-declared by a block at mining time.

Before the frozen finality boundary protects a prefix, future DAG information may change deterministic order. Consequently:

- an immature reward position is provisional;
- authoritative state replay must be able to recompute provisional reward positions and subsidy amounts if the non-final ordered DAG changes;
- a block cannot authorize extra supply merely by embedding a subsidy amount based on its local view;
- the final issued reward UTXO must be bound to the canonical settled position and consensus-derived amount;
- no provisional reorder may cause cumulative issued supply to exceed the target curve or hard cap.

The legacy `block_subsidy(height)` and fixed caller-visible coinbase amount are therefore not sufficient as the v3 monetary authority.

## 8. Coinbase maturity and finality

The approved maturity is 3,600 economic seconds, but v3 spendability requires **both**:

1. at least 3,600 economic seconds have elapsed from the reward's canonical monetary position; and
2. the reward's ordered-DAG position is protected by the frozen v3 finality/settlement policy.

If one hour of economic time has elapsed but the position is not final, the reward remains unspendable.

The current conservative v2 finality baseline that freezes only genesis is not sufficient for production v3 reward settlement. A stronger explicitly versioned and adversarially tested finality/settlement boundary is launch-blocking.

## 9. Implementation authority

The pure schedule implementation lives in:

`crates/pulsedag-core/src/monetary_v3.rs`

It provides:

- cadence-segment validation;
- `economic_time_ns_for_score`;
- exact `target_issuance_atoms`;
- `subsidy_atoms_for_score`;
- economic-time maturity checking;
- 1/2/4 BPS equivalence vectors;
- annual supply checkpoints;
- terminal residual-atom vectors.

This module does not by itself authorize production reward settlement. Integration into authoritative ordered-DAG replay, UTXO maturity, finality and mining templates must be completed and frozen before `GO_V3_DUAL_LAUNCH`.

## 10. Mandatory v3 settlement tests

Before freeze, CI/evidence must prove:

- identical monetary score/order from different valid block arrival permutations;
- every canonical non-genesis ordered block occupies exactly one position;
- no duplicate position after parallel-block merges;
- 1 BPS / 2 BPS / 4 BPS economic-time equivalence;
- cadence activation continuity at the exact activation score;
- exact year 1 through year 10 supply checkpoints;
- year-57 residual atom and permanent post-cap zero subsidy;
- provisional reorder replay produces one deterministic final reward ledger;
- rewards cannot be spent before both economic maturity and finality;
- an over-claimed or miner-self-declared subsidy cannot inflate supply;
- full replay proves `issued_supply <= MAX_SUPPLY_ATOMS` at every canonical state.

## Freeze boundary

The monetary-score algorithm is approved by this document. The launch manifest must still bind the exact:

- v3 DAG ordering implementation/version/digest;
- production cadence segment table;
- cadence activation boundaries;
- reward-settlement implementation digest;
- finality policy/version/digest;
- emission and reorder vectors;
- independent supply-accounting digest.

Until those are populated and validated, overall launch state remains `PRE_FREEZE` and `launch_ready=false`.