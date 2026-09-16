#[path = "../accelerator_reconnect.rs"]
mod accelerator_reconnect;
#[allow(dead_code)]
#[path = "../accelerator_scheduler.rs"]
mod accelerator_scheduler;

use accelerator_reconnect::run_reconnect_self_test;
use anyhow::{anyhow, Result};

fn main() -> Result<()> {
    let mut self_test = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--self-test" => self_test = true,
            "--help" | "-h" => {
                println!(
                    "usage: pulsedag-accelerator-reconnect-probe --self-test\n\nRuns a deterministic software-only reconnect/job-refresh/work-redistribution contract. It does not wire the standalone miner loop, open network sockets, execute CUDA/OpenCL kernels, or claim physical GPU validation."
                );
                return Ok(());
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if !self_test {
        return Err(anyhow!(
            "no reconnect control action requested; use --self-test"
        ));
    }

    run_reconnect_self_test()?;
    println!(
        "accelerator_reconnect_contract=PASS job_refresh_generation=PASS failed_worker_redistribution=PASS stale_completion_rejection=PASS health_state_preservation=PASS deterministic_recovery_rejoin=PASS miner_loop_integration=NOT_CLAIMED network_transport_execution=NOT_CLAIMED physical_gpu_execution=NOT_CLAIMED GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED"
    );
    Ok(())
}
