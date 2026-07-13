# v2.3.0 GitHub Actions gates

This document maps the remaining v2.2.20 closeout / v2.3.0 review-preparation gates to manual GitHub Actions workflows.

All workflows preserve:

- `VERSION=v2.2.20`;
- Cargo workspace version `2.2.20`;
- `public_testnet_ready=false`;
- no smart-contract enablement;
- no embedded pool logic;
- no public-testnet live or burn-in claim.

## Workflows

| Gate | Workflow | Closeout meaning |
|---|---|---|
| Workspace validation | `.github/workflows/v2_3_0_workspace_validation.yml` | Strict fmt/check/test/clippy evidence. Required before closeout. |
| Mempool + transaction relay | `.github/workflows/v2_3_0_mempool_tx_relay_validation.yml` | Runs the implemented core/P2P/RPC/workspace mempool regression suites. Required before closeout. |
| Lag-injection selected-segment | `.github/workflows/v2_3_0_lag_injection_selected_segment.yml` | `schema_ci` mode validates artifact shape only. It is not closeout PASS evidence. `real` mode must prove actual isolated-node recovery and is expected to fail until the operator-driven drill is implemented. |
| Non-zero prune + restart/rejoin | `.github/workflows/v2_3_0_prune_restart_rejoin_validation.yml` | Runs the retained-set pruning, snapshot, auto-prune and restart/rejoin test gate. Required before closeout. |

## Required execution order

1. `v2.3.0 workspace validation`.
2. `v2.3.0 mempool tx relay validation`.
3. Existing staged network workflows on the same candidate commit:
   - 5N/1M baseline;
   - 5N/2M intermediate;
   - 5N/4M stress observe, with manifest inspection rather than workflow color alone.
4. `v2.3.0 lag-injection selected-segment evidence` in `schema_ci` mode only to validate current artifact shape.
5. Replace the schema-only lag script with a real drill, then rerun the same workflow in `real` mode.
6. `v2.3.0 prune restart rejoin validation`.
7. Update the closeout evidence index with artifact names, workflow run IDs, commit SHA and SHA-256 values.
8. Keep PR #732 blocked until the closeout decision can truthfully record `GO_TO_START_V2_3_0_REVIEW`.

## Important warning: lag-injection

The current `scripts/v2_3_0_lag_injection_selected_segment.sh` contains a `CI_MODE=1` path that writes synthetic/schema evidence. That path is useful for validating the manifest shape, but it does not prove:

- an actually isolated node falling 64+ selected blocks behind;
- real remote selected-tip discovery;
- correlated selected-segment locator/header/block exchange;
- parent-first block application;
- final convergence after rejoin.

Only the workflow run in `real` mode, backed by a real drill implementation, may be used as closeout evidence for selected-segment recovery.

## Artifact expectations

Each workflow uploads a named artifact under `ci-evidence/` containing logs, metadata and checksums where available. The closeout decision must cite the exact workflow run, commit, artifact name and SHA-256 value. A green workflow without the required artifact and manifest is insufficient for v2.3.0 review authorization.
