#[path = "../cuda_runtime.rs"]
mod cuda_runtime;

use anyhow::{anyhow, Context, Result};
use cuda_runtime::{discover_cuda_devices, run_selection_self_test, select_cuda_device};

fn main() -> Result<()> {
    let mut self_test = false;
    let mut device_index = None;
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
            "--help" | "-h" => {
                println!(
                    "usage: pulsedag-cuda-probe [--self-test] [--device INDEX]\n\nWithout --self-test, the probe loads the NVIDIA CUDA Driver API, enumerates devices, and selects device 0 or --device INDEX. This validates runtime discovery only; it does not establish mining equivalence or GPU_MINING_NVIDIA_PASS."
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if self_test {
        if device_index.is_some() {
            return Err(anyhow!("--device cannot be combined with --self-test"));
        }
        run_selection_self_test()?;
        println!("cuda_runtime_selection_selftest=PASS hardware_execution=NOT_CLAIMED mining_equivalence=NOT_CLAIMED");
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
    Ok(())
}
