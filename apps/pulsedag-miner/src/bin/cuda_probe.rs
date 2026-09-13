#[path = "../cuda_launch.rs"]
mod cuda_launch;
#[path = "../cuda_runtime.rs"]
mod cuda_runtime;

use anyhow::{anyhow, Context, Result};
use cuda_launch::run_cuda_driver_launch_smoke;
use cuda_runtime::{discover_cuda_devices, run_selection_self_test, select_cuda_device};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut self_test = false;
    let mut device_index = None;
    let mut launch_smoke_ptx = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-test" => self_test = true,
            "--device" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --device"))?;
                device_index = Some(value.parse::<usize>().context("invalid --device index")?);
            }
            "--launch-smoke-ptx" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --launch-smoke-ptx"))?;
                launch_smoke_ptx = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                println!(
                    "usage: pulsedag-cuda-probe [--self-test] [--device INDEX] [--launch-smoke-ptx PATH]\n\nWithout --self-test, the probe loads the NVIDIA CUDA Driver API, enumerates devices, and selects device 0 or --device INDEX. With --launch-smoke-ptx PATH, it also creates a CUDA context, loads the supplied PTX module, copies one u64 input to device memory, launches pulsedag_cuda_runtime_smoke_kernel, synchronizes, and copies the output back. The launch smoke validates Driver API plumbing only; it does not establish kHeavyHash mining equivalence or GPU_MINING_NVIDIA_PASS."
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if self_test {
        if device_index.is_some() || launch_smoke_ptx.is_some() {
            return Err(anyhow!(
                "--self-test cannot be combined with --device or --launch-smoke-ptx"
            ));
        }
        run_selection_self_test()?;
        println!(
            "cuda_runtime_selection_selftest=PASS cuda_driver_kernel_launch=NOT_CLAIMED hardware_execution=NOT_CLAIMED mining_equivalence=NOT_CLAIMED"
        );
        return Ok(());
    }

    let discovery = discover_cuda_devices()?;
    let selected = select_cuda_device(&discovery.devices, device_index)?;
    println!(
        "cuda_runtime_discovery=PASS driver_version={} devices={} selected_index={} selected_name={:?} compute_capability={} mining_equivalence=NOT_CLAIMED",
        discovery.driver_version_display(),
        discovery.devices.len(),
        selected.index,
        selected.name,
        selected.compute_capability(),
    );

    if let Some(ptx_path) = launch_smoke_ptx {
        let ptx = std::fs::read(&ptx_path)
            .with_context(|| format!("failed to read CUDA smoke PTX {}", ptx_path.display()))?;
        let smoke = run_cuda_driver_launch_smoke(&ptx, Some(selected.index))?;
        println!(
            "cuda_driver_context=PASS cuda_module_load=PASS cuda_h2d_copy=PASS cuda_kernel_launch=PASS cuda_d2h_copy=PASS selected_index={} input={:#018x} output={:#018x} kheavyhash_equivalence=NOT_CLAIMED GPU_MINING_NVIDIA_PASS=NOT_CLAIMED",
            smoke.device_index, smoke.input, smoke.output,
        );
    }

    Ok(())
}
