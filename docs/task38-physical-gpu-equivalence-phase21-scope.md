# Task 38 Phase 21 — physical GPU equivalence probe scope

This slice prepares a fail-closed operator probe for **candidate physical evidence** only.

The probe:
- runs fixed legacy-v1 and activated-v2 canonical PoW vectors;
- compares CUDA output against the CPU protocol oracle;
- compares OpenCL output against the CPU protocol oracle;
- when both execute, requires CUDA and OpenCL to match on the same inputs;
- includes nonce 0 and `u64::MAX` in the physical vector set;
- reports the selected CUDA device identity and OpenCL vendor/device identity;
- can require that the selected OpenCL identity is AMD/ATI;
- refuses the repository-specific `PULSEDAG_CUDA_DRIVER_LIBRARY` and `PULSEDAG_OPENCL_LIBRARY` override hooks so the repository's CI mocks/shims cannot be promoted as physical evidence;
- still requires a controlled host/runtime environment for physical evidence; this guard does not claim to prevent arbitrary operating-system loader/search-path manipulation.

Passing this probe does **not** by itself set either release flag. It intentionally prints:
- `GPU_MINING_NVIDIA_PASS=NOT_CLAIMED`
- `GPU_MINING_AMD_PASS=NOT_CLAIMED`

Final PASS promotion still requires the complete supported OS/runtime/device matrix, packaged-runtime evidence, single/multi-GPU correctness where required, and the remaining Task 38/39/40 gates.

## CI software contract

The Phase 21 CI gate is deliberately hardware-free. On Linux and Windows it:
- checks formatting;
- compiles and runs the probe's unit contracts;
- verifies `--help` without touching a GPU runtime;
- runs strict Clippy for the probe;
- preserves the accepted Phase 19 mixed-runtime regressions;
- audits the anti-mock guards, AMD identity requirement surface, both protocol vector labels, boundary nonce coverage, canonical re-verification, and explicit final PASS non-claims.

CI success proves that the evidence collector is buildable and fail-closed. It does not prove physical GPU equivalence.
