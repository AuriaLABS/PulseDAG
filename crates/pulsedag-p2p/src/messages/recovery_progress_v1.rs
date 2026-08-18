#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryProgressObservationV1 {
    pub local_selected_height: u64,
    pub network_selected_height: Option<u64>,
    pub same_height_divergence: bool,
    pub compatible_peer_available: bool,
    pub pending_requests: usize,
    pub inflight_requests: usize,
    pub pending_missing_parents: usize,
    pub orphan_count: usize,
    pub missing_parent_responses: u64,
    pub orphan_reprocess_attempts: u64,
    pub orphan_reprocess_successes: u64,
}

impl RecoveryProgressObservationV1 {
    pub fn network_gap(&self) -> u64 {
        self.network_selected_height
            .unwrap_or(self.local_selected_height)
            .saturating_sub(self.local_selected_height)
    }

    fn active_work(&self) -> bool {
        self.pending_requests > 0
            || self.inflight_requests > 0
            || self.pending_missing_parents > 0
            || self.orphan_count > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryProgressReasonV1 {
    PositiveGapIdle,
    SameHeightDivergenceIdle,
    MissingParentCycleNoProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryProgressDecisionV1 {
    NoPositiveGap,
    AwaitCompatiblePeer {
        gap: u64,
    },
    Productive {
        gap: u64,
    },
    ScheduleRecovery {
        gap: u64,
        stagnant_cycles: u32,
    },
    ContinueRecovery {
        gap: u64,
        stagnant_cycles: u32,
    },
    Degraded {
        gap: u64,
        stagnant_cycles: u32,
        reason: RecoveryProgressReasonV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryProgressTrackerV1 {
    max_stagnant_cycles: u32,
    stagnant_cycles: u32,
    previous: Option<RecoveryProgressObservationV1>,
}

impl RecoveryProgressTrackerV1 {
    pub fn new(max_stagnant_cycles: u32) -> Self {
        assert!(max_stagnant_cycles > 0);
        Self {
            max_stagnant_cycles,
            stagnant_cycles: 0,
            previous: None,
        }
    }

    pub fn stagnant_cycles(&self) -> u32 {
        self.stagnant_cycles
    }

    pub fn observe(
        &mut self,
        current: RecoveryProgressObservationV1,
    ) -> RecoveryProgressDecisionV1 {
        let gap = current.network_gap();
        if gap == 0 && !current.same_height_divergence {
            self.stagnant_cycles = 0;
            self.previous = Some(current);
            return RecoveryProgressDecisionV1::NoPositiveGap;
        }
        if !current.compatible_peer_available {
            self.stagnant_cycles = 0;
            self.previous = Some(current);
            return RecoveryProgressDecisionV1::AwaitCompatiblePeer { gap };
        }

        let productive = self.previous.is_some_and(|previous| {
            current.local_selected_height > previous.local_selected_height
                || current.pending_missing_parents < previous.pending_missing_parents
                || current.orphan_count < previous.orphan_count
                || current.orphan_reprocess_successes > previous.orphan_reprocess_successes
        });
        if productive {
            self.stagnant_cycles = 0;
            self.previous = Some(current);
            return RecoveryProgressDecisionV1::Productive { gap };
        }

        self.stagnant_cycles = self.stagnant_cycles.saturating_add(1);
        let stagnant_cycles = self.stagnant_cycles;
        let active_work = current.active_work();
        let reason = if current.pending_missing_parents > 0 || current.orphan_count > 0 {
            RecoveryProgressReasonV1::MissingParentCycleNoProgress
        } else if gap == 0 && current.same_height_divergence {
            RecoveryProgressReasonV1::SameHeightDivergenceIdle
        } else {
            RecoveryProgressReasonV1::PositiveGapIdle
        };
        self.previous = Some(current);

        if stagnant_cycles >= self.max_stagnant_cycles {
            RecoveryProgressDecisionV1::Degraded {
                gap,
                stagnant_cycles,
                reason,
            }
        } else if active_work {
            RecoveryProgressDecisionV1::ContinueRecovery {
                gap,
                stagnant_cycles,
            }
        } else {
            RecoveryProgressDecisionV1::ScheduleRecovery {
                gap,
                stagnant_cycles,
            }
        }
    }
}
