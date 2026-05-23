# Release Notes v2.2.19

## RPC metadata and readiness correctness
- `/release` metadata is aligned with effective node behavior:
  - `version: v2.2.19`
  - `pow_algorithm: kHeavyHash`
  - `pow_engine: canonical_core`
  - `miner_mode: external_standalone_miner`
  - `smart_contracts: disabled_not_included`
  - `pool_logic: disabled_not_in_node`
- Release metadata intentionally avoids stale `sha256d` references.

## Status/readiness operator trust hardening
- `/readiness` now separates claims into explicit booleans:
  - `node_ready`
  - `private_testnet_ready`
  - `public_testnet_ready`
- `public_testnet_ready` is conservatively `false` by default for v2.2.19 unless explicit evidence gates are satisfied.
- `/readiness` includes operator-safe diagnostics and conservative warning/blocker surfaces:
  - `release_blockers`
  - `warnings`
  - `effective_rpc_bind`
  - `effective_api_profile`
  - `admin_enabled`
  - `storage_path_class` (class only, not raw private path)
  - `peer_health`
  - `mining_templates_available`


## Required closeout evidence path
- v2.2.19 closeout must be driven by `docs/CLOSING_CHECKLIST_V2_2_19_FINAL.md`.
- This is a pre-public-testnet hardening closeout path only (no v2.3.0 or v3.0 readiness claim).

## GPU mining closeout posture (honest scaffold)
- `pulsedag-miner` closeout path for v2.2.19 is **CPU-required** (`--backend cpu` or `--backend auto` with CPU fallback).
- `--backend auto` must fall back to CPU when GPU is not compiled/available and log the active backend.
- `--backend gpu` must fail clearly when no canonical GPU backend is compiled/available.
- v2.2.19 does **not** claim production GPU mining; GPU remains scaffold/optional until a canonical tested kHeavyHash GPU kernel exists with reproducible evidence.
