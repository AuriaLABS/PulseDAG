use crate::accelerator_scheduler::AcceleratorDeviceKey;
use anyhow::{anyhow, ensure, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AcceleratorDeviceTelemetry {
    pub available: bool,
    pub dispatches: u64,
    pub completions: u64,
    pub replay_dispatches: u64,
    pub rejected_completions: u64,
    pub failures: u64,
    pub recoveries: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AcceleratorAggregateTelemetry {
    pub inventory_devices: u64,
    pub available_devices: u64,
    pub dispatches: u64,
    pub completions: u64,
    pub replay_dispatches: u64,
    pub rejected_completions: u64,
    pub failures: u64,
    pub recoveries: u64,
    pub job_refreshes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorTelemetry {
    devices: BTreeMap<AcceleratorDeviceKey, AcceleratorDeviceTelemetry>,
    job_refreshes: u64,
}

impl AcceleratorTelemetry {
    pub fn build(devices: &[AcceleratorDeviceKey]) -> Result<Self> {
        ensure!(
            !devices.is_empty(),
            "accelerator telemetry requires at least one device"
        );

        let mut ordered = devices.to_vec();
        ordered.sort_unstable();
        ensure!(
            !ordered.windows(2).any(|pair| pair[0] == pair[1]),
            "accelerator telemetry contains a duplicate backend/device identity"
        );

        let devices = ordered
            .into_iter()
            .map(|device| {
                (
                    device,
                    AcceleratorDeviceTelemetry {
                        available: true,
                        ..AcceleratorDeviceTelemetry::default()
                    },
                )
            })
            .collect();

        Ok(Self {
            devices,
            job_refreshes: 0,
        })
    }

    pub fn devices(
        &self,
    ) -> impl Iterator<Item = (&AcceleratorDeviceKey, &AcceleratorDeviceTelemetry)> {
        self.devices.iter()
    }

    pub fn device(&self, device: AcceleratorDeviceKey) -> Result<&AcceleratorDeviceTelemetry> {
        self.devices.get(&device).ok_or_else(|| {
            anyhow!(
                "accelerator telemetry device {:?}[{}] is not in the inventory",
                device.backend,
                device.device_index
            )
        })
    }

    fn device_mut(
        &mut self,
        device: AcceleratorDeviceKey,
    ) -> Result<&mut AcceleratorDeviceTelemetry> {
        self.devices.get_mut(&device).ok_or_else(|| {
            anyhow!(
                "accelerator telemetry device {:?}[{}] is not in the inventory",
                device.backend,
                device.device_index
            )
        })
    }

    pub fn record_dispatch(&mut self, device: AcceleratorDeviceKey, replay: bool) -> Result<()> {
        let state = self.device_mut(device)?;
        ensure!(
            state.available,
            "cannot record dispatch for unavailable accelerator device {:?}[{}]",
            device.backend,
            device.device_index
        );
        let dispatches = state
            .dispatches
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator dispatch counter overflow"))?;
        let replay_dispatches = if replay {
            state
                .replay_dispatches
                .checked_add(1)
                .ok_or_else(|| anyhow!("accelerator replay dispatch counter overflow"))?
        } else {
            state.replay_dispatches
        };
        state.dispatches = dispatches;
        state.replay_dispatches = replay_dispatches;
        Ok(())
    }

    pub fn record_completion(&mut self, device: AcceleratorDeviceKey) -> Result<()> {
        let state = self.device_mut(device)?;
        let completions = state
            .completions
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator completion counter overflow"))?;
        state.completions = completions;
        Ok(())
    }

    pub fn record_rejected_completion(&mut self, device: AcceleratorDeviceKey) -> Result<()> {
        let state = self.device_mut(device)?;
        let rejected = state
            .rejected_completions
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator rejected-completion counter overflow"))?;
        state.rejected_completions = rejected;
        Ok(())
    }

    pub fn mark_failed(&mut self, device: AcceleratorDeviceKey) -> Result<()> {
        let state = self.device_mut(device)?;
        ensure!(
            state.available,
            "accelerator telemetry device {:?}[{}] is already unavailable",
            device.backend,
            device.device_index
        );
        let failures = state
            .failures
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator failure counter overflow"))?;
        state.failures = failures;
        state.available = false;
        Ok(())
    }

    pub fn mark_recovered(&mut self, device: AcceleratorDeviceKey) -> Result<()> {
        let state = self.device_mut(device)?;
        ensure!(
            !state.available,
            "accelerator telemetry device {:?}[{}] is already available",
            device.backend,
            device.device_index
        );
        let recoveries = state
            .recoveries
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator recovery counter overflow"))?;
        state.recoveries = recoveries;
        state.available = true;
        Ok(())
    }

    pub fn record_job_refresh(&mut self) -> Result<()> {
        let next = self
            .job_refreshes
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator job-refresh counter overflow"))?;
        self.job_refreshes = next;
        Ok(())
    }

    pub fn aggregate(&self) -> Result<AcceleratorAggregateTelemetry> {
        let mut aggregate = AcceleratorAggregateTelemetry {
            inventory_devices: u64::try_from(self.devices.len())
                .map_err(|_| anyhow!("accelerator inventory size does not fit u64"))?,
            job_refreshes: self.job_refreshes,
            ..AcceleratorAggregateTelemetry::default()
        };

        for state in self.devices.values() {
            if state.available {
                aggregate.available_devices =
                    checked_sum(aggregate.available_devices, 1, "available-device aggregate")?;
            }
            aggregate.dispatches = checked_sum(aggregate.dispatches, state.dispatches, "dispatch")?;
            aggregate.completions =
                checked_sum(aggregate.completions, state.completions, "completion")?;
            aggregate.replay_dispatches = checked_sum(
                aggregate.replay_dispatches,
                state.replay_dispatches,
                "replay-dispatch",
            )?;
            aggregate.rejected_completions = checked_sum(
                aggregate.rejected_completions,
                state.rejected_completions,
                "rejected-completion",
            )?;
            aggregate.failures = checked_sum(aggregate.failures, state.failures, "failure")?;
            aggregate.recoveries = checked_sum(aggregate.recoveries, state.recoveries, "recovery")?;
        }
        Ok(aggregate)
    }
}

fn checked_sum(current: u64, add: u64, label: &str) -> Result<u64> {
    current
        .checked_add(add)
        .ok_or_else(|| anyhow!("accelerator {label} aggregate overflow"))
}

fn verify_overflow_paths_fail_closed() -> Result<()> {
    let device = AcceleratorDeviceKey::cuda(0);

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    telemetry.device_mut(device)?.dispatches = u64::MAX;
    let before = telemetry.clone();
    let rejected = telemetry.record_dispatch(device, false).is_err();
    ensure!(rejected, "dispatch overflow was not rejected");
    ensure!(telemetry == before, "dispatch overflow mutated telemetry");

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    telemetry.device_mut(device)?.replay_dispatches = u64::MAX;
    let before = telemetry.clone();
    let rejected = telemetry.record_dispatch(device, true).is_err();
    ensure!(rejected, "replay-dispatch overflow was not rejected");
    ensure!(
        telemetry == before,
        "replay-dispatch overflow mutated telemetry"
    );

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    telemetry.device_mut(device)?.completions = u64::MAX;
    let before = telemetry.clone();
    let rejected = telemetry.record_completion(device).is_err();
    ensure!(rejected, "completion overflow was not rejected");
    ensure!(telemetry == before, "completion overflow mutated telemetry");

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    telemetry.device_mut(device)?.rejected_completions = u64::MAX;
    let before = telemetry.clone();
    let rejected = telemetry.record_rejected_completion(device).is_err();
    ensure!(rejected, "rejected-completion overflow was not rejected");
    ensure!(
        telemetry == before,
        "rejected-completion overflow mutated telemetry"
    );

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    telemetry.device_mut(device)?.failures = u64::MAX;
    let before = telemetry.clone();
    let rejected = telemetry.mark_failed(device).is_err();
    ensure!(rejected, "failure overflow was not rejected");
    ensure!(telemetry == before, "failure overflow mutated telemetry");

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    {
        let state = telemetry.device_mut(device)?;
        state.available = false;
        state.recoveries = u64::MAX;
    }
    let before = telemetry.clone();
    let rejected = telemetry.mark_recovered(device).is_err();
    ensure!(rejected, "recovery overflow was not rejected");
    ensure!(telemetry == before, "recovery overflow mutated telemetry");

    let mut telemetry = AcceleratorTelemetry::build(&[device])?;
    telemetry.job_refreshes = u64::MAX;
    let before = telemetry.clone();
    let rejected = telemetry.record_job_refresh().is_err();
    ensure!(rejected, "job-refresh overflow was not rejected");
    ensure!(
        telemetry == before,
        "job-refresh overflow mutated telemetry"
    );

    let cuda0 = AcceleratorDeviceKey::cuda(0);
    let cuda1 = AcceleratorDeviceKey::cuda(1);
    let mut telemetry = AcceleratorTelemetry::build(&[cuda0, cuda1])?;
    telemetry.device_mut(cuda0)?.dispatches = u64::MAX;
    telemetry.device_mut(cuda1)?.dispatches = 1;
    let before = telemetry.clone();
    let rejected = telemetry.aggregate().is_err();
    ensure!(rejected, "aggregate overflow was not rejected");
    ensure!(telemetry == before, "aggregate overflow mutated telemetry");

    Ok(())
}

pub fn run_telemetry_self_test() -> Result<()> {
    let cuda0 = AcceleratorDeviceKey::cuda(0);
    let cuda1 = AcceleratorDeviceKey::cuda(1);
    let opencl0 = AcceleratorDeviceKey::opencl(0);
    let mut telemetry = AcceleratorTelemetry::build(&[opencl0, cuda1, cuda0])?;

    telemetry.record_dispatch(cuda0, false)?;
    telemetry.record_completion(cuda0)?;
    telemetry.record_dispatch(cuda0, false)?;
    telemetry.mark_failed(cuda0)?;
    telemetry.record_rejected_completion(cuda0)?;
    telemetry.record_dispatch(cuda1, true)?;
    telemetry.record_completion(cuda1)?;
    telemetry.mark_recovered(cuda0)?;
    telemetry.record_job_refresh()?;

    let aggregate = telemetry.aggregate()?;
    ensure!(
        aggregate.inventory_devices == 3,
        "inventory aggregate mismatch"
    );
    ensure!(
        aggregate.available_devices == 3,
        "available aggregate mismatch"
    );
    ensure!(aggregate.dispatches == 3, "dispatch aggregate mismatch");
    ensure!(aggregate.completions == 2, "completion aggregate mismatch");
    ensure!(
        aggregate.replay_dispatches == 1,
        "replay aggregate mismatch"
    );
    ensure!(
        aggregate.rejected_completions == 1,
        "rejected completion aggregate mismatch"
    );
    ensure!(aggregate.failures == 1, "failure aggregate mismatch");
    ensure!(aggregate.recoveries == 1, "recovery aggregate mismatch");
    ensure!(aggregate.job_refreshes == 1, "refresh aggregate mismatch");

    let cuda0_state = telemetry.device(cuda0)?;
    ensure!(
        *cuda0_state
            == AcceleratorDeviceTelemetry {
                available: true,
                dispatches: 2,
                completions: 1,
                replay_dispatches: 0,
                rejected_completions: 1,
                failures: 1,
                recoveries: 1,
            },
        "cuda0 per-device telemetry mismatch"
    );
    let cuda1_state = telemetry.device(cuda1)?;
    ensure!(
        *cuda1_state
            == AcceleratorDeviceTelemetry {
                available: true,
                dispatches: 1,
                completions: 1,
                replay_dispatches: 1,
                rejected_completions: 0,
                failures: 0,
                recoveries: 0,
            },
        "cuda1 per-device telemetry mismatch"
    );
    let opencl0_state = telemetry.device(opencl0)?;
    ensure!(
        *opencl0_state
            == AcceleratorDeviceTelemetry {
                available: true,
                dispatches: 0,
                completions: 0,
                replay_dispatches: 0,
                rejected_completions: 0,
                failures: 0,
                recoveries: 0,
            },
        "opencl0 per-device telemetry mismatch"
    );

    let ordered = telemetry
        .devices()
        .map(|(device, _)| *device)
        .collect::<Vec<_>>();
    ensure!(
        ordered == vec![cuda0, cuda1, opencl0],
        "telemetry device ordering is not canonical"
    );

    verify_overflow_paths_fail_closed()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_is_canonical_and_rejects_duplicates() {
        let cuda0 = AcceleratorDeviceKey::cuda(0);
        let opencl0 = AcceleratorDeviceKey::opencl(0);
        let telemetry = AcceleratorTelemetry::build(&[opencl0, cuda0]).unwrap();
        assert_eq!(
            telemetry
                .devices()
                .map(|(device, _)| *device)
                .collect::<Vec<_>>(),
            vec![cuda0, opencl0]
        );
        assert!(AcceleratorTelemetry::build(&[cuda0, cuda0]).is_err());
        assert!(AcceleratorTelemetry::build(&[]).is_err());
    }

    #[test]
    fn aggregate_is_exact_sum_of_per_device_counters() {
        let cuda0 = AcceleratorDeviceKey::cuda(0);
        let opencl0 = AcceleratorDeviceKey::opencl(0);
        let mut telemetry = AcceleratorTelemetry::build(&[cuda0, opencl0]).unwrap();
        telemetry.record_dispatch(cuda0, false).unwrap();
        telemetry.record_completion(cuda0).unwrap();
        telemetry.record_dispatch(opencl0, true).unwrap();
        telemetry.record_rejected_completion(opencl0).unwrap();
        telemetry.record_job_refresh().unwrap();

        let aggregate = telemetry.aggregate().unwrap();
        assert_eq!(aggregate.inventory_devices, 2);
        assert_eq!(aggregate.available_devices, 2);
        assert_eq!(aggregate.dispatches, 2);
        assert_eq!(aggregate.completions, 1);
        assert_eq!(aggregate.replay_dispatches, 1);
        assert_eq!(aggregate.rejected_completions, 1);
        assert_eq!(aggregate.job_refreshes, 1);
    }

    #[test]
    fn health_transitions_are_fail_closed() {
        let device = AcceleratorDeviceKey::cuda(0);
        let mut telemetry = AcceleratorTelemetry::build(&[device]).unwrap();
        telemetry.mark_failed(device).unwrap();
        let snapshot = telemetry.clone();
        assert!(telemetry.mark_failed(device).is_err());
        assert_eq!(telemetry, snapshot);
        assert!(telemetry.record_dispatch(device, false).is_err());
        assert_eq!(telemetry, snapshot);
        telemetry.mark_recovered(device).unwrap();
        let recovered = telemetry.clone();
        assert!(telemetry.mark_recovered(device).is_err());
        assert_eq!(telemetry, recovered);
    }

    #[test]
    fn unknown_device_does_not_mutate_telemetry() {
        let known = AcceleratorDeviceKey::cuda(0);
        let unknown = AcceleratorDeviceKey::cuda(1);
        let mut telemetry = AcceleratorTelemetry::build(&[known]).unwrap();
        let before = telemetry.clone();
        assert!(telemetry.record_dispatch(unknown, false).is_err());
        assert_eq!(telemetry, before);
    }

    #[test]
    fn dispatch_counter_overflow_fails_before_mutation() {
        let device = AcceleratorDeviceKey::cuda(0);
        let mut telemetry = AcceleratorTelemetry::build(&[device]).unwrap();
        telemetry.devices.get_mut(&device).unwrap().dispatches = u64::MAX;
        let before = telemetry.clone();
        assert!(telemetry.record_dispatch(device, false).is_err());
        assert_eq!(telemetry, before);
    }

    #[test]
    fn aggregate_overflow_is_reported_without_mutation() {
        let cuda0 = AcceleratorDeviceKey::cuda(0);
        let cuda1 = AcceleratorDeviceKey::cuda(1);
        let mut telemetry = AcceleratorTelemetry::build(&[cuda0, cuda1]).unwrap();
        telemetry.devices.get_mut(&cuda0).unwrap().dispatches = u64::MAX;
        telemetry.devices.get_mut(&cuda1).unwrap().dispatches = 1;
        let before = telemetry.clone();
        assert!(telemetry.aggregate().is_err());
        assert_eq!(telemetry, before);
    }

    #[test]
    fn all_overflow_paths_are_exercised_by_self_test() {
        verify_overflow_paths_fail_closed().unwrap();
    }

    #[test]
    fn software_only_self_test_passes() {
        run_telemetry_self_test().unwrap();
    }
}
