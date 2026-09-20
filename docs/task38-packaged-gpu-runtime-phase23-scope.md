# Task 38 Phase 23 — packaged GPU miner runtime scope

This slice makes the production standalone miner release artifact explicitly GPU-capable at compile time and records that capability in artifact provenance.

## Contract

- The release workflow builds `pulsedag-miner` with Cargo features `gpu,cuda` on Linux x86_64 and Windows x86_64; the existing macOS artifact remains CPU-only.
- Linux/Windows packaged binaries must report `gpu_compiled=true` and `cuda_compiled=true` through the hardware-free `--version` path; macOS must continue to report both as `false`.
- Linux/Windows miner manifests record canonical `build_features=["cuda","gpu"]`; macOS records an empty feature list.
- Linux x86_64 and Windows x86_64 exact-candidate CI build, package, checksum/manifest verify and smoke-test the same feature-enabled miner surface.
- Existing node packaging remains unchanged.

## Evidence boundary

This is packaged **compile/runtime-surface** evidence. It does not establish that NVIDIA CUDA or AMD OpenCL drivers are installed on the runner, that a physical GPU executed from the packaged archive, that physical mixed-vendor mining works, or that either final GPU PASS flag can be recorded.

The #1038 Linux/Windows packaged/runtime checkbox therefore remains open until physical packaged-runtime evidence is attached to an exact candidate.

`GPU_MINING_NVIDIA_PASS=NOT_CLAIMED`
`GPU_MINING_AMD_PASS=NOT_CLAIMED`
