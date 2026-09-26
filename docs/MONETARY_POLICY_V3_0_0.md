# PulseDAG v3.0.0 monetary policy

Status: **SMOOTH FAIR-EMISSION POLICY IMPLEMENTED / EXACT-CANDIDATE EVIDENCE PENDING**

Authority: #781, #794, #1045. The prior 500M year-one / annual-halving curve and the older #1016 tail-emission proposal are superseded for the active #1045 integration line.

## Mainnet policy candidate

- hard cap: **1,000,000,000.00000000 PDG**
- atomic precision: **8 decimals**
- spendable mainnet genesis issuance: **0**
- premine / treasury / foundation allocation: **0**
- emission model: **deterministic smooth exponential decay**
- monetary half-life: **3 economic years / 94,608,000 economic seconds**
- fixed-point emission quantum: **21,600 economic seconds (6 hours)**
- Q64 decay factor per quantum: **18,443,825,056,137,834,748**
- year-one cumulative target: **206,299,474.01590029 PDG**
- tail emission: **none**
- terminal residual settlement: **quantum 247,333 / 5,342,392,800 economic seconds**
- coinbase maturity: **3,600 economic seconds**, plus the separately frozen settlement/finality rule
- ordinary transaction fees: **100% to the eligible reward recipient**
- consensus burn: **0%**
- programmable resource fees: **consensus-unreachable on v3.0.0 mainnet because smart-contract deployment/execution is INACTIVE**
- canonical monetary index: deterministic ordered-DAG ordinal, genesis = score 0
- raw block height / raw BPS / header blue score are not monetary authority

The exact integer implementation is `crates/pulsedag-core/src/monetary_v3.rs`.

## Policy fingerprint

Canonical bytes are embedded in `MONETARY_POLICY_CANONICAL_V3`.

SHA-256:

`1c1cdf61e46a59418d10315dd0fa705d7e7fd4dbe4d1bb0f855a808148838e36`

The policy version is `pulsedag-monetary-v3.0.0-smooth-v1`.

This digest binds economic rules only. The cadence table has a separate canonical SHA-256 produced by `monetary_cadence_fingerprint_v3`; persistence binds **protocol fingerprint + monetary-policy fingerprint + cadence fingerprint + explicit reward-finality policy version**. Final launch evidence must additionally bind the exact source/tree SHA, network identities, deterministic genesis identities, settlement/finality identity and artifact digests.

## Exact issuance rule

Let the hard cap be `Smax = 100,000,000,000,000,000` atoms. The intended continuous economic curve is:

`S(t) = Smax * (1 - 2^(-t / 3 years))`

Consensus does not evaluate floating point, logarithms or runtime powers. Instead it freezes:

- `EMISSION_QUANTUM_SECONDS = 21,600`
- `HALF_LIFE_QUANTA = 4,380`
- `Q64_ONE = 2^64`
- `DECAY_FACTOR_Q64 = 18,443,825,056,137,834,748`

At exact six-hour quantum `q`, remaining supply is derived with Q64 exponentiation by squaring. Between adjacent quantum checkpoints, cumulative issuance is interpolated using integer arithmetic only. Therefore emission changes smoothly at block cadence while the expensive exponential calculation remains deterministic and bounded.

For canonical score `s`:

`subsidy(s) = total_supply(s) - total_supply(s - 1)`

The cumulative-difference rule deterministically carries every rounding remainder. Due to fixed-point precision, the three-year checkpoint is **500,000,000.00000006 PDG**, a deterministic six-atom deviation from the ideal mathematical half. The final residual atom is settled at quantum 247,333, after which total supply is exactly the hard cap forever.

Fees are transfers and never increase total supply.

## Golden cumulative checkpoints

| Economic time | Cumulative PDG | Share of cap |
|---|---:|---:|
| 0 | 0.00000000 | 0% |
| 1 year | 206,299,474.01590029 | ~20.63% |
| 2 years | 370,039,475.05256347 | ~37.00% |
| 3 years | 500,000,000.00000006 | ~50.00% |
| 4 years | 603,149,737.00795019 | ~60.31% |
| 6 years | 750,000,000.00000006 | ~75.00% |
| 10 years | 900,787,434.25198757 | ~90.08% |
| 20 years | 990,156,866.79769632 | ~99.02% |
| 30 years | 999,023,437.50000001 | ~99.90% |
| terminal quantum | 1,000,000,000.00000000 | 100% |

