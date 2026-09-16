# Task 38 — AMD/OpenCL runtime phase 10 scope

This slice is a software-only runtime/launch contract built on top of the merged phase-9 canonical OpenCL kHeavyHash translation.

It deliberately does **not** activate the existing `--backend gpu` path yet.

## Included

- canonical OpenCL matrix-generation kernel using the same pinned XoShiRo256++ sequence and f64 rank test (`EPS=1e-9`) as the pinned `rusty-kaspa` `Matrix::generate` implementation;
- mandatory `cl_khr_fp64` capability gate; reduced-precision rank semantics are refused;
- dynamically loaded OpenCL context/queue/program/kernel/buffer/write/launch/readback/cleanup contract;
- deterministic global GPU selection by device index;
- checked buffer/work-size arithmetic;
- fail-closed program build, launch, readback and successful-path cleanup handling;
- runtime source assembled from the exact phase-9 shared math plus matrix/hash kernels;
- Linux/Windows Rust contract CI and OpenCL 1.2 source compilation without physical GPU hardware.

## Explicitly deferred

- wiring the driver into `GpuMiningBackend` / `ProtocolMiningBackend`;
- target filtering and `ProtocolPowWork::reverify_accelerator_hash` integration;
- physical AMD/ATI execution or CPU↔AMD hardware equivalence;
- multi-GPU/mixed-vendor runtime operation;
- packaged OpenCL runtime evidence, recovery/watchdog/soak;
- `GPU_MINING_AMD_PASS=true` or any launch GO.

The next slice may activate the OpenCL backend only after this exact runtime contract is accepted and must preserve canonical core re-verification before any result becomes submit-capable.
