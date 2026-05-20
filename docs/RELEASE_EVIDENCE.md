# Release Evidence Policy (v2.2.17)

## Expected artifact directory
- Preferred: `artifacts/v2_2_17_api_security/<run_id>/`
- Alternate: `evidence/v2.2.17/<run_id>/`

## Required commands
1. `cargo fmt --check`
2. `cargo test --workspace`
3. `cargo build --workspace --release`
4. `bash scripts/v2_2_17_rpc_security_smoke.sh`
5. `bash scripts/v2_2_17_collect_api_security_evidence.sh`

## Expected outputs
- Command logs under `checks/`.
- Endpoint captures for `/health`, `/status`, `/release`, `/readiness` (+ optional `/metrics`, `/p2p/status`, `/sync/status`).
- `summary.md` and `evidence.tar.gz` in run directory.

## Pass/fail table template
| Item | Evidence path | Status |
|---|---|---|
| cargo fmt --check | `checks/cargo_fmt_check.txt` | PENDING |
| cargo test --workspace | `checks/cargo_test_workspace.txt` | PENDING |
| cargo build --workspace --release | `checks/cargo_build_release.txt` | PENDING |
| RPC smoke script | `artifacts/v2_2_17_rpc_security_smoke/<run_id>/summary.md` | PENDING |
| Evidence collector | `summary.md` + `evidence.tar.gz` | PENDING |

## Evidence bundle naming
- `v2_2_17_api_security_evidence_<run_id>.tar.gz` (or generated `evidence.tar.gz` inside run directory).

## Known missing evidence (current repo state)
- Runtime outputs are not committed in this documentation PR.
- Therefore v2.2.17 remains **PENDING EVIDENCE** until operators run required commands and attach artifacts.

- Cleanup pass 2 moved stale v2.2 historical docs into `docs/archive/v2_2_history/`.
