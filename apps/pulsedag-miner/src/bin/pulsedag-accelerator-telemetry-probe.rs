#[allow(dead_code)]
#[path = "../accelerator_scheduler.rs"]
mod accelerator_scheduler;
#[path = "../accelerator_telemetry.rs"]
mod accelerator_telemetry;

use accelerator_telemetry::run_telemetry_self_test;
use anyhow::{anyhow, Result};

fn main() -> Result<()> {
    let mut self_test = false;
    let args = std::env::args().skip(1);

    for arg in args {
        match arg.as_str() {
            "--self-test" => self_test = true,
            "--help" | "-h" => {
                println!(
                    "usage: pulsedag-accelerator-telemetry-probe --self-test\n\nRuns deterministic software-only accelerator telemetry accounting checks. It does not execute CUDA/OpenCL kernels, measure physical hashrate, read vendor sensors, or claim GPU mining equivalence."
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if !self_test {
        return Err(anyhow!(
            "no accelerator telemetry action requested; use --self-test"
        ));
    }

    run_telemetry_self_test()?;
    println!(
        "accelerator_telemetry_selftest=PASS canonical_device_order=PASS per_device_accounting=PASS aggregate_accounting=PASS health_transition_accounting=PASS overflow_fail_closed=PASS physical_hashrate=NOT_CLAIMED vendor_sensor_telemetry=NOT_CLAIMED hardware_execution=NOT_CLAIMED GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED"
    );
    Ok(())
}
