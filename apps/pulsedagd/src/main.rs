mod app_state;
mod config;

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use app_state::{
    build_startup_lifecycle_events, derive_startup_path_report, new_runtime_stats, AppState,
};
use axum::Router;
use config::Config;
use pulsedag_core::accept::{accept_block, accept_transaction, AcceptSource};
use pulsedag_core::reconcile_mempool;
use pulsedag_p2p::{
    build_p2p_stack, InboundEvent, Libp2pConfig, Libp2pRuntimeMode, P2pHandle, P2pMode,
};
use pulsedag_rpc::routes::router;
use pulsedag_storage::Storage;
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> Result<()> {
    let startup_begin = std::time::Instant::now();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = Config::from_env()?;
    let storage = Arc::new(Storage::open(&cfg.rocksdb_path)?);

    let snapshot_exists = storage.snapshot_exists().unwrap_or(false);
    let persisted_blocks = storage.list_blocks().unwrap_or_default();
    let mut chain_state = storage.load_or_init_genesis(cfg.chain_id.clone())?;
    let startup_persisted_max_height = persisted_blocks
        .iter()
        .map(|b| b.header.height)
        .max()
        .unwrap_or(0);
    let startup_consistency_issue_count = pulsedag_core::dag_consistency_issues(&chain_state).len();
    let mut startup_recovery_mode = if snapshot_exists {
        "snapshot".to_string()
    } else if persisted_blocks.is_empty() {
        "genesis_init".to_string()
    } else {
        "snapshot_missing".to_string()
    };
    let mut startup_rebuild_reason: Option<String> = None;

    if cfg.auto_rebuild_on_start && !persisted_blocks.is_empty() {
        let in_memory_block_count = chain_state.dag.blocks.len();
        let mut rebuild_reasons = Vec::new();
        if !snapshot_exists {
            rebuild_reasons.push("snapshot missing".to_string());
        }
        if persisted_blocks.len() > in_memory_block_count {
            rebuild_reasons.push(format!(
                "persisted blocks ({}) exceed in-memory blocks ({})",
                persisted_blocks.len(),
                in_memory_block_count
            ));
        }
        if startup_consistency_issue_count > 0 {
            rebuild_reasons.push(format!(
                "startup consistency issues detected ({})",
                startup_consistency_issue_count
            ));
        }
        if startup_persisted_max_height > chain_state.dag.best_height {
            rebuild_reasons.push(format!(
                "persisted max height ({}) exceeds snapshot height ({})",
                startup_persisted_max_height, chain_state.dag.best_height
            ));
        }
        if !rebuild_reasons.is_empty() {
            let reason = rebuild_reasons.join("; ");
            info!(snapshot_exists = snapshot_exists, persisted_block_count = persisted_blocks.len(), in_memory_block_count = in_memory_block_count, startup_persisted_max_height, startup_consistency_issue_count, reason = %reason, "rebuilding chain state from persisted blocks on startup");
            startup_recovery_mode = "replayed_blocks".to_string();
            startup_rebuild_reason = Some(reason);
            chain_state = storage.replay_blocks_or_init(cfg.chain_id.clone())?;
        }
    }

    let reconcile_result = reconcile_mempool(&mut chain_state);
    if !reconcile_result.removed_txids.is_empty() {
        warn!(
            removed_mempool_tx = reconcile_result.removed_txids.len(),
            "removed invalid mempool transactions on startup"
        );
    }

    if cfg.persist_snapshot_on_start {
        storage.persist_chain_state(&chain_state)?;
    }

    let (p2p, inbound_rx): (
        Option<Arc<dyn P2pHandle>>,
        Option<tokio::sync::mpsc::UnboundedReceiver<InboundEvent>>,
    ) = if cfg.p2p_enabled {
        let configured_mode = cfg.p2p_mode.clone();
        let stack = match cfg.p2p_mode.as_str() {
            "libp2p-real" => build_p2p_stack(P2pMode::Libp2p(Libp2pConfig {
                chain_id: cfg.chain_id.clone(),
                listen_addr: cfg.p2p_listen.clone(),
                bootstrap: cfg.p2p_bootstrap.clone(),
                enable_mdns: cfg.p2p_mdns,
                enable_kademlia: cfg.p2p_kademlia,
                connection_slot_budget: cfg.p2p_connection_slot_budget,
                sync_selection_stickiness_secs: 30,
                runtime: Libp2pRuntimeMode::RealSwarm,
            }))?,
            "libp2p" | "libp2p-dev" | "libp2p-skeleton" => {
                build_p2p_stack(P2pMode::Libp2p(Libp2pConfig {
                    chain_id: cfg.chain_id.clone(),
                    listen_addr: cfg.p2p_listen.clone(),
                    bootstrap: cfg.p2p_bootstrap.clone(),
                    enable_mdns: cfg.p2p_mdns,
                    enable_kademlia: cfg.p2p_kademlia,
                    connection_slot_budget: cfg.p2p_connection_slot_budget,
                    sync_selection_stickiness_secs: 30,
                    runtime: Libp2pRuntimeMode::DevLoopbackSkeleton,
                }))?
            }
            "memory" | "simulated" => build_p2p_stack(P2pMode::Memory {
                chain_id: cfg.chain_id.clone(),
                peers: cfg.simulated_peers.clone(),
            })?,
            other => {
                warn!(configured_mode = %other, "unknown P2P mode, using memory-simulated mode");
                build_p2p_stack(P2pMode::Memory {
                    chain_id: cfg.chain_id.clone(),
                    peers: cfg.simulated_peers.clone(),
                })?
            }
        };
        if let Ok(status) = stack.handle.status() {
            info!(
                configured_mode = %configured_mode,
                effective_mode = %status.mode,
                runtime_mode_detail = %status.runtime_mode_detail,
                connected_peers_are_real_network = pulsedag_p2p::mode_connected_peers_are_real_network(&status.mode),
                connected_peers_semantics = pulsedag_p2p::connected_peers_semantics(&status.mode),
                "p2p initialized"
            );
        } else {
            warn!(configured_mode = %configured_mode, "p2p initialized but status unavailable");
        }
        (Some(stack.handle), stack.inbound_rx)
    } else {
        info!("p2p disabled");
        (None, None)
    };

    let startup_report = derive_startup_path_report(
        &startup_recovery_mode,
        snapshot_exists,
        persisted_blocks.len(),
        startup_rebuild_reason.clone(),
    );
    let startup_duration_ms = startup_begin.elapsed().as_millis();
    let mut runtime_stats = new_runtime_stats();
    runtime_stats.startup_snapshot_exists = snapshot_exists;
    runtime_stats.startup_persisted_block_count = persisted_blocks.len();
    runtime_stats.startup_persisted_max_height = startup_persisted_max_height;
    runtime_stats.startup_consistency_issue_count = startup_consistency_issue_count;
    runtime_stats.startup_recovery_mode = startup_recovery_mode.clone();
    runtime_stats.startup_rebuild_reason = startup_rebuild_reason.clone();
    runtime_stats.startup_path = startup_report.startup_path.clone();
    runtime_stats.startup_bootstrap_mode = startup_report.startup_bootstrap_mode.clone();
    runtime_stats.startup_status_summary = startup_report.startup_status_summary.clone();
    runtime_stats.startup_fastboot_used = startup_report.startup_fastboot_used;
    runtime_stats.startup_snapshot_detected = startup_report.startup_snapshot_detected;
    runtime_stats.startup_snapshot_validated = startup_report.startup_snapshot_validated;
    runtime_stats.startup_delta_applied = startup_report.startup_delta_applied;
    runtime_stats.startup_replay_required = startup_report.startup_replay_required;
    runtime_stats.startup_fallback_reason = startup_report.startup_fallback_reason.clone();
    runtime_stats.startup_duration_ms = startup_duration_ms;
    runtime_stats.last_self_audit_unix = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    runtime_stats.last_self_audit_ok = startup_consistency_issue_count == 0;
    runtime_stats.last_self_audit_issue_count = startup_consistency_issue_count;
    runtime_stats.last_self_audit_message = if startup_consistency_issue_count == 0 {
        Some("startup audit ok".to_string())
    } else {
        Some(format!(
            "startup audit found {} consistency issues",
            startup_consistency_issue_count
        ))
    };
    runtime_stats.last_observed_best_height = chain_state.dag.best_height;
    runtime_stats.last_height_change_unix = runtime_stats.last_self_audit_unix;
    runtime_stats.active_alerts = Vec::new();
    runtime_stats.snapshot_auto_every_blocks = cfg.snapshot_auto_every_blocks;
    runtime_stats.auto_prune_enabled = cfg.auto_prune_enabled;
    runtime_stats.auto_prune_every_blocks = cfg.auto_prune_every_blocks;
    runtime_stats.prune_keep_recent_blocks = cfg.prune_keep_recent_blocks;
    runtime_stats.prune_require_snapshot = cfg.prune_require_snapshot;
    runtime_stats.last_snapshot_height = if snapshot_exists {
        Some(chain_state.dag.best_height)
    } else {
        None
    };
    runtime_stats.last_snapshot_unix = storage.snapshot_captured_at_unix().ok().flatten();
    runtime_stats.sync_pipeline.resume_after_restart(now_unix());

    let app_state = AppState {
        chain: Arc::new(tokio::sync::RwLock::new(chain_state)),
        storage: storage.clone(),
        p2p,
        runtime: Arc::new(tokio::sync::RwLock::new(runtime_stats)),
    };

    {
        let lifecycle_events = build_startup_lifecycle_events(
            &startup_recovery_mode,
            &startup_report,
            startup_duration_ms,
        );
        for event in lifecycle_events {
            let _ = app_state
                .storage
                .append_runtime_event(event.level, event.kind, &event.message);
        }

        let summary = if startup_consistency_issue_count == 0 {
            format!(
                "startup audit ok; recovery_mode={}; startup_path={}",
                startup_recovery_mode, startup_report.startup_path
            )
        } else {
            format!(
                "startup audit found {} consistency issues; recovery_mode={}; startup_path={}",
                startup_consistency_issue_count, startup_recovery_mode, startup_report.startup_path
            )
        };
        let _ = app_state
            .storage
            .append_runtime_event("info", "startup_audit", &summary);
        if let Some(reason) = startup_report.startup_fallback_reason.clone() {
            let _ = app_state
                .storage
                .append_runtime_event("warn", "startup_rebuild", &reason);
        }
        let _ = app_state.storage.append_runtime_event(
            "info",
            "startup_path",
            &format!(
                "path={} bootstrap_mode={} fastboot_used={} snapshot_detected={} snapshot_validated={} delta_applied={} replay_required={} duration_ms={} summary=\"{}\"",
                startup_report.startup_path,
                startup_report.startup_bootstrap_mode,
                startup_report.startup_fastboot_used,
                startup_report.startup_snapshot_detected,
                startup_report.startup_snapshot_validated,
                startup_report.startup_delta_applied,
                startup_report.startup_replay_required,
                startup_duration_ms,
                startup_report.startup_status_summary
            ),
        );
    }

    if let Some(mut rx) = inbound_rx {
        let chain = app_state.chain.clone();
        let storage = storage.clone();
        let runtime = app_state.runtime.clone();
        let p2p = app_state.p2p.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    InboundEvent::Transaction(tx) => {
                        let txid = tx.txid.clone();
                        {
                            let mut rt = runtime.write().await;
                            rt.tx_inbound_total += 1;
                        }
                        let mut guard = chain.write().await;
                        let already_in_mempool = guard.mempool.transactions.contains_key(&txid);
                        let already_confirmed =
                            guard.dag.blocks.values().any(|block| {
                                block.transactions.iter().any(|known| known.txid == txid)
                            });
                        if already_in_mempool || already_confirmed {
                            let mut rt = runtime.write().await;
                            rt.duplicate_p2p_txs += 1;
                            rt.dropped_p2p_txs += 1;
                            rt.tx_inbound_dropped_total += 1;
                            rt.last_tx_drop_unix = Some(now_unix());
                            rt.last_tx_drop_txid = Some(txid.clone());
                            let reason = if already_in_mempool {
                                rt.dropped_p2p_txs_duplicate_mempool += 1;
                                "duplicate_mempool"
                            } else {
                                rt.dropped_p2p_txs_duplicate_confirmed += 1;
                                "duplicate_confirmed"
                            };
                            rt.last_tx_drop_reason = Some(reason.to_string());
                            rt.tx_drop_reasons
                                .push(format!("txid={} reason={}", txid, reason));
                            if rt.tx_drop_reasons.len() > 32 {
                                let overflow = rt.tx_drop_reasons.len() - 32;
                                rt.tx_drop_reasons.drain(0..overflow);
                            }
                            info!(txid = %txid, already_in_mempool, already_confirmed, "ignored duplicate inbound p2p transaction");
                            let _ = storage.append_runtime_event(
                                "info",
                                "tx_drop",
                                &format!(
                                    "txid={} reason={}",
                                    txid,
                                    if already_in_mempool {
                                        "duplicate_mempool"
                                    } else {
                                        "duplicate_confirmed"
                                    }
                                ),
                            );
                            continue;
                        }
                        if let Err(e) = accept_transaction(tx, &mut guard, AcceptSource::P2p) {
                            let mut rt = runtime.write().await;
                            rt.rejected_p2p_txs += 1;
                            rt.dropped_p2p_txs += 1;
                            rt.tx_inbound_rejected_total += 1;
                            rt.tx_inbound_dropped_total += 1;
                            rt.dropped_p2p_txs_accept_failed += 1;
                            let now = now_unix();
                            rt.last_tx_reject_unix = Some(now);
                            rt.last_tx_drop_unix = Some(now);
                            rt.last_tx_drop_reason = Some("accept_failed".to_string());
                            rt.last_tx_drop_txid = Some(txid.clone());
                            rt.tx_drop_reasons
                                .push(format!("txid={} reason=accept_failed error={}", txid, e));
                            if rt.tx_drop_reasons.len() > 32 {
                                let overflow = rt.tx_drop_reasons.len() - 32;
                                rt.tx_drop_reasons.drain(0..overflow);
                            }
                            warn!(txid = %txid, error = %e, "rejected inbound p2p transaction");
                            let _ = storage.append_runtime_event(
                                "warn",
                                "tx_reject",
                                &format!("txid={} reason=accept_failed error={}", txid, e),
                            );
                        } else {
                            let tx_for_rebroadcast = guard.mempool.transactions.get(&txid).cloned();
                            let snapshot = guard.clone();
                            if let Err(e) = storage.persist_chain_state(&snapshot) {
                                drop(guard);
                                let mut rt = runtime.write().await;
                                rt.dropped_p2p_txs += 1;
                                rt.tx_inbound_dropped_total += 1;
                                rt.dropped_p2p_txs_persist_failed += 1;
                                rt.last_tx_drop_unix = Some(now_unix());
                                rt.last_tx_drop_reason = Some("persist_failed".to_string());
                                rt.last_tx_drop_txid = Some(txid.clone());
                                rt.tx_drop_reasons.push(format!(
                                    "txid={} reason=persist_failed error={}",
                                    txid, e
                                ));
                                if rt.tx_drop_reasons.len() > 32 {
                                    let overflow = rt.tx_drop_reasons.len() - 32;
                                    rt.tx_drop_reasons.drain(0..overflow);
                                }
                                warn!(error = %e, "failed persisting chain state after inbound transaction");
                                let _ = storage.append_runtime_event(
                                    "warn",
                                    "tx_drop",
                                    &format!("txid={} reason=persist_failed error={}", txid, e),
                                );
                            } else {
                                drop(guard);
                                let mut rt = runtime.write().await;
                                rt.accepted_p2p_txs += 1;
                                rt.tx_inbound_accepted_total += 1;
                                rt.last_tx_accept_unix = Some(now_unix());
                                if let Some(ref p2p) = p2p {
                                    if let Ok(status) = p2p.status() {
                                        if status.connected_peers.is_empty() {
                                            let peers_are_real_network =
                                                pulsedag_p2p::mode_connected_peers_are_real_network(
                                                    &status.mode,
                                                );
                                            let skip_reason = if peers_are_real_network {
                                                "no_connected_peers"
                                            } else {
                                                "no_real_network_connectivity_in_current_mode"
                                            };
                                            rt.tx_rebroadcast_skipped_no_peers += 1;
                                            warn!(
                                                txid = %txid,
                                                reason = skip_reason,
                                                mode = %status.mode,
                                                connected_peers = status.connected_peers.len(),
                                                "skipping transaction rebroadcast"
                                            );
                                            let _ = storage.append_runtime_event(
                                                "warn",
                                                "tx_rebroadcast_skipped",
                                                &format!("txid={} reason={}", txid, skip_reason),
                                            );
                                            continue;
                                        }
                                    }
                                    rt.tx_rebroadcast_attempts += 1;
                                    rt.last_tx_rebroadcast_unix = Some(now_unix());
                                    rt.last_tx_rebroadcast_error = None;
                                    match tx_for_rebroadcast.as_ref() {
                                        Some(tx_to_rebroadcast) => {
                                            match p2p.broadcast_transaction(tx_to_rebroadcast) {
                                                Ok(_) => {
                                                    rt.tx_rebroadcast_success += 1;
                                                    info!(txid = %txid, "rebroadcasted accepted inbound p2p transaction");
                                                }
                                                Err(e) => {
                                                    rt.tx_rebroadcast_failed += 1;
                                                    rt.last_tx_rebroadcast_error =
                                                        Some(e.to_string());
                                                    warn!(txid = %txid, error = %e, "failed rebroadcasting accepted inbound p2p transaction");
                                                    let _ = storage.append_runtime_event(
                                                        "warn",
                                                        "tx_rebroadcast_failed",
                                                        &format!("txid={} error={}", txid, e),
                                                    );
                                                }
                                            }
                                        }
                                        None => {
                                            rt.tx_rebroadcast_failed += 1;
                                            rt.last_tx_rebroadcast_error =
                                                Some("tx_missing_after_accept".to_string());
                                            warn!(txid = %txid, "accepted transaction missing from mempool before rebroadcast");
                                        }
                                    }
                                } else {
                                    rt.tx_rebroadcast_skipped_no_p2p += 1;
                                    info!(txid = %txid, "skipping transaction rebroadcast because p2p is disabled");
                                    let _ = storage.append_runtime_event(
                                        "info",
                                        "tx_rebroadcast_skipped",
                                        &format!("txid={} reason=p2p_disabled", txid),
                                    );
                                }
                            }
                        }
                    }
                    InboundEvent::Block(block) => {
                        {
                            let mut rt = runtime.write().await;
                            let now = now_unix();
                            rt.sync_pipeline.begin_cycle(now);
                            rt.sync_pipeline.observe_peer_candidate(now);
                        }
                        let mut guard = chain.write().await;
                        if guard.dag.blocks.contains_key(&block.hash)
                            || guard.orphan_blocks.contains_key(&block.hash)
                        {
                            let mut rt = runtime.write().await;
                            rt.duplicate_p2p_blocks += 1;
                            info!(block = %block.hash, "ignored duplicate inbound p2p block");
                            continue;
                        }
                        {
                            let mut rt = runtime.write().await;
                            let now = now_unix();
                            rt.sync_pipeline.observe_headers(1, now);
                            rt.sync_pipeline.request_blocks(1, now);
                            rt.sync_pipeline.acquire_blocks(1);
                        }
                        let missing_parents = pulsedag_core::missing_block_parents(&block, &guard);
                        if !missing_parents.is_empty() {
                            pulsedag_core::queue_orphan_block(
                                &mut guard,
                                block.clone(),
                                missing_parents.clone(),
                            );
                            let pruned = pulsedag_core::prune_orphans(
                                &mut guard,
                                pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                                pulsedag_core::DEFAULT_ORPHAN_MAX_AGE_MS,
                            );
                            {
                                let mut rt = runtime.write().await;
                                rt.queued_orphan_blocks += 1;
                                rt.sync_pipeline.fallback_after_failure(
                                    format!(
                                        "orphaned block {} missing parents {:?}",
                                        block.hash, missing_parents
                                    ),
                                    now_unix(),
                                );
                            }
                            info!(block = %block.hash, missing_parents = ?missing_parents, orphan_count = guard.orphan_blocks.len(), pruned, "queued inbound p2p orphan block");
                            if let Err(e) = storage.persist_block_and_chain_state(&block, &guard) {
                                warn!(error = %e, "failed persisting chain state after orphan queue");
                            }
                        } else if let Err(e) =
                            accept_block(block.clone(), &mut guard, AcceptSource::P2p)
                        {
                            let mut rt = runtime.write().await;
                            rt.rejected_p2p_blocks += 1;
                            rt.sync_pipeline.fallback_after_failure(
                                format!("block {} validation failed: {}", block.hash, e),
                                now_unix(),
                            );
                            warn!(error = %e, "rejected inbound p2p block");
                        } else {
                            let adopted =
                                pulsedag_core::adopt_ready_orphans(&mut guard, AcceptSource::P2p);
                            {
                                let mut rt = runtime.write().await;
                                rt.sync_pipeline.validate_and_apply_blocks(1, now_unix());
                                rt.accepted_p2p_blocks += 1;
                                rt.adopted_orphan_blocks += adopted as u64;
                                rt.sync_pipeline.complete_cycle(now_unix());
                            }
                            if adopted > 0 {
                                info!(
                                    adopted,
                                    remaining_orphans = guard.orphan_blocks.len(),
                                    "adopted ready orphan blocks after inbound block"
                                );
                            }
                            if let Err(e) = storage.persist_block_and_chain_state(&block, &guard) {
                                warn!(error = %e, "failed persisting chain state after inbound block");
                            }
                        }
                    }
                    InboundEvent::PeerConnected(peer) => {
                        info!(peer = %peer, "p2p peer connected or runtime event");
                    }
                }
            }
        });
    }

    {
        let chain = app_state.chain.clone();
        let runtime = app_state.runtime.clone();
        let storage = app_state.storage.clone();
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(60)).await;
                let (issue_count, best_height, orphan_count, mempool_size) = {
                    let guard = chain.read().await;
                    (
                        pulsedag_core::dag_consistency_issues(&guard).len(),
                        guard.dag.best_height,
                        guard.orphan_blocks.len(),
                        guard.mempool.transactions.len(),
                    )
                };
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let mut rt = runtime.write().await;
                if best_height > rt.last_observed_best_height {
                    rt.last_observed_best_height = best_height;
                    rt.last_height_change_unix = Some(now);
                }
                let mut active_alerts = Vec::new();
                if orphan_count >= 32 {
                    active_alerts.push(format!("high orphan count: {}", orphan_count));
                }
                if mempool_size >= 512 {
                    active_alerts.push(format!("high mempool size: {}", mempool_size));
                }
                let stagnation_secs = rt
                    .last_height_change_unix
                    .map(|ts| now.saturating_sub(ts))
                    .unwrap_or(0);
                if stagnation_secs >= 600 {
                    active_alerts.push(format!("height stagnant for {} seconds", stagnation_secs));
                }
                rt.active_alerts = active_alerts.clone();
                rt.last_self_audit_unix = Some(now);
                rt.last_self_audit_ok = issue_count == 0;
                rt.last_self_audit_issue_count = issue_count;
                rt.last_self_audit_message = if issue_count == 0 {
                    Some(format!("periodic self audit ok at height {}", best_height))
                } else {
                    Some(format!(
                        "periodic self audit found {} issues at height {}",
                        issue_count, best_height
                    ))
                };
                if issue_count == 0 {
                    info!(
                        best_height,
                        orphan_count,
                        mempool_size,
                        active_alert_count = rt.active_alerts.len(),
                        "periodic self audit ok"
                    );
                } else {
                    warn!(
                        best_height,
                        issue_count,
                        orphan_count,
                        mempool_size,
                        active_alert_count = rt.active_alerts.len(),
                        "periodic self audit found consistency issues"
                    );
                    let _ = storage.append_runtime_event(
                        "warn",
                        "consistency_issue",
                        &format!(
                            "{} consistency issues detected at height {}",
                            issue_count, best_height
                        ),
                    );
                }
                if !active_alerts.is_empty() {
                    warn!(best_height, alerts = ?active_alerts, "runtime alerts active");
                    let _ = storage.append_runtime_event(
                        "warn",
                        "runtime_alert",
                        &format!(
                            "height {} alerts: {}",
                            best_height,
                            active_alerts.join(" | ")
                        ),
                    );
                }
            }
        });
    }

    {
        let chain = app_state.chain.clone();
        let runtime = app_state.runtime.clone();
        let storage = app_state.storage.clone();
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(5)).await;
                let chain_snapshot = chain.read().await.clone();
                let best_height = chain_snapshot.dag.best_height;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let (
                    snapshot_every,
                    last_snapshot_height,
                    auto_prune_enabled,
                    auto_prune_every,
                    prune_keep_recent_blocks,
                    last_prune_height,
                ) = {
                    let rt = runtime.read().await;
                    (
                        rt.snapshot_auto_every_blocks,
                        rt.last_snapshot_height,
                        rt.auto_prune_enabled,
                        rt.auto_prune_every_blocks,
                        rt.prune_keep_recent_blocks.max(1),
                        rt.last_prune_height,
                    )
                };

                if snapshot_every > 0 && best_height > 0 {
                    let should_snapshot = last_snapshot_height
                        .map(|h| best_height.saturating_sub(h) >= snapshot_every)
                        .unwrap_or(best_height >= snapshot_every);
                    if should_snapshot {
                        if let Err(e) = storage.persist_chain_state(&chain_snapshot) {
                            warn!(error = %e, best_height, "auto snapshot failed");
                            let _ = storage.append_runtime_event(
                                "warn",
                                "snapshot_auto_failed",
                                &format!(
                                    "failed persisting auto snapshot at height {}: {}",
                                    best_height, e
                                ),
                            );
                        } else {
                            let captured_at = storage
                                .snapshot_captured_at_unix()
                                .ok()
                                .flatten()
                                .unwrap_or(now);
                            {
                                let mut rt = runtime.write().await;
                                rt.last_snapshot_height = Some(best_height);
                                rt.last_snapshot_unix = Some(captured_at);
                            }
                            info!(best_height, captured_at, "auto snapshot persisted");
                            let _ = storage.append_runtime_event(
                                "info",
                                "snapshot_auto",
                                &format!("auto snapshot persisted at height {}", best_height),
                            );
                        }
                    }
                }

                if auto_prune_enabled && auto_prune_every > 0 && best_height > 0 {
                    let should_prune = last_prune_height
                        .map(|h| best_height.saturating_sub(h) >= auto_prune_every)
                        .unwrap_or(best_height >= auto_prune_every);
                    if should_prune {
                        let keep_from_height =
                            best_height.saturating_sub(prune_keep_recent_blocks.saturating_sub(1));
                        let snapshot_status = match storage.load_chain_state() {
                            Ok(Some(snapshot)) => (
                                snapshot.dag.best_height >= keep_from_height,
                                Some(snapshot.dag.best_height),
                            ),
                            Ok(None) => (false, None),
                            Err(_) => (false, None),
                        };
                        if !snapshot_status.0 {
                            let reason = match snapshot_status.1 {
                                Some(height) => format!(
                                    "auto prune skipped at height {}: snapshot_height={} below keep_from_height={}",
                                    best_height, height, keep_from_height
                                ),
                                None => format!(
                                    "auto prune skipped at height {}: validated snapshot required and missing (keep_from_height={})",
                                    best_height, keep_from_height
                                ),
                            };
                            warn!(
                                best_height,
                                keep_from_height,
                                "auto prune skipped; validated snapshot+delta base required"
                            );
                            let _ =
                                storage.append_runtime_event("warn", "prune_auto_skipped", &reason);
                            continue;
                        }

                        match storage.prune_blocks_below_height(keep_from_height) {
                            Ok(pruned) => {
                                match storage
                                    .replay_from_validated_snapshot_and_delta(Some(&cfg.chain_id))
                                {
                                    Ok(rebuilt) => {
                                        {
                                            let mut chain_guard = chain.write().await;
                                            *chain_guard = rebuilt.clone();
                                        }
                                        {
                                            let mut rt = runtime.write().await;
                                            rt.last_prune_height = Some(rebuilt.dag.best_height);
                                            rt.last_prune_unix = Some(now);
                                            rt.last_snapshot_height = Some(rebuilt.dag.best_height);
                                            rt.last_snapshot_unix = storage
                                                .snapshot_captured_at_unix()
                                                .ok()
                                                .flatten()
                                                .or(Some(now));
                                        }
                                        info!(
                                            best_height = rebuilt.dag.best_height,
                                            pruned,
                                            keep_from_height,
                                            "auto prune completed and snapshot+delta verified"
                                        );
                                        let _ = storage.append_runtime_event("info", "prune_auto", &format!("auto prune removed {} blocks below {}; snapshot+delta verified at height {}", pruned, keep_from_height, rebuilt.dag.best_height));
                                    }
                                    Err(e) => {
                                        warn!(error = %e, best_height, keep_from_height, "auto prune snapshot+delta verification failed");
                                        let _ = storage.append_runtime_event(
                                        "error",
                                        "prune_auto_failed",
                                        &format!(
                                            "auto prune snapshot+delta verify failed at height {}: {}",
                                            best_height, e
                                        ),
                                    );
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, best_height, keep_from_height, "auto prune failed");
                                let _ = storage.append_runtime_event(
                                    "error",
                                    "prune_auto_failed",
                                    &format!("auto prune failed at height {}: {}", best_height, e),
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    let app: Router = router().with_state(app_state);
    let addr: SocketAddr = cfg.rpc_bind.parse()?;
    let listener = TcpListener::bind(addr).await?;

    info!(p2p_enabled = cfg.p2p_enabled, p2p_mode = %cfg.p2p_mode, auto_rebuild_on_start = cfg.auto_rebuild_on_start, persist_snapshot_on_start = cfg.persist_snapshot_on_start, snapshot_auto_every_blocks = cfg.snapshot_auto_every_blocks, auto_prune_enabled = cfg.auto_prune_enabled, auto_prune_every_blocks = cfg.auto_prune_every_blocks, prune_keep_recent_blocks = cfg.prune_keep_recent_blocks, prune_require_snapshot = cfg.prune_require_snapshot, target_block_interval_secs = cfg.target_block_interval_secs, difficulty_window = cfg.difficulty_window, max_future_drift_secs = cfg.max_future_drift_secs, "pulsedagd RPC listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
