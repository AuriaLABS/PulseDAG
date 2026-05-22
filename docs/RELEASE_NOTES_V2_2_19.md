# Release Notes v2.2.19

## RPC metadata and readiness correctness
- `/release` metadata is aligned with effective node behavior:
  - `version: v2.2.19`
  - `pow_algorithm: kHeavyHash`
  - `pow_engine: canonical_core`
  - `miner_mode: external`
  - `smart_contracts: disabled`
  - `pool_logic: disabled_not_in_node`
- Public release metadata removes stale `sha256d` references.

## Status/readiness operator trust hardening
- `/status` now includes explicit network/operator summary fields including `network_id` and `peer_summary` alongside existing `best_height`, `selected_tip`, `block_count`, `chain_id`, and `uptime_secs`.
- `/readiness` now reports effective runtime-facing config values and operator-safe classifications:
  - `effective_rpc_bind`
  - `effective_api_profile`
  - `admin_enabled`
  - `storage_path_class` (class only, not raw private path)
  - `peer_health`
  - `mining_templates_available`
