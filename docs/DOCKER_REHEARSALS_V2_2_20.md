# v2.2.20 Docker rehearsals

This document describes how to run the v2.2.20 private convergence rehearsals inside Docker.

The Docker wrapper does not replace the existing rehearsal scripts. It builds the workspace in a container and then delegates to the same v2.2.20 scripts used by CI:

- `scripts/v2_2_20_private_5n_1m_rehearsal.sh`
- `scripts/v2_2_20_private_5n_2m_rehearsal.sh`
- `scripts/v2_2_20_private_5n_4m_rehearsal.sh`

## Requirements

- Docker with Compose v2.
- `bash`, `jq`, `curl`, `tar`, and `gzip` available in the execution environment.
- Enough CPU and memory for five local nodes plus miners.
- No host services bound to the rehearsal ports when using host networking is not required; the default Compose setup runs everything inside one container process namespace.

Run the Docker-mode preflight from the host before starting a local Compose rehearsal:

```bash
bash scripts/v2_2_20_preflight_check.sh --docker-mode
```

Missing tools are reported as `ENV_FAIL` and classified as `failure_class=environment` so bad local setup is not confused with a `5N/1M`, `5N/2M`, or node/convergence failure.

## Windows local execution notes

PowerShell does not support Bash-style inline environment assignment such as `REHEARSAL_GATE=5n1m docker compose ...`. Use PowerShell environment variables instead:

```powershell
$env:REHEARSAL_GATE = "5n1m"
$env:REHEARSAL_OUT_DIR = "private_5n_1m_rehearsal"
docker compose -f docker-compose.rehearsal.yml up --build --abort-on-container-exit --exit-code-from rehearsal
```

For direct script execution on Windows, use Git Bash and invoke the repository scripts from the Git Bash shell. A common Git for Windows path is:

```text
C:\Program Files\Git\bin\bash.exe
```

The recommended Windows path is Docker Compose from PowerShell, because Compose uses the Linux rehearsal image with the required `bash`, `jq`, `curl`, `tar`, and `gzip` dependencies installed. Direct WSL/Git Bash runs should still execute the preflight first; if `/bin/bash` or `bash` is missing, install/repair the shell before treating the rehearsal as node evidence.

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

## Interpreting self-classifying evidence bundles

Each `5N/1M`, `5N/2M`, and `5N/4M` rehearsal writes `evidence_manifest.json` beside `evidence-summary.md` and includes the run-copy of that manifest inside `evidence.tar.gz`. The manifest is the first file to inspect because it carries the self-classifying result fields without requiring manual log grep:

- `result` is the harness result (`PASS`, `FAIL`, `ENV_FAIL`, or observe-only diagnostic state for the stress gate).
- `failure_class` groups the primary cause as `none`, `environment`, `timeout`, `convergence`, or `node`.
- `stage`, `node_count`, `miner_count`, and `duration` identify which gate produced the bundle and how long it ran.
- `git_ref`, `git_commit`, `version`, and `cargo_workspace_version` tie the evidence to the exact source and package versions without bumping `VERSION`.
- `rpc_liveness` separates live-listener curl timeouts, explicit RPC liveness timeouts, and stale/degraded snapshot counters.
- `sync_orphan` reports orphan backlog, missing-parent, INV-request, and orphan-recovery classification counters.
- `peers` reports active, recovering, cooldown, and rate-limited peer totals.
- `mining` reports template, submit, accepted, rejected, busy, and submit actor timeout totals.
- `checksums` records SHA-256 digests for the key summary, manifest-adjacent metadata, and archive artifacts when available.

Interpretation rules:

1. Treat `failure_class=environment` as a host/container setup problem, not as node or miner behavior.
2. Treat `failure_class=timeout` as an incomplete run unless the manifest also shows enough final endpoint captures to classify a node/convergence symptom.
3. Treat `failure_class=convergence` as a staged gate failure involving readiness schema, P2P connection, height/tip convergence, or required stage gates.
4. Treat `failure_class=node` as a runtime/node symptom that was not better classified as environment, timeout, or convergence.
5. For `5N/4M`, use the manifest as diagnostic stress evidence only; it does not claim public testnet readiness or v2.3.0 readiness.
