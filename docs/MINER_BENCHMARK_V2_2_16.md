# v2.2.16 miner performance evidence harness

`v2.2.16` includes an optional, lightweight miner performance evidence harness for local and VPS operators. The harness measures standalone external miner behavior without changing consensus, mining protocol semantics, pool/share logic, or default CPU-miner behavior.

## Scope

The harness records these evidence fields when the configured node and miner binary are available:

- CPU backend hashes/sec.
- GPU backend hashes/sec when `pulsedag-miner` was compiled with the optional `gpu` feature and a usable GPU/OpenCL runtime is available.
- Accepted submits.
- Rejected submits.
- Stale skips performed before submit.
- Average template age at submit, computed from the miner-observed submit timestamp and the `created_at_unix` value returned by `/mining/template`.

The harness writes both CSV and JSON under `artifacts/v2.2.16/miner-benchmark/<UTC timestamp>/` and also writes a Markdown summary in the same directory.

## Guardrails

- The benchmark is optional release evidence, not a consensus test.
- CPU mining remains the default backend.
- The miner remains external and standalone.
- No pool coordination, share accounting, payout logic, or mining protocol changes are added.
- GPU is not required. GPU rows may report `not_requested` or `skip_gpu_unavailable_or_not_implemented`.
- Runs are bounded by `ITERATIONS`, `MAX_TRIES`, and `TIMEOUT_SECS` so the default is safe for laptops, local VMs, and small VPS hosts.
- The script does not start or mutate nodes. Point it at an already-running local/private node.

## Prerequisites

Build the miner first:

```bash
cargo build -p pulsedag-miner --release
```

Start or select a local/private node that exposes the v2.2.16 mining API:

- `POST /mining/template`
- `POST /mining/submit`

The mining template and submit contract are documented in [v2.2.16 miner/node contract](MINER_NODE_CONTRACT_V2_2_16.md).

## Basic run

```bash
bash scripts/v2_2_16_miner_benchmark.sh
```

Default settings are intentionally conservative:

| Variable | Default | Purpose |
| --- | --- | --- |
| `NODE_URL` | `http://127.0.0.1:8080` | Node RPC endpoint. |
| `MINER_ADDRESS` | `bench-v2_2_16-local` | Miner address sent to `/mining/template`. |
| `ITERATIONS` | `1` | Number of bounded miner invocations per backend. |
| `MAX_TRIES` | `50000` | Maximum nonce attempts per invocation. |
| `THREADS` | `min(nproc, 2)` | CPU worker threads unless overridden. |
| `TIMEOUT_SECS` | `45` | Per-invocation timeout. |
| `RUN_GPU` | `auto` | Try GPU and record skip status when unavailable. Use `false` to skip. |
| `GPU_DEVICE` | unset | Optional GPU device index passed to `--gpu-device`. |
| `BENCH_BUILD` | `0` | Set `1` to build `pulsedag-miner` before running. |
| `BENCH_STRICT` | `0` | Set `1` to exit non-zero if samples are incomplete. |
| `PULSEDAG_MINER_BIN` | `target/release/pulsedag-miner` | Miner binary path. |
| `ARTIFACT_DIR` | `artifacts/v2.2.16/miner-benchmark` | Artifact root. |

Example bounded local run:

```bash
NODE_URL=http://127.0.0.1:18080 \
MINER_ADDRESS=bench-local-1 \
ITERATIONS=3 \
MAX_TRIES=100000 \
THREADS=2 \
RUN_GPU=false \
bash scripts/v2_2_16_miner_benchmark.sh
```

Example optional GPU probe:

```bash
cargo build -p pulsedag-miner --release --features gpu
RUN_GPU=true GPU_DEVICE=0 bash scripts/v2_2_16_miner_benchmark.sh
```

If the optional GPU backend is unavailable, not compiled, or explicitly refuses to mine because canonical OpenCL kHeavyHash is not implemented, the harness records a skip status instead of failing default CPU evidence.

## Output files

For each run, the harness writes:

- `miner_benchmark.csv` — one row per backend.
- `miner_benchmark.json` — machine-readable backend summary.
- `summary.md` — operator-readable summary.
- `cpu.log` and `gpu.log` — timestamp-prefixed miner stdout/stderr used to calculate the metrics.

CSV columns:

| Column | Meaning |
| --- | --- |
| `backend` | `cpu` or `gpu`. |
| `status` | `pass`, `no_mining_sample`, `timeout`, `error`, `skip_binary_missing`, `not_requested`, or `skip_gpu_unavailable_or_not_implemented`. |
| `iterations` | Requested bounded invocations for that backend. |
| `hashes_per_sec_avg` | Average parsed `hashes_per_sec` across mining samples. |
| `accepted_submits` | Count of `submit_result: accepted=true`. |
| `rejected_submits` | Count of rejected submit outcomes. |
| `stale_skips` | Count of miner-side stale-template skips before submit. |
| `avg_template_age_secs` | Average submit timestamp minus template `created_at_unix`. |
| `log_path` | Raw timestamp-prefixed backend log. |

## Interpreting results

- `pass` means the backend produced at least one mining sample and the bounded invocation completed.
- `no_mining_sample` usually means the node was unreachable, the template endpoint failed, the run ended before a mining line was emitted, or difficulty/tries did not produce a canonical accepted nonce before the miner refused to submit.
- `accepted_submits` and `rejected_submits` depend on the node profile and target difficulty. A performance-only run may legitimately produce zero accepted submits unless the node/profile makes valid proofs easy enough for the bounded `MAX_TRIES`.
- `stale_skips` is miner-side stale-work protection and is expected only when the run reaches a template near or past expiry.
- GPU skip statuses are acceptable for v2.2.16 closeout on hosts without GPU support or without a canonical feature-gated GPU backend.

## Closeout evidence

Attach the generated `summary.md`, `miner_benchmark.csv`, `miner_benchmark.json`, and backend logs to the v2.2.16 release evidence bundle or release issue. This benchmark complements, but does not replace, contract tests, stale-template tests, telemetry tests, and multi-miner rehearsal evidence.
