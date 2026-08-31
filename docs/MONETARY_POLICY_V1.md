# PulseDAG Monetary Policy v1

Status: **v3.0.0 candidate policy — consensus integration required before launch freeze**

Policy ID: `pulsedag-monetary-v1`

Fixed-parameter SHA-256: `70fafa18d010c4a3f99653ced2103f54b70030ae4471c61072e3b4db90d41503`

This document defines the production monetary intent for PulseDAG v3.0.0. It separates **how much PDG may exist** from **which accepted GHOSTDAG work receives that authorized issuance**. Raw block count, configured block cadence and BPS MUST NOT change cumulative authorized supply.

## 1. Fair-launch boundary

PulseDAG monetary v1 has:

- symbol: `PDG`;
- 8 decimal places;
- `100_000_000` atomic units per PDG;
- genesis monetary allocation: `0`;
- protocol treasury allocation: `0`;
- premine/presale/ICO allocation: `0`;
- discretionary administrative mint: none;
- private/bootstrap issuance: `0`.

The production chain may contain operator-mined bootstrap blocks before public mining is enabled, but those blocks MUST authorize **zero PDG issuance**. The economic/monetary clock starts only at the frozen permissionless public-mining activation boundary.

The exact public activation score/time binding is network configuration and MUST be committed into the final network/protocol identity before genesis/launch. It cannot be moved after launch without a separately versioned consensus activation.

## 2. Main emission

The economic reference curve is a smooth exponential approach to **1,000,000,000 PDG** with a five-monetary-year half-life:

`S_ref(t) = 1,000,000,000 * (1 - 2^(-t/5y))`

The one-billion-PDG value is a **main-emission reference**, not a permanent hard cap, because a fixed nominal tail emission begins later.

A monetary year is exactly `31_557_600` seconds (365.25 days). Five monetary years are `157_788_000` seconds.

Consensus MUST NOT evaluate floating point, `ln`, `pow`, platform math libraries or wall-clock time to calculate issuance.

## 3. Deterministic integer consensus curve

The v1 consensus approximation uses Q64 fixed-point decay at six-hour monetary quanta and integer linear interpolation inside each quantum.

Fixed constants:

```text
EMISSION_QUANTUM_SECS            = 21_600
Q64_ONE                          = 18_446_744_073_709_551_616
DECAY_FACTOR_Q64                 = 18_444_993_806_489_946_493
MAIN_EMISSION_REFERENCE_ATOMIC   = 100_000_000_000_000_000
```

For an exact quantum index `q`:

```text
decay(q)    = q64_pow(DECAY_FACTOR_Q64, q)
remaining   = floor(MAIN_EMISSION_REFERENCE_ATOMIC * decay(q) / 2^64)
main_supply = MAIN_EMISSION_REFERENCE_ATOMIC - remaining
```

`q64_pow` uses deterministic exponentiation by squaring and `q64_mul(a,b) = floor(a*b/2^64)`.

For time inside a six-hour quantum, authorized supply is the integer linear interpolation between the cumulative supplies at the surrounding quantum boundaries.

This schedule is deterministic, integer-only and independent of raw block count/BPS.

## 4. Tail emission

Tail activation is aligned to monetary quantum `35_388`:

```text
TAIL_START_SECS                       = 764_380_800
MAIN_SUPPLY_AT_TAIL_START_ATOMIC      = 96_518_997_121_567_008
MAIN_SUPPLY_AT_TAIL_START             = 965,189,971.21567008 PDG
TAIL_ANNUAL_ATOMIC                    = 482_594_985_607_835
TAIL_ANNUAL                           = 4,825,949.85607835 PDG/year
```

After tail activation:

```text
authorized_supply(t) =
    MAIN_SUPPLY_AT_TAIL_START_ATOMIC
    + floor(TAIL_ANNUAL_ATOMIC * elapsed_tail_seconds / MONETARY_YEAR_SECS)
```

The tail is a **fixed nominal amount**, not a fixed percentage. Percentage inflation therefore declines as supply grows.

Approximate/golden cumulative supply from the exact integer policy:

| Monetary time | Authorized supply |
| --- | ---: |
| activation | 0 PDG |
| 1 year | 129,449,436.70387591 PDG |
| 2 years | 242,141,716.74480103 PDG |
| 5 years | 500,000,000.00000012 PDG |
| 10 years | 750,000,000.00000012 PDG |
| 20 years | 937,500,000.00000006 PDG |
| tail start | 965,189,971.21567008 PDG |
| 30 years | 993,075,439.17255812 PDG |
| 50 years | 1,089,594,436.29412512 PDG |
| 100 years | 1,330,891,929.09804262 PDG |

The sub-atomic-reference differences at exact half-life boundaries are the frozen consequence of Q64 integer arithmetic and are part of the consensus vectors, not implementation error.

## 5. Authorized issuance and GHOSTDAG settlement

The monetary layer authorizes a budget over consensus monetary time:

```text
AuthorizedIssuance(t0, t1) = AuthorizedSupply(t1) - AuthorizedSupply(t0)
```

