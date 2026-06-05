# v2.2.19 Docker rehearsals

This document describes how to run the v2.2.19 private convergence rehearsals inside Docker.

The Docker wrapper does not replace the existing rehearsal scripts. It builds the workspace in a container and then delegates to the same scripts used by CI:

- `scripts/v2_2_19_private_5n_1m_rehearsal.sh`
- `scripts/v2_2_19_private_5n_2m_rehearsal.sh`
- `scripts/v2_2_19_private_5n_4m_rehearsal.sh`

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

## Artifacts

The host `./artifacts` directory is mounted into the container at `/workspace/artifacts`.

Expected outputs are under:

```text
artifacts/v2_2_19/<rehearsal_out_dir>/
```

The harness should write:

- `current-run-dir.txt`
- per-run logs and endpoint captures
- `evidence.tar.gz`
- `evidence.tar.gz.sha256`

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
