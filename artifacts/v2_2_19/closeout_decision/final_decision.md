# v2.2.19 Closeout Final Decision

- decision: `NO_GO`
- reviewer: `TBD`
- date (UTC): `2026-06-05`
- evaluated evidence source: `Uploaded Ubuntu evidence package plus follow-up Docker staged convergence summary`
- evaluated reachable runner commit: `5850129b1a940f7063e06500f7558b912147923c`
- non-reachable evidence commit cited by follow-up summary: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a`
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

## Follow-up Docker staged convergence summary

The follow-up Docker runner work is reachable at commit `5850129b1a940f7063e06500f7558b912147923c`. The originally cited evidence commit `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a` is not reachable in this repository, so the listed archive hashes remain supplemental until the archives or a reachable evidence commit are attached.

| Gate | Result | Closeout interpretation |
|---|---:|---|
| `5N/1M baseline` | `PASS` | Positive staged-progress signal; not enough to close v2.2.19 by itself. |
| `5N/2M intermediate` | `PASS` | Positive staged-progress signal; not enough to close v2.2.19 by itself. |
| `5N/4M stress` | `FAIL` / `OBSERVE_FAIL` | Blocking until PASS or explicit waiver with owner, approval date, scope, expiry, and exit criteria. |

The `5N/4M` stress summary records `peer count network non-zero: FAIL`, `4` distinct final tips after quiescence, and `512` orphans plus `512` pending missing parents on n2/n3/n4/n5. This is not accepted as a non-blocking limitation in the final decision.

## Blocking issues (NO-GO)

1. Local `3N/1M` runtime script failed with shell error: `node: unbound variable` in the earlier Ubuntu evidence package.
2. Private `5N/4M` runtime script failed with shell error: `idx: unbound variable` in the earlier Ubuntu evidence package.
3. Private `5N/4M` summary in the earlier Ubuntu evidence package falsely reported PASS despite runtime health being all-zero (`healthy=0`, `ready=0`, `peers=0`, `height=0`, `templates=0`, `accepted blocks=0`).
4. Required release workflow evidence is missing under `artifacts/v2_2_19/release_workflow/`.
5. Required snapshot/restore evidence is missing under `artifacts/v2_2_19/snapshot_restore/`.
6. Follow-up Docker `5N/4M` stress evidence remains non-PASS and records failed peer visibility.
7. The follow-up evidence commit `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a` is not reachable in this repository; archive hashes alone are insufficient without an attached artifact path or reachable commit.

## Automatic NO-GO enforcement rules

The following conditions automatically force `NO_GO` unless an explicit waiver is recorded (where waiver is allowed):

- Any shell/runtime script error (e.g., `unbound variable`).
- Any required node reports `healthy=0`.
- Any required node reports readiness `0`.
- All peers are `0` in `5N/4M` rehearsal.
- A peer-visibility metric records `FAIL` without replacement evidence proving the automatic all-zero-peer condition was not hit.
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

## Required waiver metadata for future non-PASS acceptance

Any future attempt to accept the `5N/4M` stress limitation as non-blocking must record:

- owner;
- UTC approval date;
- affected gate and exact scope;
- expiry as a UTC date or specific release boundary;
- exit criteria that prove `5N/4M` reaches PASS or prove with replacement metrics that the automatic peer-visibility NO-GO condition was not hit.

## Next required actions before reconsidering closeout

- Fix runtime script failures in local `3N/1M` and private `5N/4M` rehearsals, or retire the stale failed Ubuntu evidence with a complete replacement bundle.
- Re-run rehearsals and require truthful summary gating for health, readiness, peer visibility, height, miner templates, accepted blocks, and chain ID.
- Attach missing release workflow evidence.
- Attach missing snapshot/restore evidence.
- Attach Docker evidence archives under repository artifact paths or cite a reachable evidence commit for every archive hash.
- Re-evaluate go/no-go only after complete runtime evidence passes or approved waivers are recorded.
