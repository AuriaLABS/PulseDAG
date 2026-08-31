# PulseDAG v3.0.0 monetary policy

Status: **MAINNET POLICY APPROVED / IMPLEMENTATION + TESTNET FREEZE PENDING**

Authority: this document defines the economic-consensus contract that must be implemented and frozen for the exact v3.0.0 release candidate before `GO_V3_DUAL_LAUNCH` can be recorded in #781.

## Safety rule

Values already present in development code are **implementation baselines, not automatic mainnet approval**. Mainnet policy is valid only when the approved values below are implemented, covered by deterministic vectors and bound by digest in the v3 launch manifest.

No code default, test constant, private-testnet allocation or historical genesis output may silently become mainnet monetary policy.

## Approved v3 mainnet policy

The following economic parameters are approved for the v3.0.0 mainnet design:

- maximum supply: **1,000,000,000.00000000 coins**;
- atomic precision: **8 decimal places**, so one coin is `100,000,000` atomic units;
- genesis-issued spendable mainnet supply: **0 coins**;
- premine / treasury / foundation allocation: **0 coins**;
- year-1 mining budget: **500,000,000.00000000 coins**;
- mining subsidy reduction: **50% every one economic year**;
- one economic year: exactly **31,536,000 economic seconds (365 days)**;
- equivalent year-1 average emission rate: approximately **15.854895991882293252 coins per economic second**;
- coinbase maturity: **3,600 economic seconds (1 economic hour)**;
- ordinary transaction fees: **100% to the eligible miner/reward recipient**;
- programmable compute/state fees: **100% to the eligible miner/reward recipient** for v3.0.0;
- proof-verification fees: **100% to the eligible miner/reward recipient** for v3.0.0;
- consensus fee burn: **0%** for v3.0.0;
- tail emission after the terminal monetary schedule: **none**;
- hard-cap rule: consensus MUST never permit issued supply above **1,000,000,000.00000000 coins**.

The display ticker/symbol remains a release-presentation item and does not change these consensus amounts.

## Approved policy direction: annual economic halving

The v3.0.0 monetary design fixes the following requirement:

- mining subsidy is reduced by **50% every one economic year**;
- one economic year is exactly **31,536,000 economic seconds (365 days)**;
- leap years, wall-clock calendars and local timestamps do not affect consensus;
- the annual boundary is derived from the canonical consensus reward/DAA score selected for v3, never from ambiguous local block-arrival count;
- changing the public DAG cadence must not accelerate or slow the monetary schedule;
- the annual halving rule is independent of whether the validated public cadence is approximately 1 s, 500 ms or 250 ms.

Conceptually, annual mining budgets are:

`annual_budget(epoch) = 500_000_000 coins / 2^epoch`

where epoch 0 is the first economic year and:

`epoch = floor(canonical_economic_seconds / 31_536_000)`

The implementation MUST NOT use a rounded decimal rate such as `15.85489599` as consensus authority. Consensus must derive issuance from the exact annual budget using deterministic integer/rational arithmetic or an equivalent precomputed exact schedule.

### First ten annual budgets

| Economic year | Mining budget | Maximum cumulative issuance |
|---|---:|---:|
| 1 | 500,000,000 | 500,000,000 |
| 2 | 250,000,000 | 750,000,000 |
| 3 | 125,000,000 | 875,000,000 |
| 4 | 62,500,000 | 937,500,000 |
| 5 | 31,250,000 | 968,750,000 |
| 6 | 15,625,000 | 984,375,000 |
| 7 | 7,812,500 | 992,187,500 |
| 8 | 3,906,250 | 996,093,750 |
| 9 | 1,953,125 | 998,046,875 |
| 10 | 976,562.5 | 999,023,437.5 |

These values are supply ceilings for the corresponding completed economic years before any explicitly defined burn. Fees do not create supply.

## Exact integer issuance rule

The monetary schedule must be implemented in atomic units and must be independent of public BPS.

Normative requirements:

