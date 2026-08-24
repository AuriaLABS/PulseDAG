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
- claiming an official end-user custody wallet is part of this node/miner candidate.

## Repository version rule

Primary active repository surfaces must identify `v2.4.0` / `2.4.0` consistently and must preserve the pending/no-GO guardrails above. References to earlier versions are allowed only when clearly presented as historical baselines, compatibility inputs, migration evidence, or archive material.
