# PulseDAG v2.2.16 optional GPU miner backlog

GPU mining is allowed in v2.2.16 only as optional, experimental, external-miner work. It must not block the mandatory CPU miner/node contract gate.

## Hard rules

- GPU mining must live only in `pulsedag-miner` or an external miner crate/application.
- Do not add GPU mining to `pulsedagd`.
- Do not add pool logic.
- Do not add smart contracts.
- Do not change consensus rules.
- Default builds must work without GPU hardware or GPU drivers.
- CPU miner fallback must remain available.
- Any GPU-found result must be CPU-verified before submit.

## Preferred direction

Prefer a backend abstraction:

- `CpuBackend` as mandatory reference backend.
- `GpuBackend` as optional experimental backend.
- Shared template parsing.
- Shared canonical preimage construction.
- Shared target comparison.
- Shared submit path.

## Backend options

Preferred if practical:

- OpenCL backend for broader AMD/NVIDIA/Intel support.

Acceptable alternative if OpenCL is impractical:

- CUDA backend, clearly documented as NVIDIA-only.

Fallback if full kernel implementation is too large for v2.2.16:

- backend scaffolding.
- device detection.
- CLI flags.
- CPU fallback.
- GPU smoke script that skips cleanly.
- documentation of remaining implementation steps.

## Suggested CLI shape

Future miner flags may include:

```text
--backend cpu
--backend gpu
--gpu-list-devices
--gpu-device <id>
--gpu-batch-size <n>
--gpu-work-size <n>
```

## Smoke evidence policy

A GPU smoke script should report one of:

- `PASS`: GPU backend built, device detected, smoke vector passed, result CPU-verified.
- `SKIP`: GPU backend unavailable or no device detected on this host.
- `NOT_REQUESTED`: GPU feature was not requested for this evidence run.
- `FAIL`: GPU was explicitly requested and failed.

Mandatory v2.2.16 release evidence must not fail just because no GPU exists.

## Promotion criteria

GPU mining should not be considered canonical until:

- CPU and GPU produce identical results for known vectors.
- GPU result is CPU-verified before submit.
- default CPU build remains clean.
- no unsafe dependencies are required by default.
- operator docs exist.
- smoke evidence exists.
- no pool logic is introduced.
