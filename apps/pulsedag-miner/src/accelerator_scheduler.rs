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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceleratorDispatchTicket {
    pub generation: u64,
    pub lane_index: usize,
    pub owner: AcceleratorDeviceKey,
    pub nonce: u64,
    pub dispatch_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorLaneState {
    pub lane_index: usize,
    pub owner: AcceleratorDeviceKey,
    pub partition: ProtocolNoncePartition,
    pub next_iteration: u64,
    pub in_flight: Option<AcceleratorDispatchTicket>,
    next_dispatch_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorSchedule {
    devices: Vec<AcceleratorDeviceKey>,
    unavailable_devices: BTreeSet<AcceleratorDeviceKey>,
    lanes: Vec<AcceleratorLaneState>,
    max_tries: u64,
    generation: u64,
}

impl AcceleratorSchedule {
    pub fn build(devices: &[AcceleratorDeviceKey], max_tries: u64) -> Result<Self> {
        Self::build_with_health(devices, BTreeSet::new(), max_tries, 0)
    }

    fn build_with_health(
        devices: &[AcceleratorDeviceKey],
        unavailable_devices: BTreeSet<AcceleratorDeviceKey>,
        max_tries: u64,
        generation: u64,
    ) -> Result<Self> {
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
        if let Some(unknown) = unavailable_devices
            .iter()
            .find(|device| !devices.contains(device))
        {
            return Err(anyhow!(
                "unavailable accelerator device {:?}[{}] is not in the device inventory",
                unknown.backend,
                unknown.device_index
            ));
        }

        let healthy = devices
            .iter()
            .copied()
            .filter(|device| !unavailable_devices.contains(device))
            .collect::<Vec<_>>();
        ensure!(
            !healthy.is_empty(),
            "accelerator schedule requires at least one healthy device"
        );

        let max_tries = max_tries.max(1);
        let active_lanes = protocol_nonce_lane_count(max_tries, healthy.len());
        let mut lanes = Vec::with_capacity(active_lanes);
        for (lane_index, owner) in healthy.iter().copied().take(active_lanes).enumerate() {
            lanes.push(AcceleratorLaneState {
                lane_index,
                owner,
                partition: protocol_nonce_partition(max_tries, active_lanes, lane_index)?,
                next_iteration: 0,
                in_flight: None,
                next_dispatch_epoch: 0,
            });
        }

        Ok(Self {
            devices,
            unavailable_devices,
            lanes,
            max_tries,
            generation,
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

    pub fn generation(&self) -> u64 {
        self.generation
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

    pub fn dispatch_lane(
        &mut self,
        lane_index: usize,
    ) -> Result<Option<AcceleratorDispatchTicket>> {
        let generation = self.generation;
        let lane = self
            .lanes
            .get_mut(lane_index)
            .ok_or_else(|| anyhow!("accelerator lane {lane_index} does not exist"))?;
        ensure!(
            lane.in_flight.is_none(),
            "accelerator lane {lane_index} already has in-flight work"
        );

        let Some(nonce) = lane.partition.nonce_at(lane.next_iteration)? else {
            return Ok(None);
        };
        let dispatch_epoch = lane.next_dispatch_epoch;
        let next_dispatch_epoch = dispatch_epoch.checked_add(1).ok_or_else(|| {
            anyhow!(
                "accelerator lane {} dispatch epoch overflow",
                lane.lane_index
            )
        })?;
        let ticket = AcceleratorDispatchTicket {
            generation,
            lane_index,
            owner: lane.owner,
            nonce,
            dispatch_epoch,
        };
        lane.next_dispatch_epoch = next_dispatch_epoch;
        lane.in_flight = Some(ticket);
        Ok(Some(ticket))
    }

    pub fn complete_lane(&mut self, ticket: AcceleratorDispatchTicket) -> Result<()> {
        ensure!(
            ticket.generation == self.generation,
            "stale accelerator completion generation {} does not match current generation {}",
            ticket.generation,
            self.generation
        );
        let lane = self
            .lanes
            .get_mut(ticket.lane_index)
            .ok_or_else(|| anyhow!("accelerator lane {} does not exist", ticket.lane_index))?;
        ensure!(
            lane.in_flight == Some(ticket),
            "stale or mismatched accelerator completion for lane {}",
            ticket.lane_index
        );

        let next_iteration = lane
            .next_iteration
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator lane {} iteration overflow", lane.lane_index))?;
        lane.next_iteration = next_iteration;
        lane.in_flight = None;
        Ok(())
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
            lane.in_flight = None;
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
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow!("accelerator schedule generation overflow"))?;
        Self::build_with_health(
            &self.devices,
            self.unavailable_devices.clone(),
            max_tries,
            generation,
        )
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
        schedule.generation() == 0,
        "accelerator generation mismatch"
    );
    ensure!(
        schedule.unavailable_devices().is_empty(),
        "fresh accelerator schedule unexpectedly has unavailable devices"
    );
    ensure!(
        schedule.lanes().len() == 3,
        "unexpected accelerator lane count"
    );

    let first = schedule
        .dispatch_lane(0)?
        .ok_or_else(|| anyhow!("lane 0 unexpectedly exhausted"))?;
    ensure!(first.nonce == 0, "lane 0 first dispatch mismatch");
    schedule.complete_lane(first)?;
    let interrupted = schedule
        .dispatch_lane(0)?
        .ok_or_else(|| anyhow!("lane 0 unexpectedly exhausted after first completion"))?;
    ensure!(interrupted.nonce == 3, "lane 0 second dispatch mismatch");

    let failed_owner = interrupted.owner;
    let mut redistributed = schedule.redistribute_failed_device(failed_owner)?;
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
        "redistribution lost completed lane progress"
    );
    ensure!(
        redistributed.lanes()[0].in_flight.is_none(),
        "redistribution retained failed in-flight work"
    );
    ensure!(
        redistributed.lanes()[0].owner != failed_owner,
        "failed device retained lane ownership"
    );

    let replacement = redistributed
        .dispatch_lane(0)?
        .ok_or_else(|| anyhow!("redistributed lane unexpectedly exhausted"))?;
    ensure!(
        replacement.nonce == interrupted.nonce,
        "redistribution did not replay interrupted nonce"
    );
    ensure!(
        redistributed.complete_lane(interrupted).is_err(),
        "stale failed-owner completion was accepted"
    );
    ensure!(
        redistributed.lanes()[0].in_flight == Some(replacement),
        "stale completion disturbed replacement work"
    );
    redistributed.complete_lane(replacement)?;

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
        refreshed.generation() == redistributed.generation() + 1,
        "job refresh did not advance scheduler generation"
    );
    ensure!(
        refreshed.devices().contains(&failed_owner),
        "job refresh lost failed device inventory"
    );
    ensure!(
        refreshed.unavailable_devices().contains(&failed_owner),
        "job refresh lost failed device health state"
    );
    ensure!(
        refreshed
            .lanes()
            .iter()
            .all(|lane| lane.next_iteration == 0 && lane.in_flight.is_none()),
        "job refresh did not reset fresh-job lane state"
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
        let mut seen = [0u8; 19];
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
    fn exact_dispatch_completes_once_and_advances_exactly_once() {
        let device = AcceleratorDeviceKey::cuda(0);
        let mut schedule = AcceleratorSchedule::build(&[device], 4).unwrap();
        let first = schedule.dispatch_lane(0).unwrap().unwrap();
        assert_eq!(first.nonce, 0);
        assert_eq!(schedule.lanes()[0].next_iteration, 0);
        assert!(schedule.dispatch_lane(0).is_err());
        schedule.complete_lane(first).unwrap();
        assert_eq!(schedule.lanes()[0].next_iteration, 1);
        assert!(schedule.complete_lane(first).is_err());
        assert_eq!(schedule.lanes()[0].next_iteration, 1);

        let second = schedule.dispatch_lane(0).unwrap().unwrap();
        assert_eq!(second.nonce, 1);
        assert_ne!(second.dispatch_epoch, first.dispatch_epoch);
    }

    #[test]
    fn stale_failed_owner_completion_is_rejected_after_replacement_redispatch() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::cuda(1);
        let mut schedule = AcceleratorSchedule::build(&[failed, survivor], 11).unwrap();

        let first = schedule.dispatch_lane(0).unwrap().unwrap();
        assert_eq!(first.nonce, 0);
        schedule.complete_lane(first).unwrap();
        let stale = schedule.dispatch_lane(0).unwrap().unwrap();
        assert_eq!(stale.nonce, 2);

        let mut redistributed = schedule.redistribute_failed_device(failed).unwrap();
        let replacement = redistributed.dispatch_lane(0).unwrap().unwrap();
        assert_eq!(replacement.nonce, stale.nonce);
        assert_eq!(replacement.owner, survivor);
        assert_ne!(replacement.dispatch_epoch, stale.dispatch_epoch);

        let before_iteration = redistributed.lanes()[0].next_iteration;
        assert!(redistributed.complete_lane(stale).is_err());
        assert_eq!(redistributed.lanes()[0].next_iteration, before_iteration);
        assert_eq!(redistributed.lanes()[0].in_flight, Some(replacement));

        redistributed.complete_lane(replacement).unwrap();
        assert_eq!(
            redistributed.lanes()[0].next_iteration,
            before_iteration + 1
        );
        let next = redistributed.dispatch_lane(0).unwrap().unwrap();
        assert_eq!(next.nonce, 4);
    }

    #[test]
    fn stale_previous_generation_completion_is_rejected() {
        let device = AcceleratorDeviceKey::cuda(0);
        let mut schedule = AcceleratorSchedule::build(&[device], 5).unwrap();
        let stale = schedule.dispatch_lane(0).unwrap().unwrap();

        let mut refreshed = schedule.refresh_job(5).unwrap();
        let current = refreshed.dispatch_lane(0).unwrap().unwrap();
        assert_ne!(current.generation, stale.generation);
        assert!(refreshed.complete_lane(stale).is_err());
        assert_eq!(refreshed.lanes()[0].next_iteration, 0);
        assert_eq!(refreshed.lanes()[0].in_flight, Some(current));
        refreshed.complete_lane(current).unwrap();
        assert_eq!(refreshed.lanes()[0].next_iteration, 1);
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
    fn refreshed_job_preserves_inventory_and_health_then_reuses_recovered_device() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::opencl(0);
        let schedule = AcceleratorSchedule::build(&[failed, survivor], 9).unwrap();
        let redistributed = schedule.redistribute_failed_device(failed).unwrap();
        let refreshed = redistributed.refresh_job(5).unwrap();
        assert_eq!(refreshed.devices(), &[failed, survivor]);
        assert!(refreshed.unavailable_devices().contains(&failed));
        assert!(refreshed.lanes().iter().all(|lane| lane.owner == survivor));

        let recovered = refreshed.recover_device(failed).unwrap();
        let next_job = recovered.refresh_job(5).unwrap();
        assert!(next_job.unavailable_devices().is_empty());
        assert_eq!(next_job.lanes().len(), 2);
        assert_eq!(next_job.lanes()[0].owner, failed);
        assert_eq!(next_job.lanes()[1].owner, survivor);
    }
}
