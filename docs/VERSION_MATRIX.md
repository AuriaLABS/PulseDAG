# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v2.4.0` |
| Cargo workspace version | `2.4.0` |
| Current milestone | v2.4.0 Task31 release/activation candidate construction |
| Candidate state | Moving candidate; exact final SHA not frozen |
| Final decision | `PENDING_EXACT_CANDIDATE_EVIDENCE` |
| Protocol target | transaction v2 + block-header v2 + `ghostdag_v1` |
| Chain identity | Fresh v2.4 chain/genesis identity required; final digest not frozen |
| Release scope | node + standalone miner; no official custody wallet in the current candidate |
| High cadence | experimental/disabled by default |
| Tag | No `v2.4.0` tag created |
| Publication | GitHub Release publication not authorized |
| Public testnet | `public_testnet_ready=false` |
| 30-day clock | `thirty_day_public_testnet_clock_started=false` |
| Smart contracts | `contracts_enabled=false` |

## Version progression

| Version | Scope | Status |
|---|---|---|
| `v2.2.x` | Earlier private-testnet hardening and rehearsal | Historical |
| `v2.3.0` | Previous private-testnet release-candidate baseline | Historical baseline / compatibility evidence |
| `v2.4.0` | Versioned transaction/header protocol, GHOSTDAG stack, adversarial validation and final release activation | Active candidate construction |
| `v2.5.0` | Future scale/GPU/adversarial-resilience program | Future planning |
| `v2.6.0` | Future programmability program | Future planning |
| `v3.0.0` | Future Pulse Layer genesis: PulseClock, covenant templates, based-apps v0, reproducible verify, official user surface | Future planning; see `ROADMAP_V3_0_0.md`. Not authorized |

## v2.4.0 evidence state

Tasks 22–29 are completed. Task30 produced deterministic/adversarial validation for the pre-Task31 integrated candidate, but any release-freeze change that affects the candidate requires the affected evidence to be rerun on the final exact SHA. Task31 is therefore not allowed to combine old release-branch evidence with the moving candidate.

The current Task31 candidate is explicitly closing release blockers including:

- removal of raw-private-key wallet RPC behavior from the normal node;
- consistent `v2.4.0` VERSION/Cargo/repository identity;
- retirement of active v2.3-only release gates;
- explicit chain-bound v2 genesis/startup/storage/P2P activation wiring;
- exact-candidate packaging, recovery and release evidence.

## Current authorization boundary

`PENDING_EXACT_CANDIDATE_EVIDENCE` means none of the following is authorized:

- creating the `v2.4.0` tag;
- publishing a GitHub Release;
- launching the public testnet or recording Day 0;
- setting `public_testnet_ready=true`;
- starting or backdating the 30-day public-testnet clock;
- enabling high cadence by default;
- enabling smart contracts;
- enabling covenant templates or PulseClock as live protocol identity;
- claiming an official end-user custody wallet is part of this node/miner candidate;
- treating `ROADMAP_V3_0_0.md` as an activation contract.

## Repository version rule

Primary active repository surfaces must identify `v2.4.0` / `2.4.0` consistently and must preserve the pending/no-GO guardrails above. References to earlier versions are allowed only when clearly presented as historical baselines, compatibility inputs, migration evidence, or archive material. References to `v3.0.0` are allowed only as future planning.

## What `main` is (and is not)

`main` is the **moving v2.4.0 Task31 candidate line**, not a frozen release and not a public-testnet GO branch.

It currently also contains inactive v3/covenant/contract modules (`contract_v3`, `covenant_v1`, `tx_v3`, `mempool_v3`) and a v3 launch issue tree (`#781`, `#794`, `#1037+`, `#1041`). Those surfaces must remain disabled (`contracts_enabled=false`, compile-time contracts gate) and must not be described as part of the Task31 protocol identity (transaction v2 + header v2 + `ghostdag_v1`). Presence of that code on `main` is not activation.

| Line | Meaning |
|---|---|
| `main` | Moving Task31 construction line. Exact SHA not frozen. May contain inactive future code. |
| Frozen Task31 candidate | Does not exist until an exact SHA is recorded and evidence is rerun on that SHA. |
| v3 launch line | Planning and inactive foundation only. `#1041` does not authorize contract activation. `ROADMAP_V3_0_0.md` does not authorize covenant or PulseClock activation. |
| Historical `v2.3.0` / `v2.2.x` | Compatibility and provenance only. |

## Live control issues

| Topic | Live issue | Historical / closed this pass |
|---|---|---|
| Coordinated launch control | #781 | — |
| Integrated v3 program | #794 | — |
| RustSec / public-GO dependency record | #1127 (closed as tracker; `atty`/`hexplay` and `bincode` remain blockers) | #803 |
| Remaining crate-graph / bincode persist caps | #1153 (code) + `V3_BINCODE_MIGRATION_PLAN.md` | #1139 |
| Task31 audit parent | #1132 | #1115 |
| v2.4 / v3 identity mix | #1131 | #1123 |
| Stale active docs | closed | #1128 / #1120 |
| God-files / production `dead_code` | #1130 | #1122 |
