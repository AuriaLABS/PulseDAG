# PulseDAG Version Matrix

## Current baseline

| Area | Value |
|---|---|
| VERSION file | `v2.3.0` |
| Cargo workspace version | `2.3.0` |
| Current milestone | v2.3.0 private-testnet release candidate |
| Candidate state | Exact versioned candidate merged; final private-testnet release decision pending |
| Final decision | `PENDING_FINAL_CANDIDATE_EVIDENCE` |
| Tag | No `v2.3.0` tag created |
| Publication | GitHub Release publication not authorized |
| Public testnet | `public_testnet_ready=false` |
| 30-day clock | Not started |

## Version progression

| Version | Scope | Status |
|---|---|---|
| `v2.2.17` | API, operator, and security hardening | Historical |
| `v2.2.18` | Private-testnet RC preparation and evidence gates | Historical |
| `v2.2.19` | Private hardening and operator rehearsal | Historical |
| `v2.2.20` | Final v2.2 hardening, protected rehearsal, and closeout evidence | Historical baseline for v2.3.0 |
| `v2.3.0` | Current private-testnet release candidate | Active |

Historical documents are retained through [the archive index](archive/README.md). They are evidence and provenance, not current operator instructions.

## v2.3.0 accepted evidence

| Gate | Status | Reference |
|---|---|---|
| Protected five-node private-testnet rehearsal | `GO` | `docs/ROADMAP_V2_3_0.md` and Task 12 evidence records |
| Exact candidate contract | `PASS` | Candidate workflow `29800778099` |
| Linux x86_64 package and native smoke | `PASS` | Candidate workflow `29800778099` |
| Windows x86_64 package and native smoke | `PASS` | Candidate workflow `29800778099` |
| macOS x86_64 package and native smoke | `PASS` | Candidate workflow `29800778099` |
| Consolidated six-archive bundle | `PASS` | `v2_3_0_candidate_consolidated_29800778099` |
| Workspace format/check/tests/Clippy | `PASS` | Pre-burn-in verification on the exact candidate |
| RPC and release validation | `PASS` | Exact candidate checks |
| Repository hygiene | `PASS` | Exact candidate checks |

## Current authorization boundary

`APPROVE_RELEASE_CANDIDATE` authorized the versioned candidate and its validation only. It did not authorize:

- creating the `v2.3.0` tag;
- publishing a GitHub Release;
- launching a public testnet;
- setting `public_testnet_ready=true`;
- starting or backdating the 30-day public-testnet clock;
- introducing smart contracts or pool logic.

A separate final private-testnet release decision is required before tag creation or publication.

## Repository version rule

Active repository surfaces must identify `v2.3.0` as the current version. References to v2.2.x are allowed only when they are clearly historical, immutable baselines, migration inputs, compatibility notes, or archive links.
