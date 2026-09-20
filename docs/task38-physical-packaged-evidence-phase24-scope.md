# Task 38 Phase 24 — physical packaged evidence collector scope

This slice prepares an exact-candidate Linux/Windows **physical-evidence package** and a fail-closed operator collector. CI remains hardware-free.

The workflow:
- builds `pulsedag-gpu-equivalence-probe` with `gpu,cuda` on Linux x86_64 and Windows x86_64;
- compiles the kHeavyHash PTX from the exact candidate in a pinned CUDA Toolkit container, records candidate/source/module SHA provenance, and packages that PTX plus sidecar with the exact probe;
- packages the exact probe with SHA256, manifest, candidate commit and `build_features=["cuda","gpu"]`;
- verifies the extracted packaged probe can execute `--help` without GPU access;
- uploads the package as a short-lived CI artifact ready to transfer to a controlled physical GPU host.

The collector:
- requires the package archive, matching manifest and exact candidate SHA;
- verifies the canonical Phase 24 package tag, exact 40-hex candidate SHA, archive filename/SHA256/byte size, probe binary identity, build features, native Linux/Windows target, canonical repository + provenance run metadata, native archive format, and that the archive binds the candidate PTX + metadata sidecar;
- rejects path traversal and TAR link members, then extracts and executes the probe from that archive rather than accepting arbitrary probe or CUDA-module paths;
- requires the packaged PTX metadata candidate/repository/source/architecture/compiler provenance and verifies its SHA256 before execution;
- hashes the packaged probe and candidate-bound CUDA module;
- records host identity, command, timestamps, stdout and stderr;
- rejects repository-specific CUDA/OpenCL runtime override hooks;
- requires exactly one legacy-v1 and one activated-v2 vector line, each with seven canonical nonces, CPU-reference equality, and backend-specific exact-match/same-input states;
- can require AMD/ATI identity and requires same-input CUDA/OpenCL equality in `both` mode;
- refuses premature final GPU PASS promotion.

CI success proves only that the evidence package/collector are buildable and fail closed. It does not execute a physical GPU.

Physical-host success emits `physical_packaged_gpu_evidence=PASS` while retaining:
- `GPU_MINING_NVIDIA_PASS=NOT_CLAIMED`
- `GPU_MINING_AMD_PASS=NOT_CLAIMED`

Final promotion still requires the supported physical OS/runtime/device matrix, single/multi-GPU and mixed-vendor physical evidence where applicable, long-duration soak/device recovery, and the remaining Task 38/39/40 gates.