1. `MAX_SUPPLY_ATOMS = 100_000_000_000_000_000`.
2. Genesis mainnet spendable issuance is exactly `0` atoms.
3. The first economic-year mining budget is exactly `50_000_000_000_000_000` atoms.
4. Each annual budget is one half of the preceding mathematical budget.
5. The canonical reward-index mapping determines the exact fractional progress through an economic year.
6. For any canonical reward position, cumulative scheduled issuance must be calculated with deterministic integer/rational arithmetic and a documented rounding rule; floating point is forbidden.
7. Per-block/reward subsidy is the difference between the cumulative scheduled issuance at the new canonical reward position and the previous rewarded position, subject to DAG reward eligibility.
8. A cadence change must alter reward granularity, not the amount scheduled for the same economic time.
9. When atomic-unit granularity makes further geometric subdivision smaller than one atomic unit, the final residual amount may be issued only through the explicitly tested terminal-remainder rule and must never cross `MAX_SUPPLY_ATOMS`.
10. After `MAX_SUPPLY_ATOMS` is reached, subsidy is permanently zero and miners receive only eligible fees.

The exact reward/DAA-score-to-economic-time mapping remains an implementation-freeze item because it must be tied to the final v3 consensus design, but it may not alter the approved annual budgets above.

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

These baseline values/behaviors are **not** the approved v3 mainnet policy except where explicitly replaced by this document. In particular, the development `genesis-treasury` allocation is not authorized as a production premine merely because it exists in source.

The legacy `SUBSIDY_HALVING_INTERVAL = 210_000` height rule is **not compatible with the approved annual-economic-halving direction** and must be replaced before v3.0.0 monetary freeze.

## Policy table

| Field | v3 mainnet value | Parallel-testnet rule |
|---|---|---|
| Base monetary unit / atomic precision | **8 decimals / 100,000,000 atoms per coin** | Same encoding |
| Display ticker/symbol | `TBD` | Distinct testnet presentation where needed |
| Genesis issued supply | **0 coins** | Independent test-only allocation: `TBD` |
| Genesis allocation recipients | **None** | Explicit test-only recipients: `TBD` |
| Premine/treasury/foundation allocation | **0 coins** | Not inferred from mainnet |
| Maximum/terminal supply rule | **1,000,000,000.00000000 hard cap** | Explicitly frozen |
| Initial annual mining budget | **500,000,000.00000000 coins** | Same algorithm or explicit test override |
| Equivalent initial average emission | **~15.854895991882293252 coins/economic-second** | Informational only; exact budget controls |
| Subsidy schedule | **50% reduction every 1 economic year** | Same algorithm unless explicitly versioned |
| Economic year | **31,536,000 economic seconds (365 days)** | Same consensus definition |
| Subsidy interval/index basis | canonical reward/DAA score: exact mapping `TBD` | Explicitly frozen |
| Coinbase maturity | **3,600 economic seconds** | Explicitly frozen |
| Ordinary transaction fee disposition | **100% eligible miner/reward recipient** | Same unless explicitly versioned |
| Programmable compute/state fee disposition | **100% eligible miner/reward recipient** | Same unless explicitly versioned |
| Proof-verification fee disposition | **100% eligible miner/reward recipient** | Same unless explicitly versioned |
| Burn/recycling rule | **0% burn in v3.0.0** | Explicitly frozen |
| Tail emission | **None** | Explicitly frozen |
| Dust/min-output policy if consensus-visible | `TBD` | Explicitly frozen |
| Supply accounting version | **v3 annual-economic-halving / hard-cap accounting** | Same or explicitly distinct |
| Activation boundary | **genesis for v3 mainnet monetary policy** | Explicitly frozen |

## Emission definition requirements

The final implementation must define subsidy as a deterministic pure function of one canonical consensus index. Because PulseDAG is a DAG, the policy must not rely on an ambiguous local arrival count.

Before freeze, #794 must record which canonical index is authoritative for emission. Every node must derive the same subsidy for the same accepted reward position independently of arrival order.

