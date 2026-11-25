# Temporary prototype test dependencies (first-network cleanup)

No historical network compatibility or deployment is supported. The old filename is temporarily retained because the repository version audit checks it. No old version is a current release target. Only retain a tool while a current regression or still-unreplaced development harness depends on it; see #1524 and #1465.

## Classified paths

- `scripts/v2_2_*`: remaining development evidence engines and snapshot/preflight fixtures, not native launch evidence.
- `scripts/docker_v2_2_*`: temporary containerized development harnesses.
- `scripts/windows/v2_2_*`: retired family; no new entrypoints.
- `scripts/tests/test_v2_2_*`: isolated historical regression fixtures pending native replacements.
- `scripts/v2-2-*`: retired family; no new entrypoints.
- `scripts/*_v2_2_*`: remaining versioned fixture helpers, not current deployment tools.

Remaining v2.3/v2.4 wrappers, measurement scripts and private-profile controllers are also transitional development dependencies, not compatibility obligations. The first-network launch gate cannot be satisfied by relabelling an old run. Maintain the active safety regressions until equivalent native coverage exists, then remove obsolete wrappers and engines. Never add a new prototype compatibility layer or deploy a private live network. See [first-network baseline](../docs/FIRST_NETWORK_BASELINE.md) and [historical provenance](../docs/archive/README.md).
