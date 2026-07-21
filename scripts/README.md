# PulseDAG scripts

## Current v2.3.0 entrypoints

### Private-testnet rehearsal

- `v2_3_0_private_5n_1m_rehearsal.sh`
- `v2_3_0_private_5n_2m_rehearsal.sh`
- `v2_3_0_private_5n_4m_rehearsal.sh`
- `docker_v2_3_0_rehearsal.sh`

### Baseline and performance evidence

- `p2p_sync_rpc_baselines_v2_3_0.py`
- `p2p-sync-rpc-baseline.sh`
- `hot-path-baseline.sh`

### Release and verification

- `release/package_release_artifacts.py`
- `release/verify_release_artifacts.py`
- `release/render_install_guide.py`
- `release/flatten_candidate_assets.py`
- `release/standalone_operator_smoke.sh`

### Repository maintenance

- `repository_hygiene.sh`
- `repository_hygiene.py`
- `repository_version_surface_audit.py`
- `list_cleanup_candidates.sh`
- `validate_repo_cleanup.sh`
- `check_code_comment_language.py`

## Naming policy

- New current tools use a neutral name or the `v2_3_0_` prefix.
- Active workflows and operator documentation must not call a v2.2.x path directly.
- Version-pinned v2.2.x helpers are retained only as classified compatibility engines or historical evidence tools.
- The classification and removal rules are documented in [`LEGACY_COMPATIBILITY_V2_3_0.md`](LEGACY_COMPATIBILITY_V2_3_0.md).

## Safety boundary

Repository tools must preserve `public_testnet_ready=false` and must not start or backdate the 30-day public-testnet clock without a separate explicit launch decision.
