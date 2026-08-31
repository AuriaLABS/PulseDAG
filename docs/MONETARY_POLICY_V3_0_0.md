# PulseDAG v3.0.0 monetary policy

Status: **PRE-FREEZE / LAUNCH-BLOCKING UNTIL APPROVED**

Authority: this document defines the economic-consensus contract that must be frozen for the exact v3.0.0 release candidate before `GO_V3_DUAL_LAUNCH` can be recorded in #781.

## Safety rule

Values already present in development code are **implementation baselines, not automatic mainnet approval**. Mainnet policy is valid only when every field in the final policy table is populated, reviewed, implemented, covered by deterministic vectors and bound by digest in the v3 launch manifest.

No code default, test constant, private-testnet allocation or historical genesis output may silently become mainnet monetary policy.

## Approved policy direction: annual economic halving

The v3.0.0 monetary design now fixes the following requirement:

- mining subsidy is reduced by **50% every one economic year**;
- one economic year is exactly **31,536,000 economic seconds (365 days)**;
- leap years, wall-clock calendars and local timestamps do not affect consensus;
- the annual boundary is derived from the canonical consensus reward/DAA score selected for v3, never from ambiguous local block-arrival count;
- changing the public DAG cadence must not accelerate or slow the monetary schedule;
- the annual halving rule is therefore independent of whether the validated public cadence is approximately 1 s, 500 ms or 250 ms.

Conceptually:

`annual_subsidy_rate(epoch) = initial_subsidy_rate / 2^epoch`

where:

`epoch = floor(canonical_economic_seconds / 31_536_000)`

The implementation must use deterministic integer/fixed-point arithmetic or a precomputed exact schedule. Consensus monetary arithmetic must not use floating point.

Cadence examples for one full economic year, if the final reward score advances once per accepted reward interval:

- approximately 1 second cadence: 31,536,000 reward intervals/year;
- approximately 500 ms cadence: 63,072,000 reward intervals/year;
- approximately 250 ms cadence: 126,144,000 reward intervals/year.

These examples do not authorize block-height-based emission. The canonical v3 reward index remains a launch-blocking consensus freeze item.

### Interaction with terminal supply

The initial emission rate and terminal/max-supply target must be frozen together because an annual 50% reduction determines the geometric issuance envelope.

For illustration only, **not as an approved mainnet value**:

- an initial emission rate of 2 coins/economic-second with a 365-day annual halving would issue 63,072,000 coins during year 1 and converge toward approximately 126,144,000 coins before integer-rounding effects;
- therefore an initial rate of 2 coins/economic-second is incompatible with a 1,000,000,000-coin zero-premine target if the intention is to approach that full supply through mining;
- a zero-premine 1,000,000,000-coin geometric target would require a first-year mining budget of approximately 500,000,000 coins, equivalent to an average initial rate of approximately 15.85489599 coins/economic-second before exact integer scheduling and terminal-cap handling are specified.

No illustrative number above becomes consensus policy merely by appearing in this section. The exact initial subsidy, atomic precision, rounding rule and terminal-supply rule remain `TBD` until explicitly frozen.

## Current implementation baseline

The current v2.4.x-derived implementation contains:

- `GENESIS_SUPPLY = 1_000_000_000`;
- a development genesis output to `genesis-treasury`;
- `INITIAL_BLOCK_SUBSIDY = 50`;
- `SUBSIDY_HALVING_INTERVAL = 210_000` block heights;
- integer right-shift halvings until subsidy reaches zero;
- block coinbase output capped at `block_subsidy(height) + total_block_fees(block)`;
- one coinbase required as the first transaction;
- no coinbase-maturity rule identified in the current baseline.

These values/behaviors must be explicitly accepted or replaced for v3.0.0. In particular, the development `genesis-treasury` allocation is not authorized as a production premine merely because it exists in source.

The legacy `SUBSIDY_HALVING_INTERVAL = 210_000` height rule is **not compatible with the approved annual-economic-halving direction** and must be replaced before v3.0.0 monetary freeze.

## Required final policy table

All fields are launch-blocking until frozen.

