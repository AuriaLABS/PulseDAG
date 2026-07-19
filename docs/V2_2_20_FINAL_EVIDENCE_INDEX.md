# v2.2.20 Final Hardening Evidence Index

Date: 2026-07-19 UTC

This index records the final evidence used to close `v2.2.20` active hardening and authorize the start of a formal `v2.3.0` review. It does not bump versions or claim public-testnet readiness.

## Evaluated candidate

- Candidate: `e65c6c199e07214303b49f7863f5b4988a8ce107`
- Pull request: `#755`
- Merge commit: `bbca0735b50f56dadb05747aa408ea88b3f1a900`
- Final workflow run: `29662737906`

## Release-control guardrails

| Guardrail | Required value | Closeout value | Status |
|---|---|---|---|
| `VERSION` | `v2.2.20` | `v2.2.20` | PASS |
| Cargo workspace version | `2.2.20` | `2.2.20` | PASS |
| Public-testnet signal | `public_testnet_ready=false` | `false` | PASS |
| 30-day clock | Not started | `thirty_day_public_testnet_clock_started=false` | PASS |
| Public-testnet launch claim | Forbidden | Not made | PASS |
| `v2.3.0` version bump | Separate approval required | Not included | PASS |

## Final evidence matrix

| Gate | Result | Artifact | Integrity reference |
|---|---|---|---|
| Workspace validation | PASS | `v2_3_0_workspace_validation_29662737906` | artifact digest `sha256:b44c550899cc3e3552df5cb3bd6dffb290cbcb8e649903c2d60884a6f5098aa1` |
| `5N/1M` staged baseline | PASS | `v2_3_0_staged_network_29662737906` | archive sha256 `930c4c9699c80bc60add6e4a488ef486c08dd2f4ccbdbaafb1cb3cb165506374` |
| `5N/2M` staged intermediate | PASS | `v2_3_0_staged_network_29662737906` | archive sha256 `39fcd1168b65b6e4009b847cffa93b776a5c59012e38706c119612c463a44207` |
| `5N/4M` staged stress | PASS | `v2_3_0_staged_network_29662737906` | archive sha256 `77786930c5f78f4fdbb1703c6a0a68e18385c71009a373fe06066afc483c41bf` |
| Staged index | PASS | `v2_3_0_staged_network_29662737906` | artifact digest `sha256:cee4c8d31d944e5790e04c1bb10b1fc9e028489a6527b056790863619a61ad82`; all three manifests match the evaluated candidate |
| Mempool and transaction relay | PASS | `v2_3_0_mempool_tx_relay_29662737906` | artifact digest `sha256:89ab41db4cad24a2d01be96200de20790fa826eba797d50998f7a7f991af2461` |
| Selected-segment lag injection | PASS | `v2_3_0_lag_injection_29662737906` | artifact digest `sha256:85362dcb69643ba657109f7b374c274fd09f199d3331b5cb839484a09c4add21` |
| Prune/restart/rejoin | PASS | `v2_3_0_prune_restart_rejoin_29662737906` | artifact digest `sha256:097995cd80e2a371a229c664292c0a81d0e044f6b9d86494ebd566d4591275a0` |
| Final decision | PASS | `v2_2_20_pr755_final_closeout_29662737906` | artifact digest `sha256:90d4ccb28c04231bb64daa61b911914d21f16041edccb888f06310f6f768a873`; manifest sha256 `30cfa1c273bce7c93f60ca9c9c4a17f99130b598be007554dd88095024c7bf3a` |

## Replacement of historical blockers

The final run replaces the earlier non-PASS evidence rather than waiving it:

1. `5N/2M` now has a strict PASS manifest with accepted mining activity and same-candidate archive integrity.
2. `5N/4M` now has a strict PASS manifest with same-candidate archive integrity.
3. Selected-segment lag recovery passes with correlated multi-chunk recovery and full retained-set convergence.
4. Prune/restart/rejoin passes with durable storage and restored/rejoined node checks.
5. Workspace format, check, package tests and clippy pass on the evaluated candidate.

## Incident and waiver ledger

No waiver was used. The reviewed closeout evidence contains no unresolved Sev-1 consensus, sync, storage or security blocker. The decision-scoped ledger is `artifacts/v2_2_20/closeout_decision/incident_waiver_ledger.md`.

## Final decision

### `GO_TO_START_V2_3_0_REVIEW`

The `v2.2.20` hardening closeout criteria are met. This permits the formal `v2.3.0` review to begin.

It does not authorize a `2.3.0` version bump, release tag, public-testnet launch, public-testnet readiness claim, or start of the 30-day burn-in clock. Those require separate decisions and evidence.
