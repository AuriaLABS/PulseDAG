from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    text = text.replace(old, new, 1)


replace_once(
    """                    RecoveryProgressDecisionV1::NoPositiveGap => {
                        pending_task27_locator = None;
                        task27_recovery_active.store(false, Ordering::Relaxed);
                    }""",
    """                    RecoveryProgressDecisionV1::NoPositiveGap => {
                        pending_task27_locator = None;
                        let task27_work_remaining = pending_dag_frontier_peer.is_some()
                            || frontier_fetch_scheduler.queue_depth() > 0
                            || !block_requests.pending.is_empty();
                        if !task27_work_remaining {
                            task27_recovery_active.store(false, Ordering::Relaxed);
                        }
                    }""",
    "retain ownership until in-flight work drains after gap close",
)

replace_once(
    """                    RecoveryProgressDecisionV1::Degraded {
                        gap,
                        stagnant_cycles,
                        reason,
                    } if stagnant_cycles == TASK27_REJOIN_MAX_STAGNANT_CYCLES => {
                        task27_recovery_active.store(false, Ordering::Relaxed);
                        let mut rt = runtime.write().await;
                        rt.sync_state = "degraded".to_string();
                        rt.sync_failures = rt.sync_failures.saturating_add(1);
                        warn!(
                            gap,
                            stagnant_cycles,
                            reason = ?reason,
                            "Task 27 bounded rejoin recovery became non-productive"
                        );
                    }
                    RecoveryProgressDecisionV1::AwaitCompatiblePeer { .. }
                    | RecoveryProgressDecisionV1::Productive { .. }
                    | RecoveryProgressDecisionV1::ContinueRecovery { .. }
                    | RecoveryProgressDecisionV1::ScheduleRecovery { .. }
                    | RecoveryProgressDecisionV1::Degraded { .. } => {}""",
    """                    RecoveryProgressDecisionV1::Degraded {
                        gap,
                        stagnant_cycles,
                        reason,
                    } => {
                        let task27_work_remaining = pending_task27_locator.is_some()
                            || pending_dag_frontier_peer.is_some()
                            || frontier_fetch_scheduler.queue_depth() > 0
                            || !block_requests.pending.is_empty();
                        if !task27_work_remaining {
                            task27_recovery_active.store(false, Ordering::Relaxed);
                        }
                        if stagnant_cycles == TASK27_REJOIN_MAX_STAGNANT_CYCLES {
                            let mut rt = runtime.write().await;
                            rt.sync_state = "degraded".to_string();
                            rt.sync_failures = rt.sync_failures.saturating_add(1);
                            warn!(
                                gap,
                                stagnant_cycles,
                                reason = ?reason,
                                "Task 27 bounded rejoin recovery became non-productive"
                            );
                        }
                    }
                    RecoveryProgressDecisionV1::AwaitCompatiblePeer { .. }
                    | RecoveryProgressDecisionV1::Productive { .. }
                    | RecoveryProgressDecisionV1::ContinueRecovery { .. }
                    | RecoveryProgressDecisionV1::ScheduleRecovery { .. } => {}""",
    "retain ownership while degraded work remains and release after drain",
)

path.write_text(text)
