mod app_state;
mod config;

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use app_state::{new_runtime_stats, AppState};
use axum::Router;
use config::Config;
use pulsedag_core::accept::{accept_block, accept_transaction, AcceptSource};
use pulsedag_core::reconcile_mempool;
use pulsedag_p2p::{build_p2p_stack, InboundEvent, Libp2pConfig, P2pHandle, P2pMode};
use pulsedag_rpc::routes::router;
use pulsedag_storage::Storage;
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = Config::from_env();
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
        let stack = if cfg.p2p_mode.as_str() == "libp2p" {
            build_p2p_stack(P2pMode::Libp2p(Libp2pConfig {
                chain_id: cfg.chain_id.clone(),
                listen_addr: cfg.p2p_listen.clone(),
                bootstrap: cfg.p2p_bootstrap.clone(),
                enable_mdns: cfg.p2p_mdns,
                enable_kademlia: cfg.p2p_kademlia,
            }))?
        } else {
            build_p2p_stack(P2pMode::Memory {
                chain_id: cfg.chain_id.clone(),
                peers: cfg.simulated_peers.clone(),
            })?
        };
        (Some(stack.handle), stack.inbound_rx)
    } else {
        (None, None)
    };

    let mut runtime_stats = new_runtime_stats();
    runtime_stats.startup_snapshot_exists = snapshot_exists;
    runtime_stats.startup_persisted_block_count = persisted_blocks.len();
    runtime_stats.startup_persisted_max_height = startup_persisted_max_height;
    runtime_stats.startup_consistency_issue_count = startup_consistency_issue_count;
    runtime_stats.startup_recovery_mode = startup_recovery_mode.clone();
    runtime_stats.startup_rebuild_reason = startup_rebuild_reason.clone();
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

    let app_state = AppState {
        chain: Arc::new(tokio::sync::RwLock::new(chain_state)),
        storage: storage.clone(),
        p2p,
        runtime: Arc::new(tokio::sync::RwLock::new(runtime_stats)),
    };

    {
        let summary = if startup_consistency_issue_count == 0 {
            format!("startup audit ok; recovery_mode={}", startup_recovery_mode)
        } else {
            format!(
                "startup audit found {} consistency issues; recovery_mode={}",
                startup_consistency_issue_count, startup_recovery_mode
            )
        };
        let _ = app_state
            .storage
            .append_runtime_event("info", "startup_audit", &summary);
        if let Some(reason) = startup_rebuild_reason.clone() {
            let _ = app_state
                .storage
                .append_runtime_event("warn", "startup_rebuild", &reason);
        }
    }

    if let Some(mut rx) = inbound_rx {
        let chain = app_state.chain.clone();
        let storage = storage.clone();
        let runtime = app_state.runtime.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    InboundEvent::Transaction(tx) => {
                        let mut guard = chain.write().await;
                        let already_in_mempool = guard.mempool.transactions.contains_key(&tx.txid);
                        let already_confirmed = guard.dag.blocks.values().any(|block| {
                            block.transactions.iter().any(|known| known.txid == tx.txid)
                        });
                        if already_in_mempool || already_confirmed {
                            let mut rt = runtime.write().await;
                            rt.duplicate_p2p_txs += 1;
                            info!(txid = %tx.txid, already_in_mempool, already_confirmed, "ignored duplicate inbound p2p transaction");
                            continue;
                        }
                        if let Err(e) = accept_transaction(tx, &mut guard, AcceptSource::P2p) {
                            let mut rt = runtime.write().await;
                            rt.rejected_p2p_txs += 1;
                            warn!(error = %e, "rejected inbound p2p transaction");
                        } else if let Err(e) = storage.persist_chain_state(&guard) {
                            warn!(error = %e, "failed persisting chain state after inbound transaction");
                        } else {
                            let mut rt = runtime.write().await;
                            rt.accepted_p2p_txs += 1;
                        }
                    }
                    InboundEvent::Block(block) => {
                        let mut guard = chain.write().await;
                        if guard.dag.blocks.contains_key(&block.hash)
                            || guard.orphan_blocks.contains_key(&block.hash)
                        {
                            let mut rt = runtime.write().await;
                            rt.duplicate_p2p_blocks += 1;
                            info!(block = %block.hash, "ignored duplicate inbound p2p block");
                            continue;
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
                            }
                            info!(block = %block.hash, missing_parents = ?missing_parents, orphan_count = guard.orphan_blocks.len(), pruned, "queued inbound p2p orphan block");
                            if let Err(e) = storage.persist_block(&block) {
                                warn!(error = %e, "failed persisting inbound orphan block");
                            }
                            if let Err(e) = storage.persist_chain_state(&guard) {
                                warn!(error = %e, "failed persisting chain state after orphan queue");
                            }
                        } else if let Err(e) =
                            accept_block(block.clone(), &mut guard, AcceptSource::P2p)
                        {
                            let mut rt = runtime.write().await;
                            rt.rejected_p2p_blocks += 1;
                            warn!(error = %e, "rejected inbound p2p block");
                        } else {
                            let adopted =
                                pulsedag_core::adopt_ready_orphans(&mut guard, AcceptSource::P2p);
                            {
                                let mut rt = runtime.write().await;
                                rt.accepted_p2p_blocks += 1;
                                rt.adopted_orphan_blocks += adopted as u64;
                            }
                            if adopted > 0 {
                                info!(
                                    adopted,
                                    remaining_orphans = guard.orphan_blocks.len(),
                                    "adopted ready orphan blocks after inbound block"
                                );
                            }
                            if let Err(e) = storage.persist_block(&block) {
                                warn!(error = %e, "failed persisting inbound block");
                            }
                            if let Err(e) = storage.persist_chain_state(&guard) {
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
                let chain_id = chain_snapshot.chain_id.clone();
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
                    prune_require_snapshot,
                    last_prune_height,
                ) = {
                    let rt = runtime.read().await;
                    (
                        rt.snapshot_auto_every_blocks,
                        rt.last_snapshot_height,
                        rt.auto_prune_enabled,
                        rt.auto_prune_every_blocks,
                        rt.prune_keep_recent_blocks.max(1),
                        rt.prune_require_snapshot,
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
                        let snapshot_validated = match storage.load_chain_state() {
                            Ok(Some(snapshot)) => snapshot.dag.best_height >= keep_from_height,
                            Ok(None) => false,
                            Err(_) => false,
                        };
                        if prune_require_snapshot && !snapshot_validated {
                            warn!(
                                best_height,
                                keep_from_height, "auto prune skipped; valid snapshot required"
                            );
                            let _ = storage.append_runtime_event(
                                "warn",
                                "prune_auto_skipped",
                                &format!(
                                    "auto prune skipped at height {}: snapshot missing or below {}",
                                    best_height, keep_from_height
                                ),
                            );
                            continue;
                        }

                        match storage.prune_blocks_below_height(keep_from_height) {
                            Ok(pruned) => match storage.replay_blocks_or_init(chain_id.clone()) {
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
                                        "auto prune completed and replay verified"
                                    );
                                    let _ = storage.append_runtime_event("info", "prune_auto", &format!("auto prune removed {} blocks below {}; replay verified at height {}", pruned, keep_from_height, rebuilt.dag.best_height));
                                }
                                Err(e) => {
                                    warn!(error = %e, best_height, keep_from_height, "auto prune replay verification failed");
                                    let _ = storage.append_runtime_event(
                                        "error",
                                        "prune_auto_failed",
                                        &format!(
                                            "auto prune replay verify failed at height {}: {}",
                                            best_height, e
                                        ),
                                    );
                                }
                            },
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
