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
    '''                                        Ok(locator) => {
                                            let selected_tip = locator.selected_tip.clone();
                                            match p2p_handle.send_protocol_sync_v1(
                                                &peer_id,
                                                &ProtocolSyncWireV1::SelectedChainLocator(locator),
                                            ) {
                                                Ok(()) => {
                                                    pending_task27_locator =
                                                        Some(PendingTask27Locator {
                                                            peer_id: peer_id.clone(),
                                                            selected_tip: selected_tip.clone(),
                                                            sent_at_unix: now,
                                                        });
                                                    task27_recovery_active
                                                        .store(true, Ordering::SeqCst);
                                                    let mut rt = runtime.write().await;
                                                    rt.selected_segment_gap_blocks =
                                                        rt.selected_segment_gap_blocks.max(
                                                            remote_height.saturating_sub(
                                                                local_selected_height,
                                                            ),
                                                        );
                                                    rt.dag_sync_selected_chain_locator_total = rt
                                                        .dag_sync_selected_chain_locator_total
                                                        .saturating_add(1);
                                                    rt.sync_state =
                                                        DagSyncStage::SelectedChainLocator
                                                            .as_str()
                                                            .to_string();
                                                    info!(
                                                        peer = %peer_id,
                                                        selected_tip = %selected_tip,
                                                        local_selected_height,
                                                        remote_height,
                                                        gap,
                                                        stagnant_cycles,
                                                        "started Task 27 bounded rejoin locator recovery"
                                                    );
                                                }
                                                Err(error) => {
                                                    warn!(
                                                        peer = %peer_id,
                                                        error = %error,
                                                        "failed sending Task 27 rejoin locator"
                                                    );
                                                }
                                            }
                                        }''',
    '''                                        Ok(locator) => {
                                            let selected_tip = locator.selected_tip.clone();
                                            let locator_guard =
                                                selected_segment_locator_state.lock().await;
                                            let legacy_priority_now =
                                                selected_segment_recovery_has_priority(
                                                    selected_segment_session.is_some(),
                                                    locator_guard
                                                        .pending_locator
                                                        .as_ref()
                                                        .map(|pending| pending.requested_at_unix),
                                                    now,
                                                );
                                            if !legacy_priority_now
                                                && !task27_recovery_active
                                                    .load(Ordering::SeqCst)
                                            {
                                                task27_recovery_active
                                                    .store(true, Ordering::SeqCst);
                                                let send_result =
                                                    p2p_handle.send_protocol_sync_v1(
                                                        &peer_id,
                                                        &ProtocolSyncWireV1::SelectedChainLocator(
                                                            locator,
                                                        ),
                                                    );
                                                drop(locator_guard);
                                                match send_result {
                                                    Ok(()) => {
                                                        pending_task27_locator =
                                                            Some(PendingTask27Locator {
                                                                peer_id: peer_id.clone(),
                                                                selected_tip: selected_tip.clone(),
                                                                sent_at_unix: now,
                                                            });
                                                        let mut rt = runtime.write().await;
                                                        rt.selected_segment_gap_blocks = rt
                                                            .selected_segment_gap_blocks
                                                            .max(remote_height.saturating_sub(
                                                                local_selected_height,
                                                            ));
                                                        rt.dag_sync_selected_chain_locator_total = rt
                                                            .dag_sync_selected_chain_locator_total
                                                            .saturating_add(1);
                                                        rt.sync_state =
                                                            DagSyncStage::SelectedChainLocator
                                                                .as_str()
                                                                .to_string();
                                                        info!(
                                                            peer = %peer_id,
                                                            selected_tip = %selected_tip,
                                                            local_selected_height,
                                                            remote_height,
                                                            gap,
                                                            stagnant_cycles,
                                                            "started Task 27 bounded rejoin locator recovery"
                                                        );
                                                    }
                                                    Err(error) => {
                                                        task27_recovery_active
                                                            .store(false, Ordering::SeqCst);
                                                        warn!(
                                                            peer = %peer_id,
                                                            error = %error,
                                                            "failed sending Task 27 rejoin locator"
                                                        );
                                                    }
                                                }
                                            } else {
                                                drop(locator_guard);
                                            }
                                        }''',
    "serialize Task 27 locator start",
)

