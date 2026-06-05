# v2.2.19 Closeout Decision

Date: 2026-06-05

## Decision

`v2.2.19` is accepted as a **private hardening closeout** for staged local/Docker convergence gates.

This decision does **not** declare public testnet readiness, does **not** start the 30-day burn-in clock, does **not** enable smart contracts, and does **not** change miner architecture. The miner remains an external application.

## Scope accepted for closeout

Mandatory staged convergence gates:

- `5N/1M baseline`: PASS
- `5N/2M intermediate`: PASS

Diagnostic/stress gate:

- `5N/4M stress`: OBSERVE_FAIL, accepted as non-blocking diagnostic evidence for `v2.2.19`

## Evidence summary

### Docker 5N/1M baseline

Evidence source: local Docker rehearsal artifact generated from branch `codex/dockerized-v2219-rehearsals` after merge of Docker runner work.

- commit: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a`
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

### Docker 5N/2M intermediate

Evidence source: local Docker rehearsal artifact generated from branch `codex/dockerized-v2219-rehearsals` after merge of Docker runner work.

- commit: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a`
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

### Docker 5N/4M stress observe

Evidence source: local Docker rehearsal artifact generated from branch `codex/dockerized-v2219-rehearsals` after merge of Docker runner work.

- commit: `c058c2e1b5bf9e5224d61cfa1695428b4bb38c2a`
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

## Accepted limitation for v2.2.19

`5N/4M` exposes remaining high-load stress behavior in orphan/parent recovery and peer retention. This is accepted as a **non-blocking limitation** for `v2.2.19` because the mandatory staged gates `5N/1M` and `5N/2M` pass with reproducible Docker evidence.

The stress limitation must be tracked in `v2.2.20` as follow-up work.

## Guardrails preserved

- No consensus-rule change is included in this closeout record.
- No PoW semantic change is included in this closeout record.
- No smart-contract runtime is enabled.
- No pool logic is added to the miner.
- The miner remains an external application.
- `public_testnet_ready` remains false.
- This closeout does not claim `v2.3.0` or `v3.0` readiness.

## Closeout status

`v2.2.19` staged local/Docker convergence closeout: `PASS_WITH_5N4M_OBSERVE_LIMITATION`.

Next development version: `v2.2.20`, focused on turning the 5N/4M observe failure into a bounded, recoverable stress scenario without changing consensus semantics.
