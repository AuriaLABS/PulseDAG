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