replace_once(
    '''                                let selected_locator_request_id = {
                                    let guard = selected_segment_locator_state.lock().await;
                                    guard.next_request_id
                                };
                                if p2p_handle
                                    .request_headers(
                                        &selected_locator,
                                        None,
                                        selected_limits.headers_per_chunk,
                                    )
                                    .is_ok()
                                {
                                    let mut guard = selected_segment_locator_state.lock().await;
                                    guard.next_request_id = guard.next_request_id.saturating_add(1);
                                    guard.pending_locator = Some(PendingSelectedLocator {
                                        request_id: selected_locator_request_id,
                                        peer_id: peer_id.clone(),
                                        locator: selected_locator,
                                        requested_at_unix: now,
                                    });
                                    drop(guard);
                                    let mut rt = runtime.write().await;''',
    '''                                let mut locator_guard =
                                    selected_segment_locator_state.lock().await;
                                let priority_still_inactive =
                                    !selected_segment_recovery_has_priority(
                                        selected_segment_session.is_some(),
                                        locator_guard
                                            .pending_locator
                                            .as_ref()
                                            .map(|pending| pending.requested_at_unix),
                                        now,
                                    ) && !task27_recovery_active.load(Ordering::SeqCst);
                                if priority_still_inactive {
                                    let selected_locator_request_id = locator_guard.next_request_id;
                                    if p2p_handle
                                        .request_headers(
                                            &selected_locator,
                                            None,
                                            selected_limits.headers_per_chunk,
                                        )
                                        .is_ok()
                                    {
                                        locator_guard.next_request_id =
                                            locator_guard.next_request_id.saturating_add(1);
                                        locator_guard.pending_locator =
                                            Some(PendingSelectedLocator {
                                                request_id: selected_locator_request_id,
                                                peer_id: peer_id.clone(),
                                                locator: selected_locator,
                                                requested_at_unix: now,
                                            });
                                        drop(locator_guard);
                                        let mut rt = runtime.write().await;''',
    "serialize immediate legacy locator start",
)

replace_once(
    '''                                    info!(
                                        peer = %peer_id,
                                        local_height,
                                        remote_height,
                                        "remote tip inventory activated selected-segment priority before generic tip fetch"
                                    );
                                }
                            }
                        }

                        let unknown_tips = {''',
    '''                                        info!(
                                            peer = %peer_id,
                                            local_height,
                                            remote_height,
                                            "remote tip inventory activated selected-segment priority before generic tip fetch"
                                        );
                                    } else {
                                        drop(locator_guard);
                                    }
                                } else {
                                    drop(locator_guard);
                                }
                            }
                        }

                        let unknown_tips = {''',
    "close immediate legacy locator serialization",
)

