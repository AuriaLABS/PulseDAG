# Archived GitHub Actions workflows (inactive)

These seven **historical v2.3/v2.4 workflows** have been moved **byte-for-byte**, retaining each original Git blob SHA, from `.github/workflows/` into this audit-only directory as part of #1314. They are **NOT** GitHub Actions workflow entrypoints after the move, and cannot be triggered by a push or `workflow_dispatch` under their archive paths.

Reason: on v3.0 `main@d74a749b517aec5a9aaa508b4b8ad4e621110bec`, the seven legacy workflow runs completed immediately with zero jobs and a failure state. Each source file had a malformed YAML header (`name`/stray `workflow_dispatch` fragments); the exact GitHub parser diagnostic was not available. All currently active v3 CI/workflow sources are left unchanged.

| Previous workflow location under `.github/workflows/` | Archived file, same content | Original blob SHA |
| --- | --- | --- |
| `v2_4_x_wallet_custody_port.yml` | [file](v2_4_x_wallet_custody_port.yml) | `89bc4bc14d4adc7d73f157efdf0d90dfd9885657` |
| `v2_4_0_single_node_profile.yml` | [file](v2_4_0_single_node_profile.yml) | `adfa7403db1be048b40cb8aafbb0ad4fd6aa844f` |
| `v2_3_0_observability.yml` | [file](v2_3_0_observability.yml) | `bae9846d607654890180221efdd1e2ff75e2310d` |
| `v2_4_0_public_hardening.yml` | [file](v2_4_0_public_hardening.yml) | `d4da37790ce92d2e746519764923c2b0e727f812` |
| `v2_3_0_multi_host_rehearsal.yml` | [file](v2_3_0_multi_host_rehearsal.yml) | `e9392ab2e6a2535d2e71889b5d3d56a1014a1773` |
| `v2_3_0_direct_peer_accounting.yml` | [file](v2_3_0_direct_peer_accounting.yml) | `112fd469b544815707aaecfc6f7959b4a1680ba7` |
| `publish-v2.3.0.yml` | [file](publish-v2.3.0.yml) | `a3878076fb9e1ef5351f74237648a374c4074ed3` |

**Safety: especially the v2.3 publisher** — it contained GitHub Release/publication steps and `contents: write` permissions. Moving it here intentionally disables the entrypoint. Do not reintroduce the publisher into `.github/workflows/`, enable new automatic triggers or add a manual publication dispatcher without separate explicit maintainer review and release authorization. Do not treat archived v2 workflow results as v3 consensus or release proof.

References: #1314 (workflow hygiene), #781 (v3.0 mainnet/testnet NO-GO), #1309 (native-v3 consensus still not frozen). Smart contracts remain INACTIVE. No consensus/mining/runtime/release/active workflow change is made by this archive operation.
