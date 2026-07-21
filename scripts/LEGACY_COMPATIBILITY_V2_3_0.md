# v2.3.0 legacy script compatibility

Version-pinned v2.2.x helpers retained in this repository are compatibility or historical evidence tools. They are not current public entrypoints.

## Current entrypoints

- `scripts/v2_3_0_private_5n_1m_rehearsal.sh`
- `scripts/v2_3_0_private_5n_2m_rehearsal.sh`
- `scripts/v2_3_0_private_5n_4m_rehearsal.sh`
- `scripts/v2_3_0_private_rehearsal_compat.sh`
- `scripts/docker_v2_3_0_rehearsal.sh`
- `scripts/p2p_sync_rpc_baselines_v2_3_0.py`
- current `scripts/v2_3_0_*` tools referenced by active workflows
- neutral tools under `scripts/release/` and repository maintenance entrypoints

## Classified legacy families

- `scripts/v2_2_*`: version-pinned v2.2.x evidence and accepted compatibility engines.
- `scripts/docker_v2_2_*`: version-pinned Docker evidence engines.
- `scripts/windows/v2_2_*`: historical Windows evidence helpers.
- `scripts/tests/test_v2_2_*`: historical and compatibility regressions.
- `scripts/v2-2-*`: hyphenated historical release helpers.
- `scripts/*_v2_2_*`: suffix-versioned historical helpers.
- `scripts/v2_2_20_private_5n_4m_rehearsal.sh`: retained staged-network engine. Current wrappers execute a temporary transformed copy through `v2_3_0_private_rehearsal_compat.sh`, which changes visible version labels and the default artifact root while preserving internal helper paths.
- `scripts/p2p_sync_rpc_baselines.py`: retained v2.2.4 measurement engine used only through `p2p_sync_rpc_baselines_v2_3_0.py`, which rewrites run IDs, report identity, and CLI description to v2.3.0.

## Rules

1. Current documentation and workflows use v2.3.0 or neutral entrypoint names.
2. New functionality lands in current modules or wrappers, not only in a v2.2.x helper.
3. Retained compatibility engines keep regression coverage.
4. Historical helpers are not public-testnet readiness evidence.
5. New v2.2.x helper names fail repository hygiene.
6. Current evidence must not contain a visible v2.2.x identity or default to a v2.2.x artifact root.
