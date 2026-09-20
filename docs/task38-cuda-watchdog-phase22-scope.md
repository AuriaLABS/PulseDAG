# Task 38 Phase 22 — accelerator watchdog timeout and quarantine scope

This slice adds software deadline detection for accelerator kernel completion in production CUDA and OpenCL paths without claiming a universally time-bounded end-to-end batch or physical cancellation primitive.

## Contract

- CUDA matrix/hash launches are followed by a non-blocking completion event; the host polls with `cuEventQuery` and a monotonic deadline, treating `CUDA_ERROR_NOT_READY` as incomplete work.
- OpenCL records an event on the final hash kernel in its in-order queue, calls `clFlush`, and polls `CL_EVENT_COMMAND_EXECUTION_STATUS` through `clGetEventInfo` until completion or deadline. Only after kernel completion does it perform the blocking host read, so timeout never abandons an in-flight DMA into Rust host memory.
- CUDA and OpenCL kernel-completion deadline expiry returns an explicit launch error to the owning worker path, which then applies the existing quarantine/redistribution policy.
- CUDA errors after GPU work has been submitted are classified separately and skip ordinary module/memory cleanup; teardown proceeds through the dedicated per-batch CUDA context instead.
- In the homogeneous CUDA backend, the worker layer already quarantines launch failures and deterministically replays the in-flight canonical nonce partition on a surviving CUDA worker.
- Each production CUDA batch uses a dedicated context. On watchdog expiry, ordinary module/memory cleanup is skipped and context teardown owns cleanup, avoiding synchronous `cuMemFree` on known-incomplete work. CUDA documents context resource cleanup but does not give this slice a hard upper bound for `cuCtxDestroy()` itself.
- OpenCL timeout cleanup releases event, memory, kernel, program, queue, and context references; if submitted work is not confirmed complete (or cleanup itself fails), the loaded OpenCL runtime library handle is intentionally retained for process lifetime so deferred driver work cannot outlive an unloaded ICD/runtime. This is resource/lifetime safety, not a claim that a physically hung command is synchronously cancelled.
- `PULSEDAG_MINER_CUDA_WATCHDOG_MS` and `PULSEDAG_MINER_OPENCL_WATCHDOG_MS` configure positive kernel-completion deadlines; both default to 30000 ms.

## Evidence boundary

The deterministic CUDA shim can hold completion in `CUDA_ERROR_NOT_READY` so CI proves CUDA host deadline detection and context-teardown handling without a GPU. Hardware-free OpenCL unit contracts prove the event-status polling state machine; physical OpenCL hang/reset behavior remains separate evidence. Homogeneous CUDA and OpenCL workers remain quarantined across later jobs until a one-nonce launch-capability probe succeeds. Mixed CUDA+OpenCL jobs use the same fail-closed rule while also redistributing a failed device inside the current job. Interrupted canonical nonce ranges are replayed in chunks bounded by the replacement device's own batch size, preventing a large OpenCL batch from overrunning a smaller CUDA survivor (or vice versa).

This software contract does **not** prove that every physically hung NVIDIA/AMD kernel or device can be forcibly cancelled, reset, torn down, or recovered within a bounded end-to-end time on every supported OS/driver. Physical hang/recovery, host-transfer teardown behavior, and long-duration soak remain separate evidence gates.

`GPU_MINING_NVIDIA_PASS=NOT_CLAIMED`
`GPU_MINING_AMD_PASS=NOT_CLAIMED`
