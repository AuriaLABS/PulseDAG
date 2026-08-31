# PulseDAG Version Matrix

## Current repository baseline

| Area | Value |
|---|---|
| VERSION file | `v2.4.0` |
| Cargo workspace version | `2.4.0` |
| Current development line | v2.4.x / v2.4.1 follow-up integration toward v3 |
| Definitive public-launch target | **v3.0.0** |
| Target launch window | **Q4 2026 (October-December 2026)** |
| Launch model | **mainnet + parallel public testnet in one coordinated release window** |
| Final launch authority | #781 / `GO_V3_DUAL_LAUNCH` |
| Protocol direction | transaction/header v2 foundations + `ghostdag_v1`, with final v3 scope still to be frozen |
| v3 mainnet identity | `TBD` — chain ID/genesis/config/bootnodes not frozen |
| v3 parallel-testnet identity | `TBD` — independent chain ID/genesis/config/bootnodes not frozen |
| Production wallet | #819 gate; not yet approved for v3 production launch |
| Security | #803 gate; final v3 exact-candidate disposition pending |
| High cadence | separately gated; not implicitly authorized |
| Smart contracts | separately gated unless explicitly included in the frozen v3 launch scope |

## Published / historical release state

- `v2.4.0` is an already published node/miner release and remains immutable historical evidence.
- Later v2.4.x/v2.4.1 work is development and validation input toward the final v3.0.0 candidate.
- Existing v2.4.x binaries, tags, chain identities and evidence must not be relabeled as v3.0.0.
- The actual `VERSION` and Cargo workspace version remain `v2.4.0` / `2.4.0` until a reviewed final v3 candidate explicitly performs the version freeze.

## Version progression

| Version | Scope | Current interpretation |
|---|---|---|
| `v2.2.x` | Earlier private-testnet hardening and rehearsal | Historical |
| `v2.3.0` | Private-testnet release/readiness baseline | Historical / compatibility evidence |
| `v2.4.0` | Published protocol/node/miner validation release | Historical exact release + current repository version surface |
| `v2.4.1` / v2.4.x follow-ups | Wallet, relay, security and integration development | Active development input toward v3 |
| `v2.5.0` | Earlier future scale/GPU/adversarial-resilience planning | Planning requirements may be absorbed into #794; not a mandatory release rung |
| `v2.6.0` | Earlier programmability planning | Planning requirements may be absorbed into v3 or deferred; not a mandatory release rung |
| `v3.0.0` | Definitive long-lived public-launch release | **Q4 2026 target; mainnet + parallel testnet together** |

## Launch-strategy rebaseline

The former sequence of standalone public testnet -> Day 0 -> 30-day acceptance clock -> later production/mainnet progression is superseded.

For v3.0.0:

1. freeze the exact protocol/release candidate;
2. close #803 security and #819 production-wallet launch scope;
3. freeze independent mainnet and parallel-testnet network identities;
4. complete final release/adversarial/recovery/infrastructure evidence under #794;
5. record `GO_V3_DUAL_LAUNCH`, delay or no-go only in #781;
6. launch mainnet and parallel testnet in one coordinated release window and record independent first-block timestamps.

No exact day within Q4 is frozen yet.

## Legacy v2.4 validation markers

Some existing repository workflows and hygiene checks still require historical v2.4 no-GO strings. They are retained temporarily for compatibility while those surfaces are migrated:

- `PENDING_EXACT_CANDIDATE_EVIDENCE`;
- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

These strings do **not** mean the published v2.4.0 release is unpublished, and they do not define the v3 launch state. They must not be used as a substitute for #781's v3 exact-candidate launch record.

## v3.0.0 evidence rule

The final v3 launch evidence must be tied to one exact release candidate and the separately frozen mainnet/testnet identities. Evidence from incompatible source SHAs, dependency graphs, protocol activation contracts, signing domains, chain identities or genesis configurations must never be combined.

Any release-affecting code, dependency, storage, consensus, wallet-signing or activation change after candidate freeze requires an explicit evidence rebaseline and rerun of affected gates.

## Repository version rule

Until the final v3 version-bump change is reviewed, primary build/version surfaces must continue to identify the actual repository version `v2.4.0` / `2.4.0`. Documentation may identify **v3.0.0 as the future definitive launch target**, but must not claim that a v3 tag, binary, genesis or production network identity already exists.