replace_once(
    '''                        let selected_locator_request_id = {
                            let guard = selected_segment_locator_state.lock().await;
                            guard.next_request_id
                        };
                        let selected_locator_requested = p2p_handle
                            .request_headers(
                                &selected_locator,
                                None,
                                selected_limits.headers_per_chunk,
                            )
                            .is_ok();
                        if selected_locator_requested {
                            let mut guard = selected_segment_locator_state.lock().await;
                            guard.next_request_id = guard.next_request_id.saturating_add(1);
                            guard.pending_locator = Some(PendingSelectedLocator {
                                request_id: selected_locator_request_id,
                                peer_id: peer_id.clone(),
                                locator: selected_locator,
                                requested_at_unix: now,
                            });
                            let mut rt = runtime.write().await;''',
    '''                        let mut locator_guard =
                            selected_segment_locator_state.lock().await;
                        let priority_still_inactive =
                            !selected_segment_recovery_has_priority(
                                active_session,
                                locator_guard
                                    .pending_locator
                                    .as_ref()
                                    .map(|pending| pending.requested_at_unix),
                                now,
                            ) && !task27_recovery_active.load(Ordering::SeqCst);
                        let selected_locator_requested = priority_still_inactive
                            && p2p_handle
                                .request_headers(
                                    &selected_locator,
                                    None,
                                    selected_limits.headers_per_chunk,
                                )
                                .is_ok();
                        if selected_locator_requested {
                            let selected_locator_request_id = locator_guard.next_request_id;
                            locator_guard.next_request_id =
                                locator_guard.next_request_id.saturating_add(1);
                            locator_guard.pending_locator = Some(PendingSelectedLocator {
                                request_id: selected_locator_request_id,
                                peer_id: peer_id.clone(),
                                locator: selected_locator,
                                requested_at_unix: now,
                            });
                            drop(locator_guard);
                            let mut rt = runtime.write().await;''',
    "serialize proactive legacy locator start",
)

replace_once(
    '''                            info!(
                                peer = %peer_id,
                                local_height = best_height,
                                remote_height,
                                "large remote selected-height gap activated selected-segment priority"
                            );
                        }
                    }
                }

                if final_quiescence_due {''',
    '''                            info!(
                                peer = %peer_id,
                                local_height = best_height,
                                remote_height,
                                "large remote selected-height gap activated selected-segment priority"
                            );
                        } else {
                            drop(locator_guard);
                        }
                    }
                }

                if final_quiescence_due {''',
    "close proactive locator serialization",
)

replace_once(
    '''                                    let selected_locator_request_id = {
                                        let guard = selected_segment_locator_state.lock().await;
                                        guard.next_request_id
                                    };
                                    let selected_locator_requested = selected_locator_peer
                                        .is_some()
                                        && p2p
                                            .request_headers(
                                                &selected_locator,
                                                None,
                                                selected_limits.headers_per_chunk,
                                            )
                                            .is_ok();
                                    if selected_locator_requested {
                                        let mut guard = selected_segment_locator_state.lock().await;
                                        guard.next_request_id =
                                            guard.next_request_id.saturating_add(1);
                                        guard.pending_locator =
                                            selected_locator_peer.clone().map(|peer_id| {
                                                PendingSelectedLocator {
                                                    request_id: selected_locator_request_id,
                                                    peer_id,
                                                    locator: selected_locator.clone(),
                                                    requested_at_unix: now_unix(),
                                                }
                                            });
                                    } else if !selected_locator_needed {
                                        selected_segment_locator_state
                                            .lock()
                                            .await
                                            .pending_locator = None;
                                    }
                                    let requested = p2p.request_tips().is_ok();''',
    '''                                    let mut locator_guard =
                                        selected_segment_locator_state.lock().await;
                                    let selected_locator_requested = selected_locator_peer
                                        .is_some()
                                        && !task27_recovery_active.load(Ordering::SeqCst)
                                        && p2p
                                            .request_headers(
                                                &selected_locator,
                                                None,
                                                selected_limits.headers_per_chunk,
                                            )
                                            .is_ok();
                                    if selected_locator_requested {
                                        let selected_locator_request_id =
                                            locator_guard.next_request_id;
                                        locator_guard.next_request_id =
                                            locator_guard.next_request_id.saturating_add(1);
                                        locator_guard.pending_locator =
                                            selected_locator_peer.clone().map(|peer_id| {
                                                PendingSelectedLocator {
                                                    request_id: selected_locator_request_id,
                                                    peer_id,
                                                    locator: selected_locator.clone(),
                                                    requested_at_unix: now_unix(),
                                                }
                                            });
                                    } else if !selected_locator_needed {
                                        locator_guard.pending_locator = None;
                                    }
                                    drop(locator_guard);
                                    let requested = p2p.request_tips().is_ok();''',
    "serialize final-quiescence locator start",
)

path.write_text(text)
