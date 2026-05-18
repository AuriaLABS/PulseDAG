# Optional GPU mining

GPU mining for `pulsedag-miner` is optional. CPU mining remains the default and reference backend for normal operator use and for mandatory v2.2.16 release evidence.

The GPU backend is experimental until the v2.2.16 closeout evidence is complete. It is feature-gated, lives only in the standalone external miner application, and is not required to build or run the default CPU miner.

## Scope and non-goals

The GPU path does not change the miner/node contract:

1. fetch a mining template from a node
2. search for a valid nonce outside the node
3. verify the result through the canonical CPU/core PoW path
4. submit the solved block to the node

The GPU miner remains a standalone external app. It does not add:

- pool logic
- shares
- payouts
- accounting or payout tracking
- server-side coordination logic

## Build with the GPU feature

Default builds do not enable GPU support. Build the miner with the `gpu` feature only on hosts where GPU experimentation is intended:

```bash
cargo build -p pulsedag-miner --release --features gpu
```

If the miner is built without this feature, `--backend gpu` fails with a feature-not-enabled error and operators should use `--backend cpu` or rebuild with the command above.

## Runtime example

After building with the `gpu` feature, request the GPU backend explicitly:

```bash
./target/release/pulsedag-miner --node http://127.0.0.1:18080 --miner-address <addr> --backend gpu --loop
```

Optional device selection uses `--gpu-device <index>`. OpenCL batch/work defaults can be tuned with `PULSEDAG_MINER_GPU_BATCH_SIZE` and `PULSEDAG_MINER_GPU_WORK_SIZE` for experiments.

CPU remains the default when `--backend` is omitted:

```bash
./target/release/pulsedag-miner --node http://127.0.0.1:18080 --miner-address <addr> --loop
```

## Driver and OpenCL requirements

The experimental GPU backend discovers devices through OpenCL. A host intended for GPU mining needs:

- a GPU supported by the vendor OpenCL runtime
- a current vendor GPU driver installed for the operating system
- an OpenCL Installable Client Driver (ICD) loader available to the process, such as `libOpenCL.so` on Linux, `OpenCL.dll` on Windows, or the platform OpenCL framework on macOS
- at least one OpenCL GPU device visible to the user running `pulsedag-miner`

The default CPU build does not require OpenCL libraries, GPU drivers, or GPU hardware.

## Safety rule: CPU/core verification before submit

Every GPU-found nonce must be verified by the canonical CPU/core PoW path before submit. The miner must treat GPU output as an untrusted candidate until CPU/core verification confirms the canonical preimage, hash, target comparison, and acceptance rule.

If CPU/core verification fails, the miner must not submit that candidate block. This safety rule preserves the canonical PoW semantics even while the GPU backend remains experimental.

## Troubleshooting

### No OpenCL device

Symptoms include OpenCL initialization errors, no platforms, no GPU devices, or a device index that cannot be selected.

Actions:

- install or update the vendor GPU driver and OpenCL runtime
- confirm the OpenCL ICD loader is installed and visible in the runtime library path
- check that the selected `--gpu-device <index>` exists
- run the miner with `--backend cpu` on hosts without GPU/OpenCL support

### Feature not enabled

If `--backend gpu` reports that the miner was built without GPU support, rebuild with:

```bash
cargo build -p pulsedag-miner --release --features gpu
```

Alternatively, omit `--backend gpu` to use the default CPU backend.

### Kernel init failure

The v2.2.16 GPU backend is experimental and must not mine with a non-canonical kernel. If kernel initialization or canonical GPU-kernel availability fails, the miner should refuse GPU mining rather than submit a candidate from an unverified or simplified path.

Actions:

- check the error text for OpenCL driver/runtime details
- verify the host GPU supports the required OpenCL runtime
- fall back to `--backend cpu`
- record the failure as GPU evidence status `SKIP` or `FAIL`, depending on the release-evidence context

### CPU verification failure

A CPU verification failure means a backend candidate did not satisfy the canonical CPU/core PoW validation. The miner must discard the candidate and not submit it.

Actions:

- confirm the GPU path uses the exact canonical PoW material and target from the template
- verify endianness and nonce placement against the canonical CPU/core adapter
- retry on CPU with the same node/template flow when practical
- treat repeated failures as a GPU backend defect, not as a node or pool issue

### Stale template

A stale template means the node rejected or the miner skipped work because the template expired or no longer matches node state.

Actions:

- fetch a fresh `/mining/template` response
- avoid resubmitting candidates from old template IDs
- lower `--refresh-before-expiry-ms` only if the operator understands the stale-submit risk
- keep loop mode refreshing work after each accepted, rejected, or skipped attempt

## v2.2.16 status

GPU mining remains optional and experimental for v2.2.16. Mandatory closeout evidence remains based on the canonical miner/node contract and default CPU mining path. GPU evidence may be recorded as `PASS`, `SKIP`, `NOT_REQUESTED`, or `FAIL` with a reason, but missing GPU hardware must not fail the default CPU evidence gate.
