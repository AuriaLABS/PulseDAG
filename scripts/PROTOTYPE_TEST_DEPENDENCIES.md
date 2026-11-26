# Temporary prototype test dependencies (first-network cleanup)

No historical network compatibility or deployment is supported. No old version is a current release target. Only retain a tool while a current regression or still-unreplaced development harness depends on it; see #1524 and #1465.

## Classified paths

- `scripts/v2_2_*`: remaining development evidence engines and snapshot/preflight fixtures, not native launch evidence.
- `scripts/docker_v2_2_*`: temporary containerized development harnesses.
- `scripts/windows/v2_2_*`: retired family; no new entrypoints.
- `scripts/tests/test_v2_2_*`: isolated historical regression fixtures pending native replacements.
- `scripts/v2-2-*`: retired family; no new entrypoints.
- `scripts/*_v2_2_*`: remaining versioned fixture helpers, not current deployment tools.

Remaining v2.3/v2.4 wrappers, measurement scripts and private-profile controllers are also transitional development dependencies, not compatibility obligations. The first-network launch gate cannot be satisfied by relabelling an old run. Maintain the active safety regressions until equivalent native coverage exists, then remove obsolete wrappers and engines. Never add a new prototype compatibility layer or deploy a private live network. See [first-network baseline](../docs/FIRST_NETWORK_BASELINE.md) and [historical provenance](../docs/archive/README.md).

## Retired topology families

The historical family `configs/private-testnet/v2_2_*/*` (classified audit family) is no longer required for a previously deployed network. Its old topology files have been retired from the active tree. If a remaining test dependency requires a fixture, replace it with an explicitly isolated native development fixture and keep its safety checks.

No private live network or migration requirement exists. See the [first-network policy](../docs/runbooks/V3_FIRST_NETWORK_NATIVE_ONLY.md).

The strict repository audit rejects restoration of the old compatibility manifests. This classification is a transitional test dependency inventory, not a compatibility promise.
