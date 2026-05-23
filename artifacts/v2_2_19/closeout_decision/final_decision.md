# v2.2.19 Closeout Final Decision

- decision: `NO_GO`
- reviewer: `TBD`
- date (UTC): `2026-05-23`
- evaluated evidence source: `Uploaded Ubuntu evidence package`
- evaluated commit: `5bc26be416e358a7370741d949191b24173a9ca6`
- evidence path: `artifacts/v2_2_19/closeout_decision/`

## What passed in Ubuntu evidence

- Preflight: `PASS (12/12)`
- Cargo check: `PASS`
- Cargo test: `PASS` (`505 passed`, `0 failed`, `1 ignored`)
- Cargo clippy: `PASS`
- Cargo build release: `PASS`
- Binary version checks:
  - `pulsedagd --version` => `2.2.19`
  - `pulsedag-miner --version` => `2.2.19`

## Blocking issues (NO-GO)

1. Local `3N/1M` runtime script failed with shell error: `node: unbound variable`.
2. Private `5N/4M` runtime script failed with shell error: `idx: unbound variable`.
3. Private `5N/4M` summary falsely reported PASS despite runtime health being all-zero (`healthy=0`, `ready=0`, `peers=0`, `height=0`, `templates=0`, `accepted blocks=0`).
4. Required release workflow evidence is missing under `artifacts/v2_2_19/release_workflow/`.
5. Required snapshot/restore evidence is missing under `artifacts/v2_2_19/snapshot_restore/`.

## Automatic NO-GO enforcement rules

The following conditions automatically force `NO_GO` unless an explicit waiver is recorded (where waiver is allowed):

- Any shell/runtime script error (e.g., `unbound variable`).
- Any required node reports `healthy=0`.
- Any required node reports readiness `0`.
- All peers are `0` in `5N/4M` rehearsal.
- All chain heights are `0`.
- All miner templates are `0`.
- Accepted blocks are `0` without explicit waiver.
- `chain_id` is unknown without explicit waiver.
- `evidence.tar.gz` is missing.
- Evidence checksum file is missing.

## Scope guard (conservative wording)

This decision is strictly for `v2.2.19` private/pre-public-testnet hardening.

- It is **not** a public testnet declaration.
- It is **not** `v2.3.0` readiness.
- It is **not** `v3.0` readiness.
- GPU mining remains scaffold/fallback for `v2.2.19`, not production mining.

## Waivers

- `none`

## Next required actions before reconsidering closeout

- Fix runtime script failures in local `3N/1M` and private `5N/4M` rehearsals.
- Re-run rehearsals and require truthful summary gating (all-zero health/readiness must fail).
- Attach missing release workflow evidence.
- Attach missing snapshot/restore evidence.
- Re-evaluate go/no-go only after complete runtime evidence passes.