The annual-halving mapping must prove that the same amount of economic time produces the same emission schedule under every supported public cadence. A cadence change from 1 s to 500 ms or 250 ms must not multiply annual issuance.

The final emission implementation must define and test:

1. subsidy at genesis and at the first mineable reward position;
2. every annual economic boundary where subsidy changes;
3. exact mapping from canonical reward/DAA score to economic seconds;
4. integer rounding and remainder-distribution behavior;
5. terminal hard-cap behavior;
6. behavior under DAG reordering before finality;
7. whether red/non-canonical/otherwise non-rewarded blocks issue supply;
8. fee inclusion/distribution semantics;
9. overflow behavior and maximum representable amount;
10. exact total-issued-supply calculation at any canonical accepted state.

## Genesis allocation rule

The approved mainnet policy is a **fair launch / zero-premine policy**. Production mainnet genesis must create **zero spendable coin issuance**.

The production genesis generator must prove that no spendable genesis allocation is created. Any future attempt to add a treasury, ecosystem, contributor, investor, foundation, liquidity, airdrop or other allocation is a monetary-policy change that invalidates the v3 freeze and requires explicit consensus review.

The string `genesis-treasury` is a development placeholder and is prohibited as a production destination. The development `genesis-treasury` allocation is not authorized.

## Coinbase and fee rules

Before mainnet GO the implementation must enforce, in consensus rather than miner convention:

- exactly one coinbase/reward transaction in every block that requires one;
- deterministic maximum subsidy for the canonical reward index;
- deterministic inclusion of eligible transaction/program/proof fees;
- no arbitrary caller-provided reward that can exceed consensus policy;
- **3,600 economic-second coinbase maturity** and deterministic spendability checks;
- deterministic handling of under-claimed rewards;
- over-claiming is invalid;
- replay-safe network/domain binding for reward outputs where relevant;
- fees remain transfers, not new issuance;
- v3.0.0 applies **no consensus fee burn**.

## Supply invariant

A release-candidate test must compute, from genesis plus every canonically issued reward minus consensus burns, the exact issued-supply state and compare it with an independent reference implementation.

Required invariant:

`issued_supply = approved_genesis_issuance + canonical_mining_issuance - consensus_burns`

For approved v3.0.0 mainnet at genesis:

`approved_genesis_issuance = 0`

and while the v3 burn rate remains zero:

`issued_supply = canonical_mining_issuance`

At all times:

`0 <= issued_supply <= 100_000_000_000_000_000 atomic units`

Fees transferred between users/miners are not new issuance.

## Mandatory vectors

The final v3 evidence bundle must contain vectors for:

- zero mainnet genesis issuance;
- first mineable reward position;
- exact year-1 budget of 500,000,000 coins;
- one reward interval before/at/after each annual economic halving boundary;
- at least the first 10 annual halving transitions;
- cadence-equivalence vectors at approximately 1 s, 500 ms and 250 ms;
- integer remainder distribution within annual budgets;
- terminal residual-atom handling and first zero-subsidy position;
- zero-fee and non-zero-fee coinbase;
- programmable/proof fee accounting;
- maximum-value and overflow boundaries;
- 3,600-economic-second coinbase maturity boundary;
- DAG arrival/reordering permutations producing identical issuance;
- total-supply checkpoints across the full emission schedule;
- proof that `issued_supply` never exceeds the 1,000,000,000-coin hard cap.

## Freeze record

Before `GO_V3_DUAL_LAUNCH`, record in `docs/V3_0_0_LAUNCH_MANIFEST.md`:

- monetary-policy version;
- exact policy document SHA-256;
- exact implementation source SHA/tree;
- monetary constants/config digest;
- canonical annual-halving/reward-index mapping;
- reference emission-vector digest;
- exact hard-cap and terminal-remainder rule;
- zero-genesis-issuance proof/digest;
- reviewer/decision references.

Any consensus-visible change to issuance, fees, burn, maturity, economic-year definition, hard cap or genesis allocation after freeze invalidates the relevant release, replay, wallet, genesis and launch evidence.