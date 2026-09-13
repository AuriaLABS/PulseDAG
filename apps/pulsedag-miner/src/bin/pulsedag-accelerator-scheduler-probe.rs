#[path = "../accelerator_scheduler.rs"]
mod accelerator_scheduler;

use accelerator_scheduler::run_scheduler_self_test;
use anyhow::{anyhow, Result};

fn main() -> Result<()> {
    let mut self_test = false;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-test" => self_test = true,
            "--help" | "-h" => {
                println!(
                    "usage: pulsedag-accelerator-scheduler-probe --self-test\n\nRuns a deterministic software-only accelerator scheduling contract over canonical protocol nonce partitions. It does not execute CUDA/OpenCL kernels or claim GPU mining equivalence."
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if !self_test {
        return Err(anyhow!(
            "no accelerator runtime action requested; use --self-test"
        ));
    }

    run_scheduler_self_test()?;
    println!(
        "accelerator_scheduler_selftest=PASS canonical_nonce_partition=PASS deterministic_device_assignment=PASS failed_worker_redistribution=PASS job_refresh=PASS hardware_execution=NOT_CLAIMED GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED"
    );
    Ok(())
}