These values are consensus golden vectors, not floating-point calculations used at runtime.

## Cadence and monetary time

Consensus economic time is derived from a versioned list of:

- `activation_score`
- `target_interval_ns`

A cadence change must activate at an exact canonical monetary score. Changing from 1 BPS to 2 BPS or 4 BPS changes reward granularity only; equal economic time maps to equal cumulative issuance.

This property is mandatory for a DAG: block count or DAG width must never change gross PDG issuance.

The production mainnet/testnet cadence tables are separate network-freeze inputs and remain invalid to invent before #781 freezes them. Until then, RPC policy metadata reports `production_cadence_frozen=false` and no production cadence fingerprint.

## Fair-launch and fee rules

The production v3 genesis creates **zero spendable PDG**. There is no premine, protocol treasury, foundation allocation, presale or ICO issuance path.

v3.0.0 fee disposition remains:

`eligible miner reward = scheduled subsidy + 100% eligible ordinary transaction fees`

Consensus burn is zero. There is no reflection mechanism and no permanent tail inflation. A future change to fee disposition, burn or issuance requires a separately versioned policy fingerprint and activation decision.

## Smart-contract boundary

v3.0.0 mainnet keeps smart-contract deployment/execution inactive. Monetary snapshot persistence fails closed if the chain state has contracts enabled. Therefore programmable compute/state/proof fee paths must not become consensus-active and cannot create, redirect or burn supply. Any later activation requires a separately versioned protocol decision and monetary-policy compatibility review.

## Live integration status

The #1045 integration line enforces the monetary contract at the live boundaries rather than only exposing a policy library:

- `/mining/template` uses the amountless v3 reward claim, rechecks protocol/policy/cadence bindings, finalizes the authoritative state root/hash, and only then exposes nonce-search work when a valid monetary sidecar is present;
- `/mining/submit` validates the state-derived canonical monetary reward and complete accepted-state supply before persistence/commit;
- live inbound P2P routes through the monetary runtime when activated-v2 capabilities and the persisted monetary identity agree;
- incoming, staged and pending P2P blocks must use the amountless reward-claim envelope; legacy amount-bearing coinbases and additional inputless issuance are rejected before they can become authoritative;
- daemon restore revalidates protocol identity, monetary binding, transient P2P queues, complete accepted-state supply and reward-finality compatibility;
- sidecar presence never activates v3 by itself, but it makes legacy fallback invalid: startup, inbound P2P, legacy-v1 Mining Protocol fallback, `/mine`, `/mine/preview`, mining jobs, PoW auto-run and PoW mine-capture all fail closed rather than authorizing height-based issuance;
- `/block/validate` uses the monetary validation path when the monetary sidecar is active;
- the persisted reward-finality policy must equal a finality engine actually implemented by the daemon;
- maturity/finality settlement is evaluated on every authoritative monetary state.

Legacy v2.x subsidy constants remain available for historical compatibility and tests, but monetary activation guards make those paths unreachable as v3 issuance authority.

## Remaining #1045 integration gates

This implementation does **not** by itself close #1045 or claim #781 launch readiness. The exact v3 candidate must still bind and prove:

- clean exact-head CI for this changed monetary fingerprint and all golden vectors;
- exact production mainnet/testnet cadence tables and fingerprints;
- exact production chain IDs, deterministic genesis timestamps/hashes and zero-allocation genesis identities;
- final production reward-finality policy;
- downstream explorer consumption of the monetary/denomination contract;
- exact-candidate reachability evidence demonstrating that no legacy or alternate issuance path is reachable;
- artifact/evidence digests tied to the exact #781 source/tree and network identities.

Any evidence bound to the superseded annual-halving policy fingerprint must be treated as invalid for this policy.