| Field | v3 mainnet value | Parallel-testnet rule |
|---|---|---|
| Base monetary unit / atomic precision | `TBD` | Same encoding; economic value may be test-only |
| Display ticker/symbol | `TBD` | Distinct testnet presentation where needed |
| Genesis issued supply | `TBD` | Independently declared test allocation |
| Genesis allocation recipients | `TBD` | Independent test-only recipients |
| Premine/treasury/foundation allocation | `TBD` | Not inferred from mainnet |
| Maximum/terminal supply rule | `TBD` | Same consensus algorithm unless explicitly versioned |
| Initial block/economic subsidy | `TBD` | Same algorithm or explicit test override |
| Subsidy schedule | **50% reduction every 1 economic year** | Same algorithm unless explicitly versioned |
| Economic year | **31,536,000 economic seconds (365 days)** | Same consensus definition |
| Subsidy interval/index basis | canonical reward/DAA score: exact mapping `TBD` | Explicitly frozen |
| Coinbase maturity | `TBD` | Explicitly frozen |
| Ordinary transaction fee disposition | miner / burn / split: `TBD` | Explicitly frozen |
| Programmable compute/state fee disposition | `TBD` | Explicitly frozen |
| Proof-verification fee disposition | `TBD` | Explicitly frozen |
| Burn/recycling rule | `TBD` | Explicitly frozen |
| Dust/min-output policy if consensus-visible | `TBD` | Explicitly frozen |
| Supply accounting version | `TBD` | Same or explicitly distinct |
| Activation boundary | genesis / score / version: `TBD` | Explicitly frozen |

## Emission definition requirements

The final implementation must define subsidy as a deterministic pure function of one canonical consensus index. Because PulseDAG is a DAG, the policy must not rely on an ambiguous local arrival count.

Before freeze, #794 must record which canonical index is authoritative for emission (for example a frozen accepted-height/selected-order/DAA-score style index implemented by consensus). Every node must derive the same subsidy for the same accepted block independently of arrival order.

The annual-halving mapping must prove that the same amount of economic time produces the same emission schedule under every supported public cadence. A cadence change from 1 s to 500 ms or 250 ms must not multiply annual issuance.

The final emission specification must define:

1. subsidy at genesis and at the first mineable block;
2. every annual economic boundary where subsidy changes;
3. exact mapping from canonical reward/DAA score to economic seconds;
4. integer rounding and remainder-distribution behavior;
5. terminal zero-subsidy or exact hard-cap behavior;
6. behavior under DAG reordering before finality;
7. whether red/non-canonical/otherwise non-rewarded blocks issue supply;
8. fee inclusion/distribution semantics;
9. overflow behavior and maximum representable amount;
10. exact total-issued-supply calculation at any canonical accepted state.

## Genesis allocation rule

The final mainnet genesis must contain **only** allocations listed in the approved policy manifest. Any treasury, ecosystem, contributor, investor, foundation, liquidity, airdrop or other allocation must be explicit by amount, destination commitment and vesting/spending condition where applicable.

If the approved mainnet policy is fair-launch/no-premine, the production genesis generator must prove that no spendable genesis allocation is created. If a premine/allocation is approved, its exact amount and destination commitments must be published before genesis freeze.

The string `genesis-treasury` is a development placeholder and is prohibited as a production destination unless it is replaced by an explicitly approved, cryptographically valid production destination and allocation record.

## Coinbase and fee rules

Before mainnet GO the implementation must enforce, in consensus rather than miner convention:

- exactly one coinbase/reward transaction in every block that requires one;
- deterministic maximum subsidy for the canonical reward index;
- deterministic inclusion of eligible transaction/program/proof fees;
- no arbitrary caller-provided reward that can exceed consensus policy;
- coinbase maturity and spendability checks if maturity is non-zero;
- deterministic handling of under-claimed rewards;
- explicit prohibition or semantics for over-claiming;
- replay-safe network/domain binding for reward outputs where relevant.

## Supply invariant

A release-candidate test must compute, from genesis plus every canonically issued reward minus consensus burns, the exact circulating/issued-supply state and compare it with an independent reference implementation.

Required invariant:

`issued_supply = approved_genesis_issuance + canonical_mining_issuance - consensus_burns`

Fees transferred between users/miners are not new issuance. Fees destroyed by an approved burn rule reduce the relevant supply measure exactly as specified.

## Mandatory vectors

The final v3 evidence bundle must contain vectors for:

- genesis issuance;
- first mineable block;
- one reward interval before/at/after each annual economic halving boundary;
- at least the first 10 annual halving transitions;
- cadence-equivalence vectors at approximately 1 s, 500 ms and 250 ms;
- final non-zero subsidy and first zero-subsidy/hard-cap block;
- zero-fee and non-zero-fee coinbase;
- programmable/proof fee accounting where enabled;
- maximum-value and overflow boundaries;
- coinbase maturity boundary;
- DAG arrival/reordering permutations producing identical issuance;
- total-supply checkpoints across the full emission schedule.

## Freeze record

Before `GO_V3_DUAL_LAUNCH`, record in `docs/V3_0_0_LAUNCH_MANIFEST.md`:

- monetary-policy version;
- exact policy document SHA-256;
- exact implementation source SHA/tree;
- monetary constants/config digest;
- canonical annual-halving/reward-index mapping;
- reference emission-vector digest;
- total terminal issuance or exact terminal-supply rule;
- genesis issuance/allocation digest;
- reviewer/decision references.

Any consensus-visible change to issuance, fees, burn, maturity, economic-year definition or genesis allocation after freeze invalidates the relevant release, replay, wallet, genesis and launch evidence.