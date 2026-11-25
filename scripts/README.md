# PulseDAG scripts

## First-network tooling

The software candidate is v3.0.0. Public native-v3 testnet is the first deployed network; no prior chain migration or private live network is required.

- `v3_0_0_preflight_check.sh` — repository candidate identity and claim checks.
- `v3_0_0_single_node_preflight.sh` — isolated node preparation checks.
- `v3_public_testnet_identity_shape_gate.py` — pre-GO identity shape validation.
- `validate_v3_0_0_public_hardening.py` — fail-closed public preparation checks.
- `validate_v3_0_0_observability.py` — monitoring package validation.
- `v3_monetary_reachability.py` — monetary runtime boundary audit.
- `v3_compact_relay_5n4m_rehearsal.sh` — development/CI harness, not evidence of a deployed native monetary network.

## Release and maintenance

- `release/package_release_artifacts.py`
- `release/verify_release_artifacts.py`
- `release/render_install_guide.py`
- `release/flatten_candidate_assets.py`
- `release/standalone_operator_smoke.sh`
- `repository_hygiene.sh`
- `repository_version_surface_audit.py`
- `list_cleanup_candidates.sh`
- `validate_repo_cleanup.sh`

## Transitional test dependencies

Older prototype paths still referenced by CI are transitional test harnesses, not operator instructions or compatibility obligations. See [remaining dependency classification](../docs/PROTOTYPE_TEST_DEPENDENCIES.md). Replace the harnesses with native equivalents before removing associated regressions. New tools use neutral or native-v3 names.

## Safety boundary

No script or passing check authorizes monetary mining, smart-contract activation, public-testnet launch or the 30-day clock. Follow the [first-network launch runbook](../docs/runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md).
