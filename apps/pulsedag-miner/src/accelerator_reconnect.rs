use crate::accelerator_scheduler::{
    AcceleratorDeviceKey, AcceleratorDispatchTicket, AcceleratorSchedule,
};
use anyhow::{anyhow, ensure, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorControlTransition {
    DeviceFailure { device: AcceleratorDeviceKey },
    DeviceRecovery { device: AcceleratorDeviceKey },
    JobRefresh { max_tries: u64 },
    Reconnect { max_tries: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorReconnectController {
    schedule: AcceleratorSchedule,
    reconnect_epoch: u64,
}

impl AcceleratorReconnectController {
    pub fn build(devices: &[AcceleratorDeviceKey], max_tries: u64) -> Result<Self> {
        Ok(Self {
            schedule: AcceleratorSchedule::build(devices, max_tries)?,
            reconnect_epoch: 0,
        })
    }

    pub fn schedule(&self) -> &AcceleratorSchedule {
        &self.schedule
    }

    pub fn reconnect_epoch(&self) -> u64 {
        self.reconnect_epoch
    }

    pub fn dispatch_lane(
        &mut self,
        lane_index: usize,
    ) -> Result<Option<AcceleratorDispatchTicket>> {
        self.schedule.dispatch_lane(lane_index)
    }

    pub fn complete_lane(&mut self, ticket: AcceleratorDispatchTicket) -> Result<()> {
        self.schedule.complete_lane(ticket)
    }

    pub fn apply(&self, transition: AcceleratorControlTransition) -> Result<Self> {
        match transition {
            AcceleratorControlTransition::DeviceFailure { device } => Ok(Self {
                schedule: self.schedule.redistribute_failed_device(device)?,
                reconnect_epoch: self.reconnect_epoch,
            }),
            AcceleratorControlTransition::DeviceRecovery { device } => Ok(Self {
                schedule: self.schedule.recover_device(device)?,
                reconnect_epoch: self.reconnect_epoch,
            }),
            AcceleratorControlTransition::JobRefresh { max_tries } => Ok(Self {
                schedule: self.schedule.refresh_job(max_tries)?,
                reconnect_epoch: self.reconnect_epoch,
            }),
            AcceleratorControlTransition::Reconnect { max_tries } => {
                let reconnect_epoch = self
                    .reconnect_epoch
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("accelerator reconnect epoch overflow"))?;
                Ok(Self {
                    schedule: self.schedule.refresh_job(max_tries)?,
                    reconnect_epoch,
                })
            }
        }
    }
}

pub fn run_reconnect_self_test() -> Result<()> {
    let devices = [
        AcceleratorDeviceKey::cuda(0),
        AcceleratorDeviceKey::cuda(1),
        AcceleratorDeviceKey::opencl(0),
    ];
    let mut controller = AcceleratorReconnectController::build(&devices, 17)?;

    let first = controller
        .dispatch_lane(0)?
        .ok_or_else(|| anyhow!("lane 0 unexpectedly exhausted"))?;
    controller.complete_lane(first)?;
    let interrupted = controller
        .dispatch_lane(0)?
        .ok_or_else(|| anyhow!("lane 0 unexpectedly exhausted after first completion"))?;
    let failed = interrupted.owner;
    let before_failure_iteration = controller.schedule().lanes()[0].next_iteration;

    let mut redistributed =
        controller.apply(AcceleratorControlTransition::DeviceFailure { device: failed })?;
    ensure!(
        redistributed
            .schedule()
            .unavailable_devices()
            .contains(&failed),
        "failed device did not remain unavailable after redistribution"
    );
    ensure!(
        redistributed.schedule().lanes()[0].next_iteration == before_failure_iteration,
        "redistribution lost completed work progress"
    );
    ensure!(
        redistributed.schedule().lanes()[0].in_flight.is_none(),
        "redistribution retained failed in-flight work"
    );

    let replacement = redistributed
        .dispatch_lane(0)?
        .ok_or_else(|| anyhow!("redistributed lane unexpectedly exhausted"))?;
    ensure!(
        replacement.nonce == interrupted.nonce,
        "redistribution did not replay the interrupted nonce exactly"
    );
    ensure!(
        replacement.owner != failed,
        "failed device retained ownership after redistribution"
    );
    ensure!(
        redistributed.complete_lane(interrupted).is_err(),
        "stale failed-owner completion was accepted"
    );
    ensure!(
        redistributed.schedule().lanes()[0].in_flight == Some(replacement),
        "stale completion disturbed replacement work"
    );
    redistributed.complete_lane(replacement)?;

    let stale_before_reconnect = redistributed
        .dispatch_lane(1)?
        .ok_or_else(|| anyhow!("lane 1 unexpectedly exhausted before reconnect"))?;
    let generation_before_reconnect = redistributed.schedule().generation();
    let reconnect_epoch_before = redistributed.reconnect_epoch();

    let mut reconnected =
        redistributed.apply(AcceleratorControlTransition::Reconnect { max_tries: 11 })?;
    ensure!(
        reconnected.schedule().generation() == generation_before_reconnect + 1,
        "reconnect did not advance scheduler generation exactly once"
    );
    ensure!(
        reconnected.reconnect_epoch() == reconnect_epoch_before + 1,
        "reconnect did not advance reconnect epoch exactly once"
    );
    ensure!(
        reconnected
            .schedule()
            .unavailable_devices()
            .contains(&failed),
        "reconnect lost failed-device health state"
    );
    ensure!(
        reconnected
            .schedule()
            .lanes()
            .iter()
            .all(|lane| lane.owner != failed
                && lane.next_iteration == 0
                && lane.in_flight.is_none()),
        "reconnect did not build clean fresh-job lanes on healthy devices"
    );
    ensure!(
        reconnected.complete_lane(stale_before_reconnect).is_err(),
        "pre-reconnect completion was accepted in the new generation"
    );

    let recovered =
        reconnected.apply(AcceleratorControlTransition::DeviceRecovery { device: failed })?;
    ensure!(
        recovered.schedule().unavailable_devices().is_empty(),
        "recovered device remained unavailable"
    );
    ensure!(
        recovered.schedule().lanes() == reconnected.schedule().lanes(),
        "device recovery churned active lane ownership"
    );

    let generation_before_refresh = recovered.schedule().generation();
    let reconnect_epoch_before_refresh = recovered.reconnect_epoch();
    let refreshed = recovered.apply(AcceleratorControlTransition::JobRefresh { max_tries: 9 })?;
    ensure!(
        refreshed.schedule().generation() == generation_before_refresh + 1,
        "job refresh did not advance scheduler generation exactly once"
    );
    ensure!(
        refreshed.reconnect_epoch() == reconnect_epoch_before_refresh,
        "ordinary job refresh incorrectly advanced reconnect epoch"
    );
    ensure!(
        refreshed
            .schedule()
            .lanes()
            .iter()
            .any(|lane| lane.owner == failed),
        "recovered device did not rejoin scheduling on fresh job"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_advances_generation_and_epoch_and_rejects_stale_completion() {
        let devices = [AcceleratorDeviceKey::cuda(0), AcceleratorDeviceKey::cuda(1)];
        let mut controller = AcceleratorReconnectController::build(&devices, 12).unwrap();
        let stale = controller.dispatch_lane(0).unwrap().unwrap();

        let generation = controller.schedule().generation();
        let epoch = controller.reconnect_epoch();
        let mut reconnected = controller
            .apply(AcceleratorControlTransition::Reconnect { max_tries: 12 })
            .unwrap();

        assert_eq!(reconnected.schedule().generation(), generation + 1);
        assert_eq!(reconnected.reconnect_epoch(), epoch + 1);
        assert!(reconnected.complete_lane(stale).is_err());
        assert!(reconnected
            .schedule()
            .lanes()
            .iter()
            .all(|lane| lane.next_iteration == 0 && lane.in_flight.is_none()));
    }

    #[test]
    fn failure_replays_interrupted_nonce_and_reconnect_preserves_health() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::opencl(0);
        let mut controller =
            AcceleratorReconnectController::build(&[failed, survivor], 10).unwrap();

        let stale = controller.dispatch_lane(0).unwrap().unwrap();
        let mut redistributed = controller
            .apply(AcceleratorControlTransition::DeviceFailure { device: failed })
            .unwrap();
        let replacement = redistributed.dispatch_lane(0).unwrap().unwrap();

        assert_eq!(replacement.nonce, stale.nonce);
        assert_eq!(replacement.owner, survivor);
        assert!(redistributed.complete_lane(stale).is_err());
        redistributed.complete_lane(replacement).unwrap();

        let reconnected = redistributed
            .apply(AcceleratorControlTransition::Reconnect { max_tries: 7 })
            .unwrap();
        assert!(reconnected
            .schedule()
            .unavailable_devices()
            .contains(&failed));
        assert!(reconnected
            .schedule()
            .lanes()
            .iter()
            .all(|lane| lane.owner == survivor));
    }

    #[test]
    fn recovery_rejoins_only_on_next_fresh_job_without_churning_current_lanes() {
        let failed = AcceleratorDeviceKey::cuda(0);
        let survivor = AcceleratorDeviceKey::cuda(1);
        let controller = AcceleratorReconnectController::build(&[failed, survivor], 8).unwrap();
        let redistributed = controller
            .apply(AcceleratorControlTransition::DeviceFailure { device: failed })
            .unwrap();
        let recovered = redistributed
            .apply(AcceleratorControlTransition::DeviceRecovery { device: failed })
            .unwrap();

        assert_eq!(
            recovered.schedule().lanes(),
            redistributed.schedule().lanes()
        );
        assert!(recovered.schedule().unavailable_devices().is_empty());

        let refreshed = recovered
            .apply(AcceleratorControlTransition::JobRefresh { max_tries: 8 })
            .unwrap();
        assert!(refreshed
            .schedule()
            .lanes()
            .iter()
            .any(|lane| lane.owner == failed));
    }

    #[test]
    fn job_refresh_and_reconnect_have_distinct_epoch_semantics() {
        let device = AcceleratorDeviceKey::cuda(0);
        let controller = AcceleratorReconnectController::build(&[device], 5).unwrap();
        let refreshed = controller
            .apply(AcceleratorControlTransition::JobRefresh { max_tries: 5 })
            .unwrap();
        assert_eq!(refreshed.reconnect_epoch(), 0);
        assert_eq!(refreshed.schedule().generation(), 1);

        let reconnected = refreshed
            .apply(AcceleratorControlTransition::Reconnect { max_tries: 5 })
            .unwrap();
        assert_eq!(reconnected.reconnect_epoch(), 1);
        assert_eq!(reconnected.schedule().generation(), 2);
    }

    #[test]
    fn last_device_failure_fails_closed_without_mutating_controller() {
        let device = AcceleratorDeviceKey::cuda(0);
        let controller = AcceleratorReconnectController::build(&[device], 5).unwrap();
        let before = controller.clone();
        assert!(controller
            .apply(AcceleratorControlTransition::DeviceFailure { device })
            .is_err());
        assert_eq!(controller, before);
    }

    #[test]
    fn transition_sequence_is_deterministic() {
        let devices = [
            AcceleratorDeviceKey::opencl(1),
            AcceleratorDeviceKey::cuda(1),
            AcceleratorDeviceKey::cuda(0),
        ];
        let a = AcceleratorReconnectController::build(&devices, 13).unwrap();
        let b = AcceleratorReconnectController::build(&[devices[2], devices[0], devices[1]], 13)
            .unwrap();

        let transitions = [
            AcceleratorControlTransition::DeviceFailure {
                device: AcceleratorDeviceKey::cuda(0),
            },
            AcceleratorControlTransition::Reconnect { max_tries: 11 },
            AcceleratorControlTransition::DeviceRecovery {
                device: AcceleratorDeviceKey::cuda(0),
            },
            AcceleratorControlTransition::JobRefresh { max_tries: 9 },
        ];

        let apply_all = |mut state: AcceleratorReconnectController| {
            for transition in transitions {
                state = state.apply(transition).unwrap();
            }
            state
        };

        assert_eq!(apply_all(a), apply_all(b));
    }
}
