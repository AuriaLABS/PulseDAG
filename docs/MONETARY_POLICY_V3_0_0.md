# PulseDAG v3.0.0 monetary policy

Status: **POLICY PARAMETERS FROZEN / ACTIVATION INTEGRATION IN PROGRESS**

Authority: #781, #794, #1045. The superseded #1016 policy must not be used for v3.0.0.

## Frozen mainnet policy

- hard cap: **1,000,000,000.00000000 PDG**
- atomic precision: **8 decimals**
- spendable mainnet genesis issuance: **0**
- premine / treasury / foundation allocation: **0**
- first economic-year mining budget: **500,000,000.00000000 PDG**
- reduction: **50% every 31,536,000 economic seconds**
- tail emission: **none**
- coinbase maturity: **3,600 economic seconds**, plus the separately frozen settlement/finality rule
- ordinary transaction fees: **100% to the eligible reward recipient**
- consensus burn: **0%**
- programmable resource fees: **consensus-unreachable on v3.0.0 mainnet because smart-contract deployment/execution is INACTIVE**
- canonical monetary index: deterministic ordered-DAG ordinal, genesis = score 0
- raw block height / raw BPS / header blue score are not monetary authority

The exact integer implementation is `crates/pulsedag-core/src/monetary_v3.rs`.

## Frozen policy fingerprint

Canonical bytes are embedded in `MONETARY_POLICY_CANONICAL_V3`.

SHA-256:

`14605483aa65a17d654ffc4db1571b1416eb45b3f9b56af452d88c9023311366`

This digest binds the economic rules only. Final launch evidence must additionally bind the exact source/tree SHA, network identities, deterministic genesis identities, cadence schedule, settlement/finality identity and artifact digests.

## Exact issuance rule

The implementation uses integer-only cumulative issuance. Within each economic year the exact annual budget is distributed linearly over economic time. The annual budget halves at each 31,536,000-second boundary.

For canonical score `s`:

`subsidy(s) = total_supply(s) - total_supply(s - 1)`

This cumulative-difference rule deterministically carries rounding remainders and prevents hidden issuance. At the year-57 terminal boundary the final residual atom is settled, total scheduled supply reaches the hard cap exactly, and subsidy remains zero forever.

Fees are transfers and never increase total supply.

## Cadence and monetary time

Consensus economic time is derived from a versioned list of:

- `activation_score`
- `target_interval_ns`

A cadence change must activate at an exact canonical monetary score. Changing from 1 BPS to 2 BPS or 4 BPS changes reward granularity only; equal economic time must map to equal cumulative issuance.

The production mainnet/testnet cadence tables are separate network-freeze inputs and remain invalid to invent before #781 freezes them.

## Smart-contract boundary

v3.0.0 mainnet keeps smart-contract deployment/execution inactive. Therefore programmable compute/state/proof fee paths must not become consensus-active and cannot create, redirect or burn supply. Any later activation requires a separately versioned protocol decision and monetary-policy compatibility review.

## Remaining #1045 integration gates

This policy core does **not** by itself close #1045. Before closure the exact v3 candidate must also prove:

- zero-allocation deterministic production genesis;
- mining templates consume state-derived authorized issuance, not legacy `block_subsidy(height)`;
- coinbase validation consumes the same authorized issuance;
- coinbase maturity and settlement/finality are enforced;
- protocol and persisted activation identity bind the policy fingerprint;
- exact accepted-state total-supply accounting passes;
- wallet/RPC/explorer denomination semantics are explicit;
- no legacy or alternate hidden issuance path is reachable;
- golden vectors and the final policy digest are bound to the exact #781 release/network identities.

Legacy v2.x constants remain development compatibility only and are not v3 mainnet authority.