GHOSTDAG/accepted-work settlement determines **who receives** that budget. It MUST NOT determine how much total supply is authorized.

Required invariant:

```text
sum(settled miner subsidy) + deterministic carry/remainder
    == authorized issuance budget
```

Changing from 1 BPS to 10 BPS, 100 BPS or another approved cadence MUST NOT multiply or reduce gross authorized issuance for the same consensus monetary interval.

Any integer allocation remainder MUST be carried deterministically; it MUST NOT be silently burned, duplicated or minted.

## 6. Consensus monetary time

Raw block header timestamps MUST NOT be passed directly into the monetary function.

Production integration MUST define a `ConsensusMonetaryTime` derived from canonical accepted GHOSTDAG state with all of the following properties:

- monotonic;
- deterministic under replay and reorg;
- bounded against miner timestamp manipulation;
- independent of local wall clock except through already-consensus-bounded timestamp rules;
- maximum permitted forward advance per transition;
- recomputable from accepted consensus history;
- identical across supported architectures.

The public activation boundary defines monetary time zero. Bootstrap blocks before it do not advance authorized monetary supply.

## 7. Coinbase maturity

New subsidy outputs are not spendable until **86,400 seconds (24 hours) of consensus monetary time** after their settlement point.

Maturity is intentionally time/consensus based rather than a fixed block count so changing BPS cannot change the economic maturity horizon. Reorg/replay semantics remain authoritative: a reward that is no longer in canonical accepted settlement cannot remain spendable merely because local time passed.

## 8. Fees

Monetary v1 separates fees from gross issuance.

Initial disposition:

- ordinary transaction fees: 100% claimable by the eligible miner/settlement recipient;
- programmable compute/state/proof fees: accounted deterministically and routed to miners under the frozen fee policy;
- protocol treasury share: 0%;
- base-fee burn: 0% in monetary v1;
- fees MUST NOT increase `AuthorizedSupply`.

A later versioned fee policy may introduce burn or different resource pricing, but a demand-dependent burn MUST remain separate from deterministic gross PoW issuance accounting.

## 9. Supply invariants

Consensus/replay tests MUST prove at least:

1. genesis allocation is exactly zero;
2. bootstrap/private blocks before public monetary activation mint exactly zero;
3. cumulative minted subsidy never exceeds authorized supply;
4. complete eligible settlement converges to authorized supply modulo only the explicitly tracked deterministic remainder;
5. rejected/orphaned/non-canonical reward paths cannot create persistent supply;
6. deep reorg/replay cannot double mint;
7. BPS/cadence changes do not change cumulative authorized supply for the same monetary interval;
8. fee accounting cannot be mistaken for new issuance;
9. Q64 overflow/rounding boundaries match golden vectors;
10. tail transition is exact and monotonic;
11. 20/30/50/100-year vectors are stable;
12. aggregate monetary arithmetic uses at least `u128` even where individual transaction amounts remain `u64`.

## 10. Current implementation gaps that block production freeze

This policy document and `pulsedag-core::monetary_policy` define the candidate constants and deterministic supply function, but production launch MUST remain fail-closed until the existing consensus paths are migrated.

Known required integration work includes:

- activated genesis must be changed from the historical inherited treasury allocation to zero monetary allocation;
- activated mining templates must stop deriving subsidy from legacy `block_subsidy(height)`;
- activated coinbase validation must validate against authorized issuance/settlement rather than legacy per-height halving;
- `ConsensusMonetaryTime` must be frozen and implemented;
- monetary policy fingerprint must be bound into activated protocol/network identity and persisted restore/snapshot compatibility identity;
- coinbase maturity enforcement must be added to UTXO spend validation;
- GHOSTDAG subsidy allocation and deterministic remainder carry must be frozen and replay-tested;
- wallet/RPC/explorer amount formatting must consistently use 8 PDG decimals.

No production/mainnet GO follows from adding this candidate policy alone.

## 11. Canonical fixed-parameter digest

The canonical policy bytes are the exact UTF-8 text below, including the final newline:

```text
PulseDAG:monetary-policy:v1
symbol=PDG
decimals=8
atomic_units_per_pdg=100000000
main_emission_reference_atomic=100000000000000000
monetary_year_secs=31557600
half_life_secs=157788000
emission_quantum_secs=21600
decay_factor_q64=18444993806489946493
tail_start_quantum=35388
tail_start_secs=764380800
main_supply_at_tail_start_atomic=96518997121567008
tail_annual_atomic=482594985607835
genesis_allocation_atomic=0
treasury_bps=0
bootstrap_issuance_atomic=0
coinbase_maturity_secs=86400
fee_disposition=miner_100pct_no_burn_v1
activation_clock=consensus_monetary_time_since_public_activation
block_rate_dependency=none
```

SHA-256:

`70fafa18d010c4a3f99653ced2103f54b70030ae4471c61072e3b4db90d41503`

The exact network activation boundary is intentionally committed by the final mainnet/testnet network configuration/activation identity, not by this network-independent economic-policy digest.
