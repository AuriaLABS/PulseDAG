# PulseDAG v3.0.0 monetary policy

Status: **PRE-FREEZE / LAUNCH-BLOCKING UNTIL APPROVED**

Authority: this document defines the economic-consensus contract that must be frozen for the exact v3.0.0 release candidate before `GO_V3_DUAL_LAUNCH` can be recorded in #781.

## Safety rule

Values already present in development code are **implementation baselines, not automatic mainnet approval**. Mainnet policy is valid only when every field in the final policy table is populated, reviewed, implemented, covered by deterministic vectors and bound by digest in the v3 launch manifest.

No code default, test constant, private-testnet allocation or historical genesis output may silently become mainnet monetary policy.

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
| Initial block subsidy | `TBD` | Same algorithm or explicit test override |
| Subsidy schedule | `TBD` | Same algorithm or explicit test override |
| Subsidy interval/index basis | `TBD` | Explicitly frozen |
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

The final emission specification must define:

1. subsidy at genesis and at the first mineable block;
2. every boundary where subsidy changes;
3. integer rounding behavior;
4. terminal zero-subsidy behavior;
5. behavior under DAG reordering before finality;
6. whether red/non-canonical/otherwise non-rewarded blocks issue supply;
7. fee inclusion/distribution semantics;
8. overflow behavior and maximum representable amount;
9. exact total-issued-supply calculation at any canonical accepted state.

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
- one block before/at/after every subsidy transition;
- final non-zero subsidy and first zero-subsidy block;
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
- reference emission-vector digest;
- total terminal issuance or exact terminal-supply rule;
- genesis issuance/allocation digest;
- reviewer/decision references.

Any consensus-visible change to issuance, fees, burn, maturity or genesis allocation after freeze invalidates the relevant release, replay, wallet, genesis and launch evidence.