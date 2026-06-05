# v2.2.19 Closeout Decision

Date: 2026-06-05

## Decision

`v2.2.19` remains **NO_GO for closeout**.

The staged Docker convergence rerun is recorded as useful follow-up evidence, but it does **not** supersede the canonical final decision at `artifacts/v2_2_19/closeout_decision/final_decision.md` because the stress gate still has a non-PASS peer-visibility result and the evidence commit originally cited for the rerun is not reachable in this repository.

This decision does **not** declare public testnet readiness, does **not** start the 30-day burn-in clock, does **not** enable smart contracts, and does **not** change miner architecture. The miner remains an external application.

## Scope reviewed

Mandatory staged convergence gates observed in the Docker rerun:

- `5N/1M baseline`: PASS
- `5N/2M intermediate`: PASS

Diagnostic/stress gate observed in the Docker rerun:

- `5N/4M stress`: FAIL / `OBSERVE_FAIL`

The `5N/4M` stress result is **not accepted as a non-blocking limitation** in this closeout decision. It remains a closeout blocker until either:

1. a reproducible rerun reaches PASS with reachable evidence, or
2. an explicit waiver is approved with owner, UTC approval date, scope, expiry, and exit criteria.

## Evidence summary

### Docker 5N/1M baseline

Evidence source: local Docker rehearsal artifact generated from branch `codex/dockerized-v2219-rehearsals` after merge of Docker runner work.

- reachable runner commit in this repository: `5850129b1a940f7063e06500f7558b912147923c`
- originally cited evidence commit: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a` (**not reachable in this repository; not accepted as canonical auditable evidence by itself**)
- stage: `5N/1M baseline`
- result: `PASS`
- exit code: `0`
- runtime: `860s`
- version: `pulsedagd 2.2.19`
- node count: `5`
- miner count: `1`
- final height: `399` on all nodes
- distinct final tips after quiescence: `1`
- final tip: `f87c74234bd60ae31be498ba9296a436c50dd6205a5a5ced93571835b000b8c0`
- orphan count after quiescence: `0` on all nodes
- pending missing parents after quiescence: `0` on all nodes
- preflight: `PASS (12/12 explicit checks passed)`
- evidence archive sha256: `abbd49907aedd648a1b17db9d5657a89572130b47a060401641f554f29a8b6f5`
- auditable artifact status: `PENDING` until the archive or a reachable evidence commit is attached

### Docker 5N/2M intermediate

Evidence source: local Docker rehearsal artifact generated from branch `codex/dockerized-v2219-rehearsals` after merge of Docker runner work.

- reachable runner commit in this repository: `5850129b1a940f7063e06500f7558b912147923c`
- originally cited evidence commit: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a` (**not reachable in this repository; not accepted as canonical auditable evidence by itself**)
- stage: `5N/2M intermediate`
- result: `PASS`
- exit code: `0`
- runtime: `869s`
- version: `pulsedagd 2.2.19`
- node count: `5`
- miner count: `2`
- final height: `770` on all nodes
- distinct final tips after quiescence: `1`
- final tip: `94759868321353f35ccdbca05101b2574628f18c2e49ee4919b835202c2936c9`
- orphan count after quiescence: `0` on all nodes
- pending missing parents after quiescence: `0` on all nodes
- preflight: `PASS (12/12 explicit checks passed)`
- evidence archive sha256: `de3a93a8cd0bf0d88b00dcd392a2b6698180c22a4e844721a532d7e854d049f9`
- auditable artifact status: `PENDING` until the archive or a reachable evidence commit is attached

### Docker 5N/4M stress observe

Evidence source: local Docker rehearsal artifact generated from branch `codex/dockerized-v2219-rehearsals` after merge of Docker runner work.

- reachable runner commit in this repository: `5850129b1a940f7063e06500f7558b912147923c`
- originally cited evidence commit: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a` (**not reachable in this repository; not accepted as canonical auditable evidence by itself**)
- stage: `5N/4M stress`
- result: `FAIL`
- exit code: `1`
- classification: `OBSERVE_FAIL` for the stress gate
- runtime: `885s`
- version: `pulsedagd 2.2.19`
- node count: `5`
- miner count: `4`
- accepted blocks: `1483`
- rejected blocks: `7417`
- distinct final tips after quiescence: `4`
- orphan pressure: `512` orphans and `512` pending missing parents on n2/n3/n4/n5
- peer count network non-zero: `FAIL`
- evidence archive sha256: `69995dd46e3d44872ec8f8abb1decfd2090355e968da3c544f844959d9f0e12c`
- auditable artifact status: `PENDING` until the archive or a reachable evidence commit is attached

## Blocking limitations for v2.2.19 closeout

`5N/4M` exposes remaining high-load stress behavior in orphan/parent recovery and peer retention. Because peer visibility is recorded as `FAIL`, the stress gate cannot be treated as a non-blocking closeout limitation without an explicit waiver.

Required waiver metadata for any future acceptance:

- owner: `TBD`
- UTC approval date: `TBD`
- scope: `5N/4M stress OBSERVE_FAIL with measured divergence, orphan pressure, missing-parent backlog, peer visibility, final tips, and recovery behavior`
- expiry: `TBD; must be bounded to a specific release or UTC date before acceptance`
- exit criteria: `5N/4M stress PASS with non-zero peer visibility and bounded orphan/missing-parent recovery, or a replacement stress metric proving the automatic NO-GO peer condition was not hit`

No waiver is approved by this document.

## Guardrails preserved

- No consensus-rule change is included in this closeout record.
- No PoW semantic change is included in this closeout record.
- No smart-contract runtime is enabled.
- No pool logic is added to the miner.
- The miner remains an external application.
- `public_testnet_ready` remains false.
- This closeout does not claim `v2.3.0` or `v3.0` readiness.

## Closeout status

`v2.2.19` staged local/Docker convergence closeout: `NO_GO_WITH_STAGED_PROGRESS`.

Next development version: `v2.2.20`, focused on turning the 5N/4M observe failure into a bounded, recoverable stress scenario without changing consensus semantics.
