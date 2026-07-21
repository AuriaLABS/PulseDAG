# v2.3.0 legacy script compatibility

This file classifies version-pinned helpers retained after the v2.3.0 repository cleanup.

They are **not current public entrypoints**. Exact historical behavior remains useful for regression evidence and for the accepted staged-rehearsal implementation.

## Supported v2.3.0 entrypoints

- `scripts/v2_3_0_private_5n_1m_rehearsal.sh`
- `scripts/v2_3_0_private_5n_2m_rehearsal.sh`
- `scripts/v2_3_0_private_5n_4m_rehearsal.sh`
- other `scripts/v2_3_0_*` tools referenced by current workflows and runbooks
- neutral tools under `scripts/release/`, `scripts/tests/`, and repository maintenance entrypoints

## Classified historical families

| Pattern | Disposition | Reason |
|---|---|---|
| `scripts/v2_2_17_*` | Historical evidence helper | Reproduces v2.2.17 API/security evidence only |
| `scripts/v2_2_18_*` | Historical evidence helper | Reproduces v2.2.18 private-RC and perturbation evidence only |
| `scripts/v2_2_19_*` | Historical evidence helper | Reproduces v2.2.19 rehearsal/closeout evidence only |
| `scripts/docker_v2_2_19_*` | Historical evidence helper | Reproduces the v2.2.19 Docker rehearsal only |
| `scripts/v2_2_20_*` | Compatibility engine / historical evidence | Three accepted rehearsal engines are invoked only through v2.3.0 wrappers; remaining files reproduce v2.2.20 evidence |
| `scripts/tests/test_v2_2_17_*` | Historical regression | Protects retained v2.2.17 evidence behavior |
| `scripts/tests/test_v2_2_18_*` | Historical regression | Protects retained v2.2.18 evidence behavior |
| `scripts/tests/test_v2_2_19_*` | Historical regression | Protects retained v2.2.19 evidence behavior |
| `scripts/tests/test_v2_2_20_*` | Compatibility regression | Protects the accepted v2.2.20 harness engines used behind v2.3.0 wrappers |

## Rules

1. Current documentation and workflows must use v2.3.0 or neutral entrypoint names.
2. No new feature may be added only to a v2.2.x helper.
3. Behavior changes required by v2.3.0 must land in a current module or wrapper and retain regression coverage.
4. A legacy family can be deleted only after no current wrapper, workflow, test, or evidence procedure depends on it.
5. Historical helpers must never be cited as proof that the public testnet is ready or live.
