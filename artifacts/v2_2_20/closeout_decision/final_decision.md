# v2.2.20 closeout decision

Date: 2026-07-19 UTC

## Evaluated scope

- Functional candidate: `e65c6c199e07214303b49f7863f5b4988a8ce107`
- Fix PR: `#755`
- Merge commit: `bbca0735b50f56dadb05747aa408ea88b3f1a900`
- Final Actions run: `29662737906`

## Decision

Decision: `GO_TO_START_V2_3_0_REVIEW`

All mandatory `v2.2.20` hardening closeout gates passed on the same functional candidate:

- workspace format/check/tests/clippy;
- staged `5N/1M`, `5N/2M` and `5N/4M`;
- mempool and transaction relay;
- selected-segment lag recovery with retained-set convergence;
- prune/restart/rejoin and restore confidence.

No closeout waiver is used.

## Final evidence

| Evidence | Reference |
|---|---|
| Final manifest | `v2_2_20_pr755_final_closeout_29662737906` |
| Final artifact digest | `sha256:90d4ccb28c04231bb64daa61b911914d21f16041edccb888f06310f6f768a873` |
| Final manifest checksum | `30cfa1c273bce7c93f60ca9c9c4a17f99130b598be007554dd88095024c7bf3a` |
| Workspace artifact | `v2_3_0_workspace_validation_29662737906`, digest `sha256:b44c550899cc3e3552df5cb3bd6dffb290cbcb8e649903c2d60884a6f5098aa1` |
| Staged artifact | `v2_3_0_staged_network_29662737906`, digest `sha256:cee4c8d31d944e5790e04c1bb10b1fc9e028489a6527b056790863619a61ad82` |
| Relay artifact | `v2_3_0_mempool_tx_relay_29662737906`, digest `sha256:89ab41db4cad24a2d01be96200de20790fa826eba797d50998f7a7f991af2461` |
| Lag artifact | `v2_3_0_lag_injection_29662737906`, digest `sha256:85362dcb69643ba657109f7b374c274fd09f199d3331b5cb839484a09c4add21` |
| Prune/rejoin artifact | `v2_3_0_prune_restart_rejoin_29662737906`, digest `sha256:097995cd80e2a371a229c664292c0a81d0e044f6b9d86494ebd566d4591275a0` |

## Guardrails

- `VERSION=v2.2.20` remains unchanged.
- Cargo workspace version `2.2.20` remains unchanged.
- `public_testnet_ready=false` remains unchanged.
- `thirty_day_public_testnet_clock_started=false` remains unchanged.
- This decision does not authorize a `v2.3.0` version bump, release tag or publication.
- This decision does not authorize public-testnet launch or make a readiness/live claim.

The only authorized next state is the start of formal `v2.3.0` review.
