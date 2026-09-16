from pathlib import Path

LIB = Path("apps/pulsedag-miner/src/lib.rs")
BACKEND = Path("apps/pulsedag-miner/src/opencl_backend.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


lib = LIB.read_text()
lib = replace_once(
    lib,
    '#[cfg(feature = "gpu")]\n#[path = "accelerator_scheduler.rs"]\nmod accelerator_scheduler_runtime;\n',
    '#[cfg(feature = "gpu")]\n#[allow(dead_code)]\n#[path = "accelerator_scheduler.rs"]\nmod accelerator_scheduler_runtime;\n',
    "scheduler dead-code scope",
)
LIB.write_text(lib)

backend = BACKEND.read_text()
backend = replace_once(
    backend,
    '''    search_work(\n        header,\n        work,\n        max_tries,\n        &device_indices,\n        batch_size,\n        backend.config().work_size,\n        launcher,\n        worker_health,\n    )\n''',
    '''    search_work(\n        header,\n        work,\n        max_tries,\n        OpenClSearchRuntime {\n            device_indices: &device_indices,\n            batch_size,\n            work_size: backend.config().work_size,\n            launcher,\n            worker_health,\n        },\n    )\n''',
    "search runtime call",
)
backend = replace_once(
    backend,
    '''fn search_work(\n    header: BlockHeader,\n    work: ProtocolPowWork,\n    max_tries: u64,\n    device_indices: &[usize],\n    batch_size: usize,\n    work_size: usize,\n    launcher: &dyn OpenClBatchLauncher,\n    worker_health: &OpenClWorkerHealth,\n) -> Result<NonceSearchResult> {\n''',
    '''struct OpenClSearchRuntime<'a> {\n    device_indices: &'a [usize],\n    batch_size: usize,\n    work_size: usize,\n    launcher: &'a dyn OpenClBatchLauncher,\n    worker_health: &'a OpenClWorkerHealth,\n}\n\nfn search_work(\n    header: BlockHeader,\n    work: ProtocolPowWork,\n    max_tries: u64,\n    runtime: OpenClSearchRuntime<'_>,\n) -> Result<NonceSearchResult> {\n    let OpenClSearchRuntime {\n        device_indices,\n        batch_size,\n        work_size,\n        launcher,\n        worker_health,\n    } = runtime;\n''',
    "search runtime struct",
)
BACKEND.write_text(backend)
