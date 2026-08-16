use pulsedag_p2p::messages::recovery_progress_v1::{
    RecoveryProgressDecisionV1, RecoveryProgressObservationV1, RecoveryProgressReasonV1,
    RecoveryProgressTrackerV1,
};

fn observation() -> RecoveryProgressObservationV1 {
    RecoveryProgressObservationV1 {
        local_selected_height: 100,
        network_selected_height: Some(105),
        compatible_peer_available: true,
        pending_requests: 0,
        inflight_requests: 0,
        pending_missing_parents: 0,
        orphan_count: 0,
        missing_parent_responses: 0,
        orphan_reprocess_attempts: 0,
        orphan_reprocess_successes: 0,
    }
}

#[test]
fn positive_gap_with_empty_queues_requires_rescheduling_then_degrades() {
    let current = observation();
    let mut tracker = RecoveryProgressTrackerV1::new(3);
    assert!(matches!(
        tracker.observe(current),
        RecoveryProgressDecisionV1::ScheduleRecovery {
            stagnant_cycles: 1,
            ..
        }
    ));
    assert!(matches!(
        tracker.observe(current),
        RecoveryProgressDecisionV1::ScheduleRecovery {
            stagnant_cycles: 2,
            ..
        }
    ));
    assert_eq!(
        tracker.observe(current),
        RecoveryProgressDecisionV1::Degraded {
            gap: 5,
            stagnant_cycles: 3,
            reason: RecoveryProgressReasonV1::PositiveGapIdle,
        }
    );
}

#[test]
fn response_and_reprocess_traffic_without_success_is_not_fake_progress() {
    let mut tracker = RecoveryProgressTrackerV1::new(2);
    let mut first = observation();
    first.pending_missing_parents = 4;
    first.orphan_count = 4;
    first.missing_parent_responses = 10;
    first.orphan_reprocess_attempts = 10;
    assert!(matches!(
        tracker.observe(first),
        RecoveryProgressDecisionV1::ContinueRecovery { .. }
    ));

    let mut second = first;
    second.missing_parent_responses = 20;
    second.orphan_reprocess_attempts = 20;
    assert_eq!(
        tracker.observe(second),
        RecoveryProgressDecisionV1::Degraded {
            gap: 5,
            stagnant_cycles: 2,
            reason: RecoveryProgressReasonV1::MissingParentCycleNoProgress,
        }
    );
}

#[test]
fn real_progress_resets_stagnation() {
    let mut tracker = RecoveryProgressTrackerV1::new(3);
    let first = observation();
    tracker.observe(first);

    let mut height = first;
    height.local_selected_height = 101;
    assert_eq!(
        tracker.observe(height),
        RecoveryProgressDecisionV1::Productive { gap: 4 }
    );
    assert_eq!(tracker.stagnant_cycles(), 0);

    let mut backlog = height;
    backlog.pending_missing_parents = 5;
    backlog.orphan_count = 5;
    tracker.observe(backlog);
    let mut drained = backlog;
    drained.pending_missing_parents = 4;
    assert!(matches!(
        tracker.observe(drained),
        RecoveryProgressDecisionV1::Productive { .. }
    ));

    let mut succeeded = drained;
    succeeded.orphan_reprocess_successes = 1;
    assert!(matches!(
        tracker.observe(succeeded),
        RecoveryProgressDecisionV1::Productive { .. }
    ));
}

#[test]
fn incompatible_peer_does_not_authorize_v2_recovery() {
    let mut tracker = RecoveryProgressTrackerV1::new(3);
    let mut current = observation();
    current.compatible_peer_available = false;
    assert_eq!(
        tracker.observe(current),
        RecoveryProgressDecisionV1::AwaitCompatiblePeer { gap: 5 }
    );
    assert_eq!(tracker.stagnant_cycles(), 0);
}

#[test]
fn zero_gap_resets_without_claiming_finality() {
    let mut tracker = RecoveryProgressTrackerV1::new(3);
    tracker.observe(observation());
    let mut current = observation();
    current.network_selected_height = Some(100);
    assert_eq!(
        tracker.observe(current),
        RecoveryProgressDecisionV1::NoPositiveGap
    );
    assert_eq!(tracker.stagnant_cycles(), 0);
}
