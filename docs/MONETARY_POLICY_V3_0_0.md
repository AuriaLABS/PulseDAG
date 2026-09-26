# PulseDAG v3.0.0 monetary policy — smooth emission proposal

Status: **PROPOSED POLICY REPLACEMENT / STACKED ON #1233 / NOT ACTIVATED**

Authority path: #781, #794 and #1045. This proposal intentionally changes the emission curve currently carried by #1233. It must not be treated as frozen production policy until the launch authority accepts the replacement and the exact-candidate evidence is regenerated.

## Proposed mainnet policy

- hard cap: **1,000,000,000.00000000 PDG**
- atomic precision: **8 decimals**
- spendable mainnet genesis issuance: **0**
- premine / treasury / foundation allocation: **0**
- emission curve: **smooth geometric decay**
- exact monetary half-life: **3 economic years / 94,608,000 economic seconds**
- monetary quantum: **21,600 economic seconds / 6 hours**
- Q64 per-quantum decay factor: **18,443,825,056,137,834,748**
- first economic-year scheduled issuance: **206,299,474.01590026 PDG**
- three-year cumulative issuance: **500,000,000.00000000 PDG**
- six-year cumulative issuance: **750,000,000.00000000 PDG**
- tail emission: **none**
- coinbase maturity: **3,600 economic seconds**, plus the separately frozen settlement/finality rule
- ordinary transaction fees: **100% to the eligible reward recipient**
- consensus burn: **0%**
- programmable resource fees: **consensus-unreachable on v3.0.0 mainnet while smart-contract deployment/execution is INACTIVE**
- canonical monetary index: deterministic ordered-DAG ordinal, genesis = score 0
- raw block height / raw BPS / header blue score are not monetary authority

The exact integer implementation is `crates/pulsedag-core/src/monetary_v3.rs`.

## Why replace annual halvings

The previous candidate emitted 500,000,000 PDG during the first economic year and then cut the annual mining budget by 50% at discrete one-year boundaries. That is deterministic, but it front-loads half of the entire hard cap into year one and creates large scheduled miner-revenue cliffs.

The smooth curve keeps the same fair-launch properties while spreading issuance over a longer security horizon:

| Economic time | Cumulative issuance | Hard-cap share |
|---:|---:|---:|
| 1 year | 206,299,474.01590026 PDG | 20.6299474% |
| 2 years | 370,039,475.05256342 PDG | 37.0039475% |
| 3 years | 500,000,000.00000000 PDG | 50% |
| 4 years | 603,149,737.00795013 PDG | 60.3149737% |
| 6 years | 750,000,000.00000000 PDG | 75% |
| 10 years | 900,787,434.25198753 PDG | 90.0787434% |
| 12 years | 937,500,000.00000000 PDG | 93.75% |
| 15 years | 968,750,000.00000000 PDG | 96.875% |
| 20 years | 990,156,866.79769630 PDG | 99.0156867% |
| 30 years | 999,023,437.50000000 PDG | 99.90234375% |

No permanent tail is introduced. The geometric residual is settled at the terminal 57th half-life boundary, equal to economic year 171, after which scheduled subsidy is exactly zero forever.

## Deterministic integer curve

Consensus uses no floating point, runtime logarithms or runtime exponentiation.

The frozen six-hour factor approximates:

`2^(-1 / 4380)`

where 4,380 quanta equal one three-year half-life.

For each half-life interval:

1. the exact integer remaining supply at the interval boundaries is derived from the hard cap;
2. a Q64 decay curve determines relative progress inside the interval;
3. that curve is normalized so every three-year boundary lands on the exact half-life checkpoint;
4. the cumulative amount is linearly interpolated inside each six-hour monetary quantum;
5. per-score subsidy remains the cumulative difference:

`subsidy(s) = total_supply(s) - total_supply(s - 1)`

This preserves exact telescoping supply accounting and deterministic remainder carry.

## Policy fingerprint

Canonical bytes are embedded in `MONETARY_POLICY_CANONICAL_V3`.

Proposed SHA-256:

`feb4fd1c466a03cbd73404ab2e38d8920941ff8c6cb4ba4614b21e4fd70d7b8b`

The policy version string is:

`pulsedag-monetary-v3.0.0-smooth-v1`

The cadence table remains separately fingerprinted. Persistence must continue binding protocol fingerprint + monetary-policy fingerprint + cadence fingerprint + reward-finality policy version.

## DAG and cadence invariants

Economic time remains derived from versioned `(activation_score, target_interval_ns)` cadence segments.

A cadence change must activate at an exact canonical monetary score. Moving between 1 BPS, 2 BPS and 4 BPS changes only reward granularity. Equal economic time must produce equal cumulative issuance.

The core invariant remains:

> Economic time determines how much PDG may exist. Block count and DAG width never determine gross issuance.

That prevents higher BPS or wider parallel DAG activity from silently accelerating supply.

## Fees, burn and funding

v3.0.0 keeps:

- miner/reward recipient: **100% of ordinary transaction fees**
- protocol treasury: **0%**
- consensus burn: **0%**
- premine: **0**
- ICO/presale/bootstrap issuance: **0**

Fees are transfers and never increase total supply. Any future treasury, burn, redistribution or programmable-fee split requires a separately versioned monetary-policy decision and a new fingerprint.

## Smart-contract boundary

Smart-contract deployment/execution remains inactive for v3.0.0 mainnet. This proposal does not activate PulseVM, programmable resource fees or any hidden issuance path.

## Integration boundary

This proposal is stacked on the #1233 monetary integration rather than replacing its safety architecture.

The following #1233 properties remain required:

- amountless reward claims before canonical ordered-DAG settlement;
- state-derived monetary score;
- protocol/policy/cadence fingerprint revalidation;
- no legacy `block_subsidy(height)` fallback under monetary activation;
- deterministic accepted-state total-supply audit;
- fail-closed P2P/mining/restore behavior;
- zero-allocation production genesis;
- coinbase maturity plus explicit finality/settlement;
- exact mainnet/testnet identity binding.

If this curve is accepted, all old annual-halving golden vectors and the old policy fingerprint become superseded evidence and must be regenerated on the exact candidate.
