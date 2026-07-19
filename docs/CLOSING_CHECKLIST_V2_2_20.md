# v2.2.20 Closing Checklist

Date: 2026-07-19 UTC

This checklist closes `v2.2.20` active hardening only. It does not claim public-testnet launch, public-testnet readiness, or `v2.3.0` release readiness.

## Evaluated closeout

- Functional candidate: `e65c6c199e07214303b49f7863f5b4988a8ce107`
- Fix PR: `#755`
- Merge commit: `bbca0735b50f56dadb05747aa408ea88b3f1a900`
- Final Actions run: `29662737906`
- Final artifact: `v2_2_20_pr755_final_closeout_29662737906`
- Final artifact digest: `sha256:90d4ccb28c04231bb64daa61b911914d21f16041edccb888f06310f6f768a873`
- Final manifest checksum: `30cfa1c273bce7c93f60ca9c9c4a17f99130b598be007554dd88095024c7bf3a`

## Required closeout evidence

| Gate | Result | Evidence |
|---|---|---|
| Version guard | PASS | `VERSION=v2.2.20`; root Cargo workspace version `2.2.20`; no version bump in the evaluated candidate. |
| Public-testnet signal guard | PASS | Final manifest records `public_testnet_ready=false` and `thirty_day_public_testnet_clock_started=false`. |
| Workspace validation | PASS | Artifact `v2_3_0_workspace_validation_29662737906`; digest `sha256:b44c550899cc3e3552df5cb3bd6dffb290cbcb8e649903c2d60884a6f5098aa1`. |
| `5N/1M` baseline | PASS | Staged archive checksum `930c4c9699c80bc60add6e4a488ef486c08dd2f4ccbdbaafb1cb3cb165506374`. |
| `5N/2M` intermediate | PASS | Replacement staged archive checksum `39fcd1168b65b6e4009b847cffa93b776a5c59012e38706c119612c463a44207`. |
| `5N/4M` stress | PASS | Replacement staged archive checksum `77786930c5f78f4fdbb1703c6a0a68e18385c71009a373fe06066afc483c41bf`. |
| Staged evidence integrity | PASS | Artifact `v2_3_0_staged_network_29662737906`; digest `sha256:cee4c8d31d944e5790e04c1bb10b1fc9e028489a6527b056790863619a61ad82`; all stages match the evaluated candidate. |
| Mempool/transaction relay | PASS | Artifact `v2_3_0_mempool_tx_relay_29662737906`; digest `sha256:89ab41db4cad24a2d01be96200de20790fa826eba797d50998f7a7f991af2461`. |
| Selected-segment lag recovery | PASS | Artifact `v2_3_0_lag_injection_29662737906`; digest `sha256:85362dcb69643ba657109f7b374c274fd09f199d3331b5cb839484a09c4add21`. |
| Prune/restart/rejoin and restore confidence | PASS | Artifact `v2_3_0_prune_restart_rejoin_29662737906`; digest `sha256:097995cd80e2a371a229c664292c0a81d0e044f6b9d86494ebd566d4591275a0`. |
| Incident and waiver ledger | PASS | `artifacts/v2_2_20/closeout_decision/incident_waiver_ledger.md`; no unresolved Sev-1 closeout blocker and no waiver used. |

## Final decision

Decision: `GO_TO_START_V2_3_0_REVIEW`

All mandatory closeout gates passed on the same functional candidate. The historical `5N/2M`, `5N/4M`, restore-confidence, selected-segment and final-workspace blockers have replacement PASS evidence, so no closeout waiver is required.

The decision records permission to start the formal `v2.3.0` review only. It does **not** authorize changing `VERSION`, tagging or publishing `v2.3.0`, enabling smart contracts, adding embedded pool logic, or launching a public testnet.

## Guardrails after closeout

- `VERSION` remains `v2.2.20`.
- Cargo workspace version remains `2.2.20`.
- `public_testnet_ready=false` remains mandatory.
- The 30-day public-testnet clock has not started and must not be backdated.
- A `2.3.0` version bump requires a separate explicit maintainer approval after this closeout.

## Decision records

- `docs/V2_2_20_FINAL_EVIDENCE_INDEX.md`
- `docs/KNOWN_LIMITATIONS_V2_2_20.md`
- `artifacts/v2_2_20/closeout_decision/final_decision.md`
- `artifacts/v2_2_20/closeout_decision/v2_3_0_start_decision.md`
- `artifacts/v2_2_20/closeout_decision/incident_waiver_ledger.md`
