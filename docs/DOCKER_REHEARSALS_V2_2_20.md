# v2.2.20 Docker rehearsals

This document describes how to run the v2.2.20 private convergence rehearsals inside Docker.

The Docker wrapper does not replace the existing rehearsal scripts. It builds the workspace in a container and then delegates to the same v2.2.20 scripts used by CI:

- `scripts/v2_2_20_private_5n_1m_rehearsal.sh`
- `scripts/v2_2_20_private_5n_2m_rehearsal.sh`
- `scripts/v2_2_20_private_5n_4m_rehearsal.sh`

## Requirements

- Docker with Compose v2.
- Enough CPU and memory for five local nodes plus miners.
- No host services bound to the rehearsal ports when using host networking is not required; the default Compose setup runs everything inside one container process namespace.

## Run a gate locally

```bash
REHEARSAL_GATE=5n1m REHEARSAL_OUT_DIR=private_5n_1m_rehearsal docker compose -f docker-compose.rehearsal.yml up --build --abort-on-container-exit --exit-code-from rehearsal
```

```bash
REHEARSAL_GATE=5n2m REHEARSAL_OUT_DIR=private_5n_2m_rehearsal docker compose -f docker-compose.rehearsal.yml up --build --abort-on-container-exit --exit-code-from rehearsal
```

```bash
REHEARSAL_GATE=5n4m REHEARSAL_OUT_DIR=private_5n_4m_rehearsal docker compose -f docker-compose.rehearsal.yml up --build --abort-on-container-exit --exit-code-from rehearsal
```

## Tunables

The same environment variables used by the rehearsal scripts can be passed through Compose:

```bash
REHEARSAL_GATE=5n2m \
DURATION_SECS=600 \
QUIESCENCE_WAIT_SECS=180 \
GLOBAL_DEADLINE_SECS=2700 \
REHEARSAL_OUT_DIR=private_5n_2m_rehearsal \
docker compose -f docker-compose.rehearsal.yml up --build --abort-on-container-exit --exit-code-from rehearsal
```

## CI workflows

Manual GitHub Actions workflows are available for the three v2.2.20 Docker rehearsal evidence gates:

- `.github/workflows/v2_2_20_5n_1m_baseline_evidence.yml` uploads `v2_2_20_5n_1m_baseline_evidence`.
- `.github/workflows/v2_2_20_5n_2m_intermediate_evidence.yml` uploads `v2_2_20_5n_2m_intermediate_evidence`.
- `.github/workflows/v2_2_20_5n_4m_stress_evidence.yml` uploads `v2_2_20_5n_4m_stress_observe_evidence`; this remains observe-only and records a diagnostic result without failing the workflow on a non-zero rehearsal exit.

Each workflow stages exactly these upload files from the run evidence root, falling back to archiving the current run directory when a prepackaged archive is missing:

- `ci-evidence/evidence.tar.gz`
- `ci-evidence/evidence.tar.gz.sha256`

The final workflow step fails the mandatory `5N/1M` and `5N/2M` jobs when their rehearsal exit code is non-zero. The `5N/4M` job keeps the non-zero exit code visible in logs while preserving the observe-only evidence contract for v2.2.20 stress diagnosis.

## Artifacts

The host `./artifacts` directory is mounted into the container at `/workspace/artifacts`.

Expected outputs are under:

```text
artifacts/v2_2_20/<rehearsal_out_dir>/
```

The harness should write both root-level and per-run evidence pointers/artifacts:

- `current-run-dir.txt`
- per-run logs and endpoint captures
- `evidence-summary.md`
- `summaries/package-metadata.txt`
- `evidence.tar.gz`
- `evidence.tar.gz.sha256`

`evidence.tar.gz` is created from the run directory and mirrored to the evidence root. The checksum is regenerated with `sha256sum` when available, with `shasum`/`openssl` fallbacks in the local harness and an explicit `sha256sum` regeneration step in CI before upload.

## Guardrails

This Docker setup is execution infrastructure only.

It does not change:

- consensus rules
- PoW semantics
- smart-contract behavior
- pool logic
- miner architecture
- version numbers
- public-testnet readiness status
