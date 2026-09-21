# Task 38 Phase 25 — physical GPU evidence matrix verifier

This slice adds a fail-closed verifier for **physical evidence JSONs produced by the merged Phase 24 collector**. It does not execute GPU hardware and it does not choose release policy on its own.

## Contract

The verifier:
- accepts one canonical lowercase 40-hex candidate SHA plus one or more Phase 24 evidence JSON files;
- rejects evidence from another candidate, repository, schema, package tag or artifact provenance, and rechecks the native target/binary plus candidate-bound PTX sidecars;
- revalidates the combined captured stdout+stderr stream, rejecting premature/duplicate/conflicting markers there, then revalidates the two protocol cases, seven canonical nonces, CPU reference equality and backend-specific exact-match markers;
- requires nonempty 64-hex archive/probe/PTX digests, requires the exact Phase 24 CUDA metadata schema, reads `kheavyhash_launch.cu` from the requested candidate Git object (not the current worktree) and recomputes that source digest, rejects active repository CUDA/OpenCL override hooks, and parses the trailing AMD identity field instead of trusting arbitrary device-name text;
- requires packaged CUDA-module candidate/digest/source/PTX-arch/compiler provenance to remain bound to the evidence;
- classifies proven physical cells by vendor (`nvidia` / AMD-identified OpenCL) and host OS (`Linux` / `Windows`);
- classifies same-host cross-vendor evidence only when one `both` run contains physical CUDA identity, AMD OpenCL identity and exact same-input equality;
- supports explicit fail-closed requirements such as `--require-cell nvidia:Windows`, `--require-cell amd:Linux`, or `--require-cross-vendor-os Windows`;
- emits a machine-readable matrix summary while retaining both final GPU flags as `NOT_CLAIMED`.

## Boundary

A green Phase 25 CI run proves only that evidence verification logic is portable and fail-closed. It does **not** prove:
- any physical GPU execution by CI;
- physical multi-GPU correctness;
- physical watchdog/device recovery;
- long-duration soak;
- final NVIDIA or AMD release PASS.

Those gates require controlled physical-host evidence on the exact candidate and remain separately reviewable.

`GPU_MINING_NVIDIA_PASS=NOT_CLAIMED`
`GPU_MINING_AMD_PASS=NOT_CLAIMED`
