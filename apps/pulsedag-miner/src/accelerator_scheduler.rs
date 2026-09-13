use anyhow::{anyhow, ensure, Result};
use pulsedag_miner::protocol_backend::{
    protocol_nonce_lane_count, protocol_nonce_partition, ProtocolNoncePartition,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AcceleratorBackendKind {
    Cuda,
    OpenCl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AcceleratorDeviceKey {
    pub backend: AcceleratorBackendKind,
    pub device_index: usize,
}

impl AcceleratorDeviceKey {
    pub const fn cuda(device_index: usize) -> Self {
        Self {
            backend: AcceleratorBackendKind::Cuda,
            device_index,
        }
    }

    pub const fn opencl(device_index: usize) -> Self {
        Self {
            backend: AcceleratorBackendKind::OpenCl,
            device_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorLaneState {
    pub lane_index: usize,
    pub owner: AcceleratorDeviceKey,
    pub partition: ProtocolNoncePartition,
    pub next_iteration: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorSchedule {
    devices: Vec<AcceleratorDeviceKey>,
    unavailable_devices: BTreeSet<AcceleratorDeviceKey>,
    lanes: Vec<AcceleratorLaneState>,
    max_tries: u64,
}

impl AcceleratorSchedule {
    pub fn build(devices: &[AcceleratorDeviceKey], max_tries: u64) -> Result<Self> {
        if devices.is_empty() {
            return Err(anyhow!("accelerator schedule requires at least one device"));
        }

        let mut devices = devices.to_vec();
        devices.sort_unstable();
        if devices.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(anyhow!(
                "accelerator schedule contains a duplicate backend/device identity"
            ));
        }

        let max_tries = max_tries.max(1);
        let active_lanes = protocol_nonce_lane_count(max_tries, devices.len());
        let mut lanes = Vec::with_capacity(active_lanes);
        for (lane_index, owner) in devices.iter().copied().take(active_lanes).enumerate() {
            lanes.push(AcceleratorLaneState {
                lane_index,
                owner,
                partition: protocol_nonce_partition(max_tries, active_lanes, lane_index)?,
                next_iteration: 0,
            });
        }

        Ok(Self {
            devices,
            unavailable_devices: BTreeSet::new(),
            lanes,
            max_tries,
        })
    }

    pub fn devices(&self) -> &[AcceleratorDeviceKey] {
        &self.devices
    }

    pub fn lanes(&self) -> &[AcceleratorLaneState] {
        &self.lanes
    }

    pub fn max_tries(&self) -> u64 {
        self.max_tries
    }

    pub fn available_devices(&self) -> Vec<AcceleratorDeviceKey> {
        self.devices
            .iter()
            .copied()
            .filter(|device| !self.unavailable_devices.contains(device))
            .collect()
    }

    pub fn unavailable_devices(&self) -> &BTreeSet<AcceleratorDeviceKey> {
        &self.unavailable_devices
    }

    pub fn advance_lane(&mut self, lane_index: usize) -> Result<Option<u64>> {
        let lane = self
            .lanes
            .get_mut(lane_index)
            .ok_or_else(|| anyhow!("accelerator lane {lane_index} does not exist"))?;
        let nonce = lane.partition.nonce_at(lane.next_iteration)?;
        if nonce.is_some() {
            lane.next_iteration = lane.next_iteration.checked_add(1).ok_or_else(|| {
                anyhow!("accelerator lane {} iteration overflow", lane.lane_index)
            })?;
        }
        Ok(nonce)
    }

    pub fn redistribute_failed_device(&self, failed: AcceleratorDeviceKey) -> Result<Self> {
        ensure!(
            self.devices.contains(&failed),
            "accelerator device {:?}[{}] is not part of this schedule",
            failed.backend,
            failed.device_index
        );
        ensure!(
            !self.unavailable_devices.contains(&failed),
            "accelerator device {:?}[{}] is already unavailable",
            failed.backend,
            failed.device_index
        );

        let healthy = self
            .devices
            .iter()
            .copied()
            .filter(|device| *device != failed && !self.unavailable_devices.contains(device))
            .collect::<Vec<_>>();
        ensure!(
            !healthy.is_empty(),
            "cannot redistribute accelerator work: no healthy device remains"
        );

        let mut next = self.clone();
        next.unavailable_devices.insert(failed);

        let mut lane_counts = healthy
            .iter()
            .copied()
            .map(|device| (device, 0usize))
            .collect::<BTreeMap<_, _>>();
        for lane in &next.lanes {
            if let Some(count) = lane_counts.get_mut(&lane.owner) {
                *count = count.saturating_add(1);
            }
        }

        for lane in next.lanes.iter_mut().filter(|lane| lane.owner == failed) {
            let replacement = lane_counts
                .iter()
                .min_by_key(|(device, count)| (**count, **device))
                .map(|(device, _)| *device)
                .ok_or_else(|| anyhow!("no deterministic replacement device available"))?;
            lane.owner = replacement;
            *lane_counts
                .get_mut(&replacement)
                .expect("replacement device exists in lane count map") += 1;
        }

        Ok(next)
    }

    pub fn recover_device(&self, device: AcceleratorDeviceKey) -> Result<Self> {
        ensure!(
            self.devices.contains(&device),
            "accelerator device {:?}[{}] is not part of this schedule",
            device.backend,
            device.device_index
        );
        ensure!(
            self.unavailable_devices.contains(&device),
            "accelerator device {:?}[{}] is not unavailable",
            device.backend,
            device.device_index
        );

        let mut next = self.clone();
        next.unavailable_devices.remove(&device);
        Ok(next)
    }

    pub fn refresh_job(&self, max_tries: u64) -> Result<Self> {
        let available = self.available_devices();
        ensure!(
            !available.is_empty(),
            "cannot refresh accelerator job: no healthy device remains"
        );
        Self::build(&available, max_tries)
    }
}

pub fn run_scheduler_self_test() -> Result<()> {
    let devices = [
        AcceleratorDeviceKey::opencl(1),
        AcceleratorDeviceKey::cuda(1),
        AcceleratorDeviceKey::cuda(0),
    ];
    let mut schedule = AcceleratorSchedule::build(&devices, 17)?;
    ensure!(schedule.max_tries() == 17, "accelerator max_tries mismatch");
    ensure!(
        schedule.unavailable_devices().is_empty(),
        "fresh accelerator schedule unexpectedly has unavailable devices"
    );
    ensure!(
        schedule.lanes().len() == 3,
        "unexpected accelerator lane count"
    );
    ensure!(
        schedule.advance_lane(0)? == Some(0),
        "lane 0 first nonce mismatch"
    );
    ensure!(
        schedule.advance_lane(0)? == Some(3),
        "lane 0 second nonce mismatch"
    );

    let failed_owner = schedule.lanes()[0].owner;
    let redistributed = schedule.redistribute_failed_device(failed_owner)?;
    ensure!(
        redistributed.unavailable_devices().contains(&failed_owner),
        "failed accelerator device was not marked unavailable"
    );
    ensure!(
        redistributed.available_devices().len() == 2,
        "unexpected healthy accelerator device count after failure"
    );
    ensure!(
        redistributed.lanes()[0].partition == schedule.lanes()[0].partition,
        "redistribution changed the canonical nonce partition"
    );
    ensure!(
        redistributed.lanes()[0].next_iteration == schedule.lanes()[0].next_iteration,
        "redistribution lost lane progress"
    );
    ensure!(
        redistributed.lanes()[0].owner != failed_owner,
        "failed device retained lane ownership"
    );

    let recovered = redistributed.recover_device(failed_owner)?;
    ensure!(
        recovered.unavailable_devices().is_empty(),
        "recovered accelerator device remained unavailable"
    );
    ensure!(
        recovered.lanes() == redistributed.lanes(),
        "device recovery unexpectedly churned active lane ownership"
    );

    let refreshed = redistributed.refresh_job(9)?;
    ensure!(
        refreshed
            .lanes()
            .iter()
            .all(|lane| lane.next_iteration == 0),
        "job refresh did not reset lane cursors"
    );
    ensure!(
        !refreshed.devices().contains(&failed_owner),
        "job refresh reused an unavailable device"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_partition(partition: ProtocolNoncePartition) -> Vec<u64> {
        let mut nonces = Vec::new();
        let mut iteration = 0u64;
        while let Some(nonce) = partition.nonce_at(iteration).unwrap() {
            nonces.push(nonce);
            iteration = iteration.checked_add(1).unwrap();
        }
        nonces
    }

    #[test]
    fn schedule_is_deterministic_across_discovery_order() {
        let a = [
            AcceleratorDeviceKey::opencl(2),
            AcceleratorDeviceKey::cuda(1),
            AcceleratorDeviceKey::cuda(0),
        ];
        let b = [a[2], a[0], a[1]];
        assert_eq!(
            AcceleratorSchedule::build(&a, 17).unwrap(),
            AcceleratorSchedule::build(&b, 17).unwrap()
        );
    }

    #[test]
    fn schedule_lanes_cover_nonce_domain_exactly_once() {
        let devices = [
            AcceleratorDeviceKey::cuda(0),
            AcceleratorDeviceKey::cuda(1),
            AcceleratorDeviceKey::opencl(0),
        ];
        let schedule = AcceleratorSchedule::build(&devices, 19).unwrap();
        let mut seen = vec![0u8; 19];
        for lane in schedule.lanes() {
            for nonce in collect_partition(lane.partition) {
                seen[usize::try_from(nonce).unwrap()] += 1;
            }
        }
        assert!(seen.iter().all(|count| *count == 1));
    }

    #[test]
    fn duplicate_device_identity_fails_closed() {
        let duplicate = AcceleratorDeviceKey::cuda(0);
        assert!(AcceleratorSchedule::build(&[duplicate, duplicate], 8).is_err());
        assert!(AcceleratorSchedule::build(&[], 8).is_err());
    }

    #[test]
    fn devices_are_canonicalized_by_backend_then_index() {
        let schedule = AcceleratorSchedule::build(
            &[
                AcceleratorDeviceKey::opencl(0),
                AcceleratorDeviceKey::cuda(3),
                AcceleratorDeviceKey::cuda(1),
            ],
            12,
        )
        .unwrap();
        assert_eq!(
            schedule.devices(),
            &[
                AcceleratorDeviceKey::cuda(1),
                AcceleratorDeviceKey::cuda(3),
                AcceleratorDeviceKey::opencl(0),
            ]
        );
    }

    #[test]
    fn more_devices_than_tries_caps_active_lanes() {
        let schedule = AcceleratorSchedule::build(
            &[
                AcceleratorDeviceKey::cuda(0),
                AcceleratorDeviceKey::cuda(1),
                AcceleratorDeviceKey::opencl(0),
            ],
            2,
        )
        .unwrap();
        assert_eq!(schedule.lanes().len(), 2);
        assert_eq!(collect_partition(schedule.lanes()[0].partition), vec![0]);
        assert_eq!(collect_partition(schedule.lanes()[1].partition), vec![1]);
    }

    #[test]
    fn redistribution_preserves_partition_and_cursor_without_duplicate_work() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::cuda(1);
        let mut schedule = AcceleratorSchedule::build(&[failed, survivor], 11).unwrap();

        assert_eq!(schedule.advance_lane(0).unwrap(), Some(0));
        assert_eq!(schedule.advance_lane(0).unwrap(), Some(2));
        let before = schedule.lanes()[0].clone();

        let mut redistributed = schedule.redistribute_failed_device(failed).unwrap();
        let after = redistributed.lanes()[0].clone();
        assert_eq!(after.partition, before.partition);
        assert_eq!(after.next_iteration, before.next_iteration);
        assert_eq!(after.owner, survivor);
        assert_eq!(redistributed.advance_lane(0).unwrap(), Some(4));
        assert_eq!(redistributed.advance_lane(0).unwrap(), Some(6));
    }

    #[test]
    fn redistribution_is_deterministic_and_balances_lane_ownership() {
        let devices = [
            AcceleratorDeviceKey::cuda(0),
            AcceleratorDeviceKey::cuda(1),
            AcceleratorDeviceKey::opencl(0),
        ];
        let schedule = AcceleratorSchedule::build(&devices, 30).unwrap();
        let first = schedule.redistribute_failed_device(devices[0]).unwrap();
        let second = schedule.redistribute_failed_device(devices[0]).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.lanes()[0].owner, devices[1]);
    }

    #[test]
    fn last_device_failure_fails_closed_without_mutating_plan() {
        let device = AcceleratorDeviceKey::cuda(0);
        let schedule = AcceleratorSchedule::build(&[device], 8).unwrap();
        assert!(schedule.redistribute_failed_device(device).is_err());
        assert!(schedule.unavailable_devices().is_empty());
        assert_eq!(schedule.lanes()[0].owner, device);
    }

    #[test]
    fn recovery_does_not_churn_existing_lane_ownership() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::cuda(1);
        let schedule = AcceleratorSchedule::build(&[failed, survivor], 8).unwrap();
        let redistributed = schedule.redistribute_failed_device(failed).unwrap();
        let recovered = redistributed.recover_device(failed).unwrap();
        assert!(recovered.unavailable_devices().is_empty());
        assert_eq!(recovered.lanes(), redistributed.lanes());
    }

    #[test]
    fn refreshed_job_uses_only_healthy_devices_and_resets_progress() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::opencl(0);
        let mut schedule = AcceleratorSchedule::build(&[failed, survivor], 9).unwrap();
        assert_eq!(schedule.advance_lane(1).unwrap(), Some(1));
        let redistributed = schedule.redistribute_failed_device(failed).unwrap();
        let refreshed = redistributed.refresh_job(5).unwrap();
        assert_eq!(refreshed.devices(), &[survivor]);
        assert_eq!(refreshed.lanes().len(), 1);
        assert_eq!(refreshed.lanes()[0].next_iteration, 0);
        assert_eq!(
            collect_partition(refreshed.lanes()[0].partition),
            vec![0, 1, 2, 3, 4]
        );
    }
}
