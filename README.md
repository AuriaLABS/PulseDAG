# PulseDAG v2.2.20 active hardening / pre-public-testnet preparation

## Current milestone
- **v2.2.20 active hardening / pre-public-testnet preparation / pending evidence**.

## Execution gate
- v2.2.20 execution evidence is allowed only when version files prove `v2.2.20` / `2.2.20`.
- v2.2.20 can proceed only if v2.2.19 evidence is recorded and v2.2.18 is **CLOSED_WITH_EVIDENCE** or **WAIVED_WITH_REASON**.

## Guardrails
- No consensus changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Miner remains external.
- GPU optional/scaffold only unless canonical kernel evidence exists.
- No v2.3.0 readiness claim.
- No v3.0 readiness claim.
- Keep `public_testnet_ready=false` until explicit public-testnet gates pass.
- Do not bump `VERSION` unless explicit maintainer approval is recorded after gate evidence.

## References
- `docs/DOCKER_REHEARSALS_V2_2_20.md`
- `docs/V2_2_20_START.md`
- `docs/V2_2_19_PREFLIGHT.md`
- `docs/CLOSING_CHECKLIST_V2_2_19.md`
- `docs/RELEASE_NOTES_V2_2_19.md`
- `docs/RELEASE_EVIDENCE.md`
- `docs/VERSION_MATRIX.md`
- `docs/V2_3_0_START_CHECKLIST.md`
- `docs/V2_3_0_READINESS_DECISION_INPUTS.md`
