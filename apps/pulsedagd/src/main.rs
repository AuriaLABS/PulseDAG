mod app_state;
mod block_request;
mod config;

use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use app_state::{
    build_operator_console_rollup, build_startup_lifecycle_events, derive_startup_path_report,
    new_runtime_stats, short_hash, AppState, OperatorConsoleInputs,
};
use axum::Router;
use block_request::{BlockRequestTracker, DependencyAwareFetchScheduler, HeaderFetchCandidate};
use config::Config;
use pulsedag_core::accept::{
    accept_transaction_with_result, AcceptSource, BlockAcceptanceResult, TxAcceptanceResult,
};
use pulsedag_core::reconcile_mempool;
use pulsedag_p2p::{
    build_p2p_stack, messages::HeaderInventory, InboundEvent, Libp2pConfig, Libp2pRuntimeMode,
    P2pHandle, P2pMode,
};
use pulsedag_rpc::routes::{
    router_with_profile, ApiExposureProfile as RpcApiExposureProfile, RateLimitConfig,
    RpcHardeningLimits,
};
use pulsedag_storage::Storage;
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

const MAX_INFLIGHT_BLOCK_REQUESTS: usize = 64;
const MAX_INFLIGHT_BLOCK_REQUESTS_PER_PEER: usize = 16;
const MAX_FETCH_SCHEDULER_QUEUE_DEPTH: usize = 512;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn headers_for_request(
    chain: &pulsedag_core::state::ChainState,
    locator: &[String],
    stop_hash: Option<&String>,
    limit: usize,
) -> Vec<HeaderInventory> {
    let limit = limit.clamp(1, 512);
    let locator_heights = locator
        .iter()
        .filter_map(|hash| chain.dag.blocks.get(hash).map(|block| block.header.height))
        .collect::<Vec<_>>();
    let start_height = locator_heights.into_iter().max().unwrap_or(0);
    let mut blocks = chain.dag.blocks.values().cloned().collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.hash.cmp(&b.hash))
    });
    let mut headers = Vec::new();
    for block in blocks {
        if block.header.height <= start_height && !locator.is_empty() {
            continue;
        }
        headers.push(HeaderInventory {
            hash: block.hash.clone(),
            header: block.header.clone(),
        });
        if Some(&block.hash) == stop_hash || headers.len() >= limit {
            break;
        }
    }
    headers
}

fn known_hashes_for_scheduler(chain: &pulsedag_core::state::ChainState) -> HashSet<String> {
    chain.dag.blocks.keys().cloned().collect()
}

fn pending_hashes_for_scheduler(block_requests: &BlockRequestTracker) -> HashSet<String> {
    block_requests.pending.keys().cloned().collect()
}

fn orphan_age_metrics(chain: &pulsedag_core::state::ChainState, now_unix: u64) -> (u64, u64) {
    let now_ms = now_unix.saturating_mul(1_000);
    let max_orphan_age_secs = chain
        .orphan_received_at_ms
        .values()
        .map(|received_ms| now_ms.saturating_sub(*received_ms) / 1_000)
        .max()
        .unwrap_or(0);
    let oldest_missing_parent_age_secs = chain
        .orphan_parent_index
        .values()
        .filter_map(|waiting| {
            waiting
                .iter()
                .filter_map(|orphan| chain.orphan_received_at_ms.get(orphan))
                .map(|received_ms| now_ms.saturating_sub(*received_ms) / 1_000)
                .max()
        })
        .max()
        .unwrap_or(0);
    (max_orphan_age_secs, oldest_missing_parent_age_secs)
}

fn record_orphan_reprocess_failures(
    runtime: &mut pulsedag_rpc::api::NodeRuntimeStats,
    failure_reasons: &std::collections::BTreeMap<String, usize>,
) {
    for (reason, count) in failure_reasons {
        let entry = runtime
            .orphan_reprocess_failures_by_reason
            .entry(reason.clone())
            .or_insert(0);
        *entry = entry.saturating_add(*count as u64);
        runtime.last_orphan_reprocess_failure_reason = Some(reason.clone());
    }
}

fn active_peer_ids(p2p: &Option<Arc<dyn P2pHandle>>) -> Vec<String> {
    p2p.as_ref()
        .map(active_peer_ids_from_handle)
        .unwrap_or_default()
}

fn active_peer_ids_from_handle(p2p: &Arc<dyn P2pHandle>) -> Vec<String> {
    p2p.status()
        .map(|status| {
            status
                .active_connections_by_peer
                .into_iter()
                .filter_map(|(peer, connections)| (connections > 0).then_some(peer))
                .collect()
        })
        .unwrap_or_default()
}

fn usage() -> &'static str {
    "usage: pulsedagd [--network dev|testnet|mainnet] [--rpc-listen HOST:PORT] [--p2p-listen MULTIADDR] [--bootnode MULTIADDR] [--peer MULTIADDR] [--help] [--version]"
}

fn print_help_and_exit() {
    println!("{}", usage());
}

fn print_version_and_exit() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    if cli_args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        print_help_and_exit();
        return Ok(());
    }
    if cli_args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--version" | "-V"))
    {
        print_version_and_exit();
        return Ok(());
    }

    let startup_begin = std::time::Instant::now();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut cfg = Config::from_env()?;
    cfg.apply_cli_args(cli_args)?;
    let config_safety_summary = cfg.config_safety_summary();
    if config_safety_summary.contains("warning") {
        warn!(summary = %config_safety_summary, "config safety summary");
    } else {
        info!(summary = %config_safety_summary, "config safety summary");
    }
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

    let genesis_hash = chain_state
        .dag
        .blocks
        .values()
        .find(|b| b.header.height == 0)
        .map(|b| b.hash.clone())
        .unwrap_or_else(|| "unknown".to_string());

    info!(
        version = env!("CARGO_PKG_VERSION"),
        network_profile = %cfg.network_profile,
        chain_id = %cfg.chain_id,
        p2p_enabled = cfg.p2p_enabled,
        p2p_mode = %cfg.p2p_mode,
        p2p_bind = %cfg.p2p_listen,
        rpc_bind = %cfg.rpc_bind,
        data_dir = %cfg.rocksdb_path,
        p2p_bootstrap = ?cfg.p2p_bootstrap,
        genesis_hash = %genesis_hash,
        "node startup identity"
    );
    // Export resolved runtime settings so diagnostics/readiness report effective values
    // even when config was provided by profile files or CLI flags.
    std::env::set_var("PULSEDAG_EFFECTIVE_RPC_BIND", cfg.rpc_bind.clone());
    std::env::set_var("PULSEDAG_RPC_BIND", cfg.rpc_bind.clone());
    std::env::set_var("PULSEDAG_API_PROFILE", cfg.api_profile.as_env_value());

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
            let mut block_requests = BlockRequestTracker::with_limits(
                8,
                2,
                MAX_INFLIGHT_BLOCK_REQUESTS,
                MAX_INFLIGHT_BLOCK_REQUESTS_PER_PEER,
            );
            let mut fetch_scheduler =
                DependencyAwareFetchScheduler::with_limit(MAX_FETCH_SCHEDULER_QUEUE_DEPTH);
            let mut recovery_tick: u64 = 0;
            loop {
                let maybe_event =
                    match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
                        Ok(Some(event)) => Some(event),
                        Ok(None) => break,
                        Err(_) => None,
                    };
                let now = now_unix();
                let timed_out = block_requests.drain_timeouts(now);
                let timed_out_count = timed_out
                    .retryable
                    .len()
                    .saturating_add(timed_out.expired.len());
                if timed_out_count > 0 {
                    {
                        let mut rt = runtime.write().await;
                        rt.block_request_timeouts = rt
                            .block_request_timeouts
                            .saturating_add(timed_out_count as u64);
                        rt.missing_parent_request_timeouts = rt
                            .missing_parent_request_timeouts
                            .saturating_add(timed_out_count as u64);
                        rt.block_request_retries = rt
                            .block_request_retries
                            .saturating_add(timed_out.retryable.len() as u64);
                        rt.missing_parent_request_retries = rt
                            .missing_parent_request_retries
                            .saturating_add(timed_out.retryable.len() as u64);
                        rt.block_request_fallbacks = rt
                            .block_request_fallbacks
                            .saturating_add(timed_out.expired.len() as u64);
                        rt.missing_parent_request_fallbacks = rt
                            .missing_parent_request_fallbacks
                            .saturating_add(timed_out.expired.len() as u64);
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                        rt.block_fetch_scheduler_queue_depth = fetch_scheduler.queue_depth();
                        rt.block_fetch_scheduler_inflight_by_peer =
                            block_requests.inflight_by_peer();
                    }
                    for hash in timed_out.retryable {
                        if let Some(ref p2p) = p2p {
                            if let Err(e) = p2p.request_block(&hash) {
                                warn!(error = %e, block_hash = %hash, "failed retrying timed-out GetBlock request");
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                        rt.missing_parent_requests_sent =
                            rt.missing_parent_requests_sent.saturating_add(1);
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                    }
                    for hash in timed_out.expired {
                        let missing_parent_still_needed = {
                            let guard = chain.read().await;
                            guard.orphan_parent_index.contains_key(&hash)
                                && !guard.dag.blocks.contains_key(&hash)
                        };
                        if missing_parent_still_needed
                            && block_requests.should_issue_getblock_for_peers(
                                &hash,
                                now_unix(),
                                active_peer_ids(&p2p),
                            )
                        {
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_block(&hash) {
                                    warn!(error = %e, block_hash = %hash, "failed restarting expired missing-parent GetBlock request");
                                }
                            }
                            let mut rt = runtime.write().await;
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                            rt.missing_parent_requests_sent =
                                rt.missing_parent_requests_sent.saturating_add(1);
                            rt.pending_block_requests = block_requests.pending.len();
                            rt.inflight_block_requests = block_requests.pending.len();
                            rt.pending_block_request_hashes = block_requests.pending_hashes();
                        } else {
                            warn!(block_hash = %hash, "GetBlock request expired after retry limit; clearing inflight state");
                        }
                    }
                }
                recovery_tick = recovery_tick.saturating_add(1);
                if recovery_tick.is_multiple_of(5) {
                    let (
                        stale_missing_parents,
                        adopted,
                        retried,
                        orphan_count,
                        pending_missing,
                        ages,
                        persist_failed,
                        failure_reasons,
                    ) = {
                        let mut guard = chain.write().await;
                        let known = guard.dag.blocks.keys().cloned().collect::<HashSet<_>>();
                        let mut missing = guard
                            .orphan_parent_index
                            .keys()
                            .filter(|parent| !known.contains(*parent))
                            .cloned()
                            .collect::<Vec<_>>();
                        missing.sort();
                        missing.truncate(16);
                        let mut adopted_hashes = Vec::new();
                        let adoption = pulsedag_core::adopt_ready_orphans_with_result(
                            &mut guard,
                            AcceptSource::P2p,
                            None,
                        );
                        let adopted = adoption.accepted;
                        let retried = adoption.retried;
                        let failure_reasons = adoption.failure_reasons;
                        adopted_hashes.extend(adoption.accepted_hashes);
                        let persist_failed = if retried > 0 {
                            match storage.persist_chain_state(&guard) {
                                Ok(()) => false,
                                Err(e) => {
                                    warn!(error = %e, retried, adopted, "failed persisting chain state after recovery orphan reprocess");
                                    true
                                }
                            }
                        } else {
                            false
                        };
                        for hash in adopted_hashes {
                            block_requests.resolve(&hash);
                        }
                        let ages = orphan_age_metrics(&guard, now_unix());
                        (
                            missing,
                            adopted,
                            retried,
                            guard.orphan_blocks.len(),
                            pulsedag_core::pending_missing_parent_count(&guard),
                            ages,
                            persist_failed,
                            failure_reasons,
                        )
                    };
                    if retried > 0 || !stale_missing_parents.is_empty() || pending_missing > 0 {
                        let mut rt = runtime.write().await;
                        rt.orphan_reprocess_attempts =
                            rt.orphan_reprocess_attempts.saturating_add(retried as u64);
                        rt.orphan_reprocess_success =
                            rt.orphan_reprocess_success.saturating_add(adopted as u64);
                        rt.orphan_reprocess_failed_missing_parent =
                            rt.orphan_reprocess_failed_missing_parent.saturating_add(
                                failure_reasons.get("missing_parent").copied().unwrap_or(0) as u64,
                            );
                        record_orphan_reprocess_failures(&mut rt, &failure_reasons);
                        if retried == 0 && pending_missing > 0 {
                            let entry = rt
                                .orphan_reprocess_failures_by_reason
                                .entry("waiting_missing_parent".to_string())
                                .or_insert(0);
                            *entry = entry.saturating_add(pending_missing as u64);
                            rt.last_orphan_reprocess_failure_reason =
                                Some("waiting_missing_parent".to_string());
                        }
                        rt.orphan_blocks_retried =
                            rt.orphan_blocks_retried.saturating_add(retried as u64);
                        rt.orphan_blocks_resolved =
                            rt.orphan_blocks_resolved.saturating_add(adopted as u64);
                        if persist_failed {
                            rt.orphan_reprocess_failed_persist =
                                rt.orphan_reprocess_failed_persist.saturating_add(1);
                        }
                        rt.pending_missing_parents = pending_missing;
                        rt.max_orphan_age_secs = ages.0;
                        rt.oldest_orphan_age_secs = ages.0;
                        rt.oldest_missing_parent_age_secs = ages
                            .1
                            .max(block_requests.oldest_pending_age_secs(now_unix()));
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                        rt.block_fetch_scheduler_queue_depth = fetch_scheduler.queue_depth();
                        rt.block_fetch_scheduler_inflight_by_peer =
                            block_requests.inflight_by_peer();
                        rt.sync_state = if orphan_count == 0 {
                            "synced"
                        } else {
                            "catching_up"
                        }
                        .to_string();
                    }
                    for parent in stale_missing_parents {
                        if block_requests.should_issue_getblock_for_peers(
                            &parent,
                            now_unix(),
                            active_peer_ids(&p2p),
                        ) {
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_block(&parent) {
                                    warn!(error = %e, missing_parent = %parent, "failed issuing recovery-tick missing-parent GetBlock request");
                                }
                            }
                            let mut rt = runtime.write().await;
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                            rt.missing_parent_requests_sent =
                                rt.missing_parent_requests_sent.saturating_add(1);
                            rt.block_request_fallbacks =
                                rt.block_request_fallbacks.saturating_add(1);
                            rt.missing_parent_request_fallbacks =
                                rt.missing_parent_request_fallbacks.saturating_add(1);
                            rt.pending_block_requests = block_requests.pending.len();
                            rt.inflight_block_requests = block_requests.pending.len();
                            rt.pending_block_request_hashes = block_requests.pending_hashes();
                        }
                    }
                }
                let Some(event) = maybe_event else {
                    let counters = block_requests.take_fetch_counters();
                    if counters.suppressed > 0 || counters.queued > 0 || counters.dropped > 0 {
                        let mut rt = runtime.write().await;
                        rt.block_request_backpressure_suppressed = rt
                            .block_request_backpressure_suppressed
                            .saturating_add(counters.suppressed);
                        rt.block_request_fetches_queued = rt
                            .block_request_fetches_queued
                            .saturating_add(counters.queued);
                        rt.block_request_fetches_dropped = rt
                            .block_request_fetches_dropped
                            .saturating_add(counters.dropped);
                        rt.block_fetch_duplicate_inflight_suppressed = rt
                            .block_fetch_duplicate_inflight_suppressed
                            .saturating_add(counters.suppressed);
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                        warn!(
                            suppressed = counters.suppressed,
                            queued = counters.queued,
                            dropped = counters.dropped,
                            max_pending_block_requests = block_requests.max_pending(),
                            max_pending_block_requests_per_peer =
                                block_requests.max_pending_per_peer(),
                            pending_block_requests = block_requests.pending.len(),
                            "suppressed GetBlock requests due to bounded inflight backpressure"
                        );
                    }
                    continue;
                };
                match event {
                    InboundEvent::Transaction(tx) => {
                        let txid = tx.txid.clone();
                        {
                            let mut rt = runtime.write().await;
                            rt.tx_inbound_total += 1;
                            rt.tx_inbound_received = rt.tx_inbound_received.saturating_add(1);
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
                            rt.tx_inbound_duplicate = rt.tx_inbound_duplicate.saturating_add(1);
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
                        let acceptance =
                            accept_transaction_with_result(tx, &mut guard, AcceptSource::P2p);
                        match acceptance {
                            TxAcceptanceResult::Duplicate => {
                                let mut rt = runtime.write().await;
                                rt.duplicate_p2p_txs += 1;
                                rt.tx_inbound_duplicate = rt.tx_inbound_duplicate.saturating_add(1);
                                rt.dropped_p2p_txs += 1;
                                rt.tx_inbound_dropped_total += 1;
                                rt.dropped_p2p_txs_duplicate_mempool += 1;
                                rt.last_tx_drop_unix = Some(now_unix());
                                rt.last_tx_drop_reason = Some("duplicate_mempool".to_string());
                                rt.last_tx_drop_txid = Some(txid.clone());
                                rt.tx_drop_reasons
                                    .push(format!("txid={} reason=duplicate_mempool", txid));
                                if rt.tx_drop_reasons.len() > 32 {
                                    let overflow = rt.tx_drop_reasons.len() - 32;
                                    rt.tx_drop_reasons.drain(0..overflow);
                                }
                                continue;
                            }
                            TxAcceptanceResult::Invalid(reason)
                            | TxAcceptanceResult::Rejected(reason) => {
                                let mut rt = runtime.write().await;
                                rt.rejected_p2p_txs += 1;
                                rt.tx_inbound_invalid = rt.tx_inbound_invalid.saturating_add(1);
                                rt.dropped_p2p_txs += 1;
                                rt.tx_inbound_rejected_total += 1;
                                rt.tx_inbound_dropped_total += 1;
                                rt.dropped_p2p_txs_accept_failed += 1;
                                let now = now_unix();
                                rt.last_tx_reject_unix = Some(now);
                                rt.last_tx_drop_unix = Some(now);
                                rt.last_tx_drop_reason = Some("accept_failed".to_string());
                                rt.last_tx_drop_txid = Some(txid.clone());
                                rt.tx_drop_reasons.push(format!(
                                    "txid={} reason=accept_failed error={}",
                                    txid, reason
                                ));
                                if rt.tx_drop_reasons.len() > 32 {
                                    let overflow = rt.tx_drop_reasons.len() - 32;
                                    rt.tx_drop_reasons.drain(0..overflow);
                                }
                                warn!(txid = %txid, error = %reason, "rejected inbound p2p transaction");
                                let _ = storage.append_runtime_event(
                                    "warn",
                                    "tx_reject",
                                    &format!("txid={} reason=accept_failed error={}", txid, reason),
                                );
                                continue;
                            }
                            TxAcceptanceResult::Orphan => {
                                let snapshot = guard.clone();
                                drop(guard);
                                let mut rt = runtime.write().await;
                                rt.tx_inbound_dropped_total += 1;
                                rt.last_tx_drop_unix = Some(now_unix());
                                rt.last_tx_drop_reason = Some("orphan".to_string());
                                rt.last_tx_drop_txid = Some(txid.clone());
                                rt.tx_drop_reasons
                                    .push(format!("txid={} reason=orphan", txid));
                                if rt.tx_drop_reasons.len() > 32 {
                                    let overflow = rt.tx_drop_reasons.len() - 32;
                                    rt.tx_drop_reasons.drain(0..overflow);
                                }
                                let _ = storage.persist_chain_state(&snapshot);
                                info!(txid = %txid, "tracked orphan inbound p2p transaction without rebroadcast");
                                continue;
                            }
                            TxAcceptanceResult::Accepted => {
                                let tx_for_rebroadcast =
                                    guard.mempool.transactions.get(&txid).cloned();
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
                                    rt.tx_inbound_accepted =
                                        rt.tx_inbound_accepted.saturating_add(1);
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
                                                    &format!(
                                                        "txid={} reason={}",
                                                        txid, skip_reason
                                                    ),
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
                                                        rt.tx_relayed =
                                                            rt.tx_relayed.saturating_add(1);
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
                    }
                    InboundEvent::BlockAnnouncement { hash } => {
                        info!(event = "block_announced", block_hash = %hash, "block announced by peer");
                        let known = {
                            let guard = chain.read().await;
                            guard.dag.blocks.contains_key(&hash)
                        };
                        let mut rt = runtime.write().await;
                        rt.block_announces_received = rt.block_announces_received.saturating_add(1);
                        drop(rt);
                        if known {
                            info!(event = "duplicate_block_ignored", block_hash = %hash, "duplicate block announcement ignored");
                        } else {
                            fetch_scheduler.queue_inventory([hash.clone()]);
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_headers(&[], Some(&hash), 128) {
                                    warn!(error = %e, block_hash = %hash, "failed issuing GetHeaders request");
                                }
                            }
                            let (known, pending) = {
                                let guard = chain.read().await;
                                (
                                    known_hashes_for_scheduler(&guard),
                                    pending_hashes_for_scheduler(&block_requests),
                                )
                            };
                            let plan = fetch_scheduler.next_requests(&known, &pending, 8);
                            for request_hash in plan.requests {
                                if block_requests.should_issue_getblock_for_peers(
                                    &request_hash,
                                    now_unix(),
                                    active_peer_ids(&p2p),
                                ) {
                                    if let Some(ref p2p) = p2p {
                                        if let Err(e) = p2p.request_block(&request_hash) {
                                            warn!(error = %e, block_hash = %request_hash, "failed issuing dependency-aware GetBlock request");
                                        }
                                    }
                                    let mut rt = runtime.write().await;
                                    rt.sync_state = "requesting_blocks".to_string();
                                    rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                    rt.header_requests_sent =
                                        rt.header_requests_sent.saturating_add(1);
                                    rt.dependency_fetches_scheduled =
                                        rt.dependency_fetches_scheduled.saturating_add(1);
                                    rt.parent_first_fetches = rt
                                        .parent_first_fetches
                                        .saturating_add(plan.parent_first_requests as u64);
                                    rt.pending_block_requests = block_requests.pending.len();
                                }
                            }
                        }
                    }
                    InboundEvent::Block(block) => {
                        let fulfilled_missing_parent_request =
                            block_requests.pending.contains_key(&block.hash);
                        {
                            let mut rt = runtime.write().await;
                            let now = now_unix();
                            rt.sync_pipeline.begin_cycle(now);
                            rt.sync_pipeline.observe_peer_candidate(now);
                        }
                        let mut guard = chain.write().await;
                        {
                            let mut rt = runtime.write().await;
                            let now = now_unix();
                            rt.sync_pipeline.observe_headers(1, now);
                            rt.sync_pipeline.request_blocks(1, now);
                            rt.sync_pipeline.acquire_blocks(1);
                            if fulfilled_missing_parent_request {
                                rt.missing_parent_responses_received =
                                    rt.missing_parent_responses_received.saturating_add(1);
                            }
                        }
                        info!(event = "peer_block_received", block_hash = %block.hash, parent_count = block.header.parents.len(), "received inbound p2p block payload");
                        let _ = storage.append_runtime_event(
                            "info",
                            "peer_block_received",
                            &format!("hash={} parents={}", block.hash, block.header.parents.len()),
                        );
                        let acceptance = match pulsedag_core::accept_block_atomically(
                            block.clone(),
                            &mut guard,
                            AcceptSource::P2p,
                            |block, chain| storage.persist_block_and_chain_state(block, chain),
                            |block| {
                                if let Some(ref p2p) = p2p {
                                    p2p.broadcast_block(block)?;
                                }
                                Ok(())
                            },
                        ) {
                            Ok(acceptance) => acceptance.result,
                            Err(e) => {
                                warn!(error = %e, block_hash = %block.hash, "failed durable commit for inbound block before memory commit");
                                let _ = storage.append_runtime_event(
                                    "warn",
                                    "peer_block_persist_failed",
                                    &format!("hash={} error={}", block.hash, e),
                                );
                                BlockAcceptanceResult::Rejected(e.to_string())
                            }
                        };
                        if matches!(acceptance, BlockAcceptanceResult::MissingParent) {
                            let mut rt = runtime.write().await;
                            rt.blockdata_received = rt.blockdata_received.saturating_add(1);
                            rt.blockdata_missing_parent =
                                rt.blockdata_missing_parent.saturating_add(1);
                            drop(rt);
                            let missing_parents =
                                pulsedag_core::missing_block_parents(&block, &guard);
                            let orphan_queue = pulsedag_core::queue_orphan_block_bounded(
                                &mut guard,
                                block.clone(),
                                missing_parents.clone(),
                                pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                                pulsedag_core::DEFAULT_ORPHAN_MAX_AGE_MS,
                            );
                            let pruned = orphan_queue.evicted;
                            let ages = orphan_age_metrics(&guard, now_unix());
                            {
                                let mut rt = runtime.write().await;
                                rt.sync_state = "catching_up".to_string();
                                rt.queued_orphan_blocks += 1;
                                if orphan_queue.queued {
                                    rt.orphan_blocks_queued =
                                        rt.orphan_blocks_queued.saturating_add(1);
                                }
                                rt.orphan_blocks_evicted =
                                    rt.orphan_blocks_evicted.saturating_add(pruned as u64);
                                rt.missing_parents_detected = rt
                                    .missing_parents_detected
                                    .saturating_add(missing_parents.len() as u64);
                                rt.pulsedag_blocks_rejected_total =
                                    rt.pulsedag_blocks_rejected_total.saturating_add(1);
                                rt.record_rejected_block_reason("missing_parent");
                                rt.pulsedag_sync_missing_parents_total = rt
                                    .pulsedag_sync_missing_parents_total
                                    .saturating_add(missing_parents.len() as u64);
                                rt.pending_missing_parents =
                                    pulsedag_core::pending_missing_parent_count(&guard);
                                rt.max_orphan_age_secs = ages.0;
                                rt.oldest_orphan_age_secs = ages.0;
                                rt.oldest_missing_parent_age_secs = ages
                                    .1
                                    .max(block_requests.oldest_pending_age_secs(now_unix()));
                                rt.last_rejected_peer_block_reason = Some(format!(
                                    "missing parents for {}: {:?}",
                                    block.hash, missing_parents
                                ));
                                rt.sync_pipeline.fallback_after_failure(
                                    format!(
                                        "orphaned block {} missing parents {:?}",
                                        block.hash, missing_parents
                                    ),
                                    now_unix(),
                                );
                            }
                            info!(event = "peer_block_missing_parent", block_hash = %block.hash, missing_parents = ?missing_parents, orphan_count = guard.orphan_blocks.len(), "queued inbound p2p orphan block");
                            let _ = storage.append_runtime_event(
                                "warn",
                                "peer_block_missing_parent",
                                &format!(
                                    "hash={} missing_parents={:?}",
                                    block.hash, missing_parents
                                ),
                            );
                            for parent in &missing_parents {
                                if block_requests.should_issue_getblock_for_peers(
                                    parent,
                                    now_unix(),
                                    active_peer_ids(&p2p),
                                ) {
                                    if let Some(ref p2p) = p2p {
                                        if let Err(e) = p2p.request_block(parent) {
                                            warn!(error = %e, missing_parent = %parent, "failed issuing missing-parent GetBlock request");
                                        }
                                    }
                                    let mut rt = runtime.write().await;
                                    rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                    rt.missing_parent_requests_sent =
                                        rt.missing_parent_requests_sent.saturating_add(1);
                                    rt.pending_block_requests = block_requests.pending.len();
                                    rt.inflight_block_requests = block_requests.pending.len();
                                    rt.pending_block_request_hashes =
                                        block_requests.pending_hashes();
                                    info!(event = "missing_block_requested", missing_parent = %parent, child = %block.hash, "missing parent discovered; GetBlock request emitted");
                                } else {
                                    let mut rt = runtime.write().await;
                                    rt.duplicate_block_requests_suppressed =
                                        rt.duplicate_block_requests_suppressed.saturating_add(1);
                                    rt.pending_block_requests = block_requests.pending.len();
                                    rt.inflight_block_requests = block_requests.pending.len();
                                    rt.pending_block_request_hashes =
                                        block_requests.pending_hashes();
                                }
                            }
                            if pruned > 0 {
                                warn!(
                                    event = "orphan_evicted",
                                    evicted = pruned,
                                    orphan_count = guard.orphan_blocks.len(),
                                    "orphan pool bounded; evicted oldest/expired entries"
                                );
                            }
                            block_requests.resolve(&block.hash);
                            {
                                let mut rt = runtime.write().await;
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                                rt.pending_missing_parents =
                                    pulsedag_core::pending_missing_parent_count(&guard);
                            }
                            if let Err(e) = storage.persist_chain_state(&guard) {
                                warn!(error = %e, "failed persisting chain state after orphan queue");
                            }
                        } else if !acceptance.is_accepted() {
                            let mut rt = runtime.write().await;
                            rt.blockdata_received = rt.blockdata_received.saturating_add(1);
                            if matches!(acceptance, BlockAcceptanceResult::Duplicate) {
                                rt.duplicate_p2p_blocks += 1;
                                rt.blockdata_duplicate = rt.blockdata_duplicate.saturating_add(1);
                            } else {
                                rt.sync_state = "degraded".to_string();
                                rt.sync_failures = rt.sync_failures.saturating_add(1);
                                rt.rejected_p2p_blocks += 1;
                                rt.last_rejected_peer_block_reason =
                                    Some(format!("{}: {:?}", block.hash, acceptance));
                                rt.pulsedag_blocks_rejected_total =
                                    rt.pulsedag_blocks_rejected_total.saturating_add(1);
                                rt.record_rejected_block_reason(format!("{:?}", acceptance));
                                if matches!(acceptance, BlockAcceptanceResult::InvalidPow) {
                                    rt.pulsedag_invalid_pow_total =
                                        rt.pulsedag_invalid_pow_total.saturating_add(1);
                                    rt.blockdata_invalid_pow =
                                        rt.blockdata_invalid_pow.saturating_add(1);
                                }
                            }
                            if !matches!(acceptance, BlockAcceptanceResult::Duplicate) {
                                rt.sync_pipeline.fallback_after_failure(
                                    format!(
                                        "block {} validation failed: {:?}",
                                        block.hash, acceptance
                                    ),
                                    now_unix(),
                                );
                            }
                            match acceptance {
                                BlockAcceptanceResult::Duplicate => {
                                    info!(event = "peer_block_duplicate", block_hash = %block.hash, "suppressed duplicate inbound p2p block");
                                    let _ = storage.append_runtime_event(
                                        "info",
                                        "peer_block_duplicate",
                                        &format!("hash={}", block.hash),
                                    );
                                }
                                _ => {
                                    warn!(event = "peer_block_rejected", outcome = ?acceptance, block_hash = %block.hash, "rejected inbound p2p block");
                                    let _ = storage.append_runtime_event(
                                        "warn",
                                        "peer_block_rejected",
                                        &format!("hash={} outcome={:?}", block.hash, acceptance),
                                    );
                                }
                            }
                        } else {
                            let (adopted, retried_orphans, adopted_hashes, failure_reasons) = {
                                let mut adopted_guard = guard.clone();
                                let adoption = pulsedag_core::adopt_ready_orphans_with_result(
                                    &mut adopted_guard,
                                    AcceptSource::P2p,
                                    Some(&block.hash),
                                );
                                let adopted = adoption.accepted;
                                let retried = adoption.retried;
                                let failure_reasons = adoption.failure_reasons;
                                let adopted_hashes = adoption.accepted_hashes;
                                if retried > 0 {
                                    match storage.persist_chain_state(&adopted_guard) {
                                        Ok(()) => {
                                            *guard = adopted_guard;
                                            (adopted, retried, adopted_hashes, failure_reasons)
                                        }
                                        Err(e) => {
                                            warn!(error = %e, block_hash = %block.hash, adopted, "failed persisting chain state after inbound orphan adoption; keeping orphans queued in memory");
                                            let _ = storage.append_runtime_event(
                                                "warn",
                                                "peer_orphan_adoption_persist_failed",
                                                &format!(
                                                    "hash={} adopted_orphans={} error={}",
                                                    block.hash, adopted, e
                                                ),
                                            );
                                            (0, 0, Vec::new(), failure_reasons)
                                        }
                                    }
                                } else {
                                    (0, 0, Vec::new(), failure_reasons)
                                }
                            };
                            let ages = orphan_age_metrics(&guard, now_unix());
                            let accepted_height = guard.dag.best_height;
                            let accepted_tip = pulsedag_core::preferred_tip_hash(&guard)
                                .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                            {
                                let mut rt = runtime.write().await;
                                rt.sync_pipeline.validate_and_apply_blocks(1, now_unix());
                                rt.accepted_p2p_blocks += 1;
                                rt.blockdata_received = rt.blockdata_received.saturating_add(1);
                                rt.blockdata_accepted = rt.blockdata_accepted.saturating_add(1);
                                rt.pulsedag_blocks_accepted_total =
                                    rt.pulsedag_blocks_accepted_total.saturating_add(1);
                                rt.adopted_orphan_blocks += adopted as u64;
                                rt.orphan_blocks_resolved =
                                    rt.orphan_blocks_resolved.saturating_add(adopted as u64);
                                rt.orphan_blocks_retried = rt
                                    .orphan_blocks_retried
                                    .saturating_add(retried_orphans as u64);
                                rt.orphan_reprocess_attempts = rt
                                    .orphan_reprocess_attempts
                                    .saturating_add(retried_orphans as u64);
                                rt.orphan_reprocess_success =
                                    rt.orphan_reprocess_success.saturating_add(adopted as u64);
                                rt.orphan_reprocess_failed_missing_parent =
                                    rt.orphan_reprocess_failed_missing_parent.saturating_add(
                                        failure_reasons.get("missing_parent").copied().unwrap_or(0)
                                            as u64,
                                    );
                                record_orphan_reprocess_failures(&mut rt, &failure_reasons);
                                rt.last_accepted_peer_block = Some(block.hash.clone());
                                rt.pending_missing_parents =
                                    pulsedag_core::pending_missing_parent_count(&guard);
                                rt.max_orphan_age_secs = ages.0;
                                rt.oldest_orphan_age_secs = ages.0;
                                rt.oldest_missing_parent_age_secs = ages
                                    .1
                                    .max(block_requests.oldest_pending_age_secs(now_unix()));
                                rt.sync_state = if guard.orphan_blocks.is_empty() {
                                    "synced"
                                } else {
                                    "catching_up"
                                }
                                .to_string();
                                if guard.orphan_blocks.is_empty() {
                                    rt.sync_catchup_completed =
                                        rt.sync_catchup_completed.saturating_add(1);
                                }
                                rt.sync_pipeline.complete_cycle(now_unix());
                            }
                            if adopted > 0 {
                                info!(
                                    event = "orphan_retried",
                                    adopted,
                                    remaining_orphans = guard.orphan_blocks.len(),
                                    "retried ready orphan blocks after parent acceptance"
                                );
                                info!(
                                    event = "orphan_accepted",
                                    adopted,
                                    remaining_orphans = guard.orphan_blocks.len(),
                                    "orphan blocks accepted after retry"
                                );
                            }
                            info!(
                                event = "peer_block_accepted",
                                block_hash = %block.hash,
                                accepted_height,
                                accepted_tip = %short_hash(&accepted_tip),
                                "accepted inbound p2p block"
                            );
                            let _ = storage.append_runtime_event(
                                "info",
                                "peer_block_accepted",
                                &format!(
                                    "source=p2p hash={} height={} tip={}",
                                    block.hash,
                                    accepted_height,
                                    short_hash(&accepted_tip)
                                ),
                            );
                            block_requests.resolve(&block.hash);
                            for adopted_hash in &adopted_hashes {
                                block_requests.resolve(adopted_hash);
                            }
                            let known_blocks =
                                guard.dag.blocks.keys().cloned().collect::<HashSet<_>>();
                            let unblocked = block_requests.unblock_after_resolve(
                                &block.hash,
                                &known_blocks,
                                now_unix(),
                            );
                            {
                                let mut rt = runtime.write().await;
                                rt.block_fetch_parent_deferred = rt
                                    .block_fetch_parent_deferred
                                    .saturating_add(unblocked.deferred.len() as u64);
                                rt.block_fetch_duplicate_inflight_suppressed = rt
                                    .block_fetch_duplicate_inflight_suppressed
                                    .saturating_add(unblocked.duplicate_suppressed as u64);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                            }
                            for child_hash in unblocked.ready {
                                if let Some(ref p2p) = p2p {
                                    if let Err(e) = p2p.request_block(&child_hash) {
                                        warn!(error = %e, block_hash = %child_hash, "failed issuing unblocked child GetBlock request");
                                    }
                                }
                                let mut rt = runtime.write().await;
                                rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.sync_pipeline.request_blocks(1, now_unix());
                            }
                            if p2p.is_some() {
                                info!(event = "peer_block_rebroadcast", block_hash = %block.hash, "rebroadcasted accepted first-seen inbound p2p block after durable commit");
                                let _ = storage.append_runtime_event(
                                    "info",
                                    "peer_block_rebroadcast",
                                    &format!("hash={}", block.hash),
                                );
                            }
                        }
                    }
                    InboundEvent::BlockInventory { hashes } => {
                        fetch_scheduler.queue_inventory(hashes.clone());
                        if let Some(ref p2p) = p2p {
                            for hash in &hashes {
                                if let Err(e) = p2p.request_headers(&[], Some(hash), 128) {
                                    warn!(error = %e, block_hash = %hash, "failed issuing inventory GetHeaders request");
                                }
                            }
                        }
                        let (known, pending) = {
                            let guard = chain.read().await;
                            (
                                known_hashes_for_scheduler(&guard),
                                pending_hashes_for_scheduler(&block_requests),
                            )
                        };
                        let plan = fetch_scheduler.next_requests(&known, &pending, 8);
                        let mut issued_requests = 0u64;
                        for hash in plan.requests {
                            if block_requests.should_issue_getblock_for_peers(
                                &hash,
                                now_unix(),
                                active_peer_ids(&p2p),
                            ) {
                                if let Some(ref p2p) = p2p {
                                    if let Err(e) = p2p.request_block(&hash) {
                                        warn!(error = %e, block_hash = %hash, "failed issuing inventory GetBlock request");
                                    }
                                }
                                issued_requests = issued_requests.saturating_add(1);
                                let mut rt = runtime.write().await;
                                rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.block_fetch_scheduler_queue_depth = fetch_scheduler.queue_depth();
                        rt.block_fetch_scheduler_inflight_by_peer =
                            block_requests.inflight_by_peer();
                        rt.inventory_announces_received = rt
                            .inventory_announces_received
                            .saturating_add(hashes.len() as u64);
                        rt.header_requests_sent =
                            rt.header_requests_sent.saturating_add(hashes.len() as u64);
                        rt.dependency_fetches_scheduled = rt
                            .dependency_fetches_scheduled
                            .saturating_add(issued_requests);
                        rt.parent_first_fetches = rt
                            .parent_first_fetches
                            .saturating_add(plan.parent_first_requests as u64);
                        rt.sync_state = "requesting_blocks".to_string();
                    }
                    InboundEvent::GetHeaders {
                        locator,
                        stop_hash,
                        limit,
                    } => {
                        let headers = {
                            let guard = chain.read().await;
                            headers_for_request(&guard, &locator, stop_hash.as_ref(), limit)
                        };
                        if let Some(ref p2p) = p2p {
                            if let Err(e) = p2p.send_headers(&headers) {
                                warn!(error = %e, "failed sending Headers response");
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.header_requests_received = rt.header_requests_received.saturating_add(1);
                        rt.headers_sent = rt.headers_sent.saturating_add(headers.len() as u64);
                    }
                    InboundEvent::Headers { headers } => {
                        let candidates = headers
                            .iter()
                            .map(|item| HeaderFetchCandidate {
                                hash: item.hash.clone(),
                                parents: item.header.parents.clone(),
                                height: item.header.height,
                            })
                            .collect::<Vec<_>>();
                        fetch_scheduler.queue_headers(candidates);
                        let (known, pending) = {
                            let guard = chain.read().await;
                            (
                                known_hashes_for_scheduler(&guard),
                                pending_hashes_for_scheduler(&block_requests),
                            )
                        };
                        let plan = fetch_scheduler.next_requests(&known, &pending, 8);
                        for hash in plan.requests {
                            if block_requests.should_issue_getblock_for_peers(
                                &hash,
                                now_unix(),
                                active_peer_ids(&p2p),
                            ) {
                                if let Some(ref p2p) = p2p {
                                    if let Err(e) = p2p.request_block(&hash) {
                                        warn!(error = %e, block_hash = %hash, "failed issuing header-driven GetBlock request");
                                    }
                                }
                                let mut rt = runtime.write().await;
                                rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.block_fetch_scheduler_queue_depth = fetch_scheduler.queue_depth();
                        rt.block_fetch_scheduler_inflight_by_peer =
                            block_requests.inflight_by_peer();
                        rt.headers_received =
                            rt.headers_received.saturating_add(headers.len() as u64);
                        rt.dependency_fetches_scheduled = rt
                            .dependency_fetches_scheduled
                            .saturating_add(headers.len() as u64);
                        rt.parent_first_fetches = rt
                            .parent_first_fetches
                            .saturating_add(plan.parent_first_requests as u64);
                        rt.sync_state = "requesting_blocks".to_string();
                    }
                    InboundEvent::GetTips => {
                        let tips = {
                            let guard = chain.read().await;
                            guard.dag.tips.iter().cloned().collect::<Vec<_>>()
                        };
                        if let Some(ref p2p) = p2p {
                            if let Err(e) = p2p.send_tips(&tips) {
                                warn!(error = %e, "failed sending Tips response");
                            }
                        }
                    }
                    InboundEvent::Tips { tips } => {
                        let unknown_tips = {
                            let guard = chain.read().await;
                            tips.into_iter()
                                .filter(|tip| !guard.dag.blocks.contains_key(tip))
                                .collect::<Vec<_>>()
                        };
                        {
                            let mut rt = runtime.write().await;
                            rt.tips_received = rt.tips_received.saturating_add(1);
                            rt.unknown_tips_seen = rt
                                .unknown_tips_seen
                                .saturating_add(unknown_tips.len() as u64);
                            rt.sync_state = if unknown_tips.is_empty() {
                                "synced"
                            } else {
                                "requesting_blocks"
                            }
                            .to_string();
                        }
                        for tip in unknown_tips {
                            if block_requests.should_issue_getblock_for_peers(
                                &tip,
                                now_unix(),
                                active_peer_ids(&p2p),
                            ) {
                                if let Some(ref p2p) = p2p {
                                    if let Err(e) = p2p.request_block(&tip) {
                                        warn!(error = %e, block_hash = %tip, "failed requesting unknown remote tip");
                                    }
                                }
                                let mut rt = runtime.write().await;
                                rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                            } else {
                                let mut rt = runtime.write().await;
                                rt.duplicate_block_requests_suppressed =
                                    rt.duplicate_block_requests_suppressed.saturating_add(1);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                            }
                        }
                    }

                    InboundEvent::GetBlockHeaders { hashes } => {
                        let headers = {
                            let guard = chain.read().await;
                            hashes
                                .iter()
                                .filter_map(|hash| {
                                    guard.dag.blocks.get(hash).map(|block| {
                                        pulsedag_p2p::messages::BlockHeaderAnnouncement {
                                            hash: block.hash.clone(),
                                            header: block.header.clone(),
                                        }
                                    })
                                })
                                .collect::<Vec<_>>()
                        };
                        if let Some(ref p2p) = p2p {
                            if let Err(e) = p2p.send_block_headers(&headers) {
                                warn!(error = %e, header_count = headers.len(), "failed sending BlockHeaders response");
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.block_header_requests_received =
                            rt.block_header_requests_received.saturating_add(1);
                        rt.block_headers_sent =
                            rt.block_headers_sent.saturating_add(headers.len() as u64);
                    }
                    InboundEvent::BlockHeaders { headers } => {
                        let known_blocks = {
                            let guard = chain.read().await;
                            guard.dag.blocks.keys().cloned().collect::<HashSet<_>>()
                        };
                        let schedule = block_requests.schedule_header_fetches(
                            &headers,
                            &known_blocks,
                            now_unix(),
                        );
                        {
                            let mut rt = runtime.write().await;
                            rt.sync_pipeline
                                .observe_headers(headers.len() as u64, now_unix());
                            rt.block_header_batches_received =
                                rt.block_header_batches_received.saturating_add(1);
                            rt.block_headers_received = rt
                                .block_headers_received
                                .saturating_add(headers.len() as u64);
                            rt.block_fetch_parent_deferred = rt
                                .block_fetch_parent_deferred
                                .saturating_add(schedule.deferred.len() as u64);
                            rt.block_fetch_duplicate_inflight_suppressed = rt
                                .block_fetch_duplicate_inflight_suppressed
                                .saturating_add(schedule.duplicate_suppressed as u64);
                            rt.sync_state = if schedule.ready.is_empty() {
                                "headers_received"
                            } else {
                                "requesting_blocks"
                            }
                            .to_string();
                        }
                        for hash in schedule.ready {
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_block(&hash) {
                                    warn!(error = %e, block_hash = %hash, "failed issuing dependency-scheduled GetBlock request");
                                }
                            }
                            let mut rt = runtime.write().await;
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                            rt.pending_block_requests = block_requests.pending.len();
                            rt.sync_pipeline.request_blocks(1, now_unix());
                            let _ = storage.append_runtime_event(
                                "info",
                                "block_request_sent",
                                &format!("hash={} source=headers", hash),
                            );
                        }
                    }
                    InboundEvent::GetBlock { hash } => {
                        let block = {
                            let guard = chain.read().await;
                            guard.dag.blocks.get(&hash).cloned()
                        };
                        if let Some(ref p2p) = p2p {
                            if let Err(e) = p2p.send_block_data(Some(&hash), block.as_ref()) {
                                warn!(error = %e, block_hash = %hash, "failed sending BlockData response");
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.getblock_received = rt.getblock_received.saturating_add(1);
                        rt.blockdata_sent = rt.blockdata_sent.saturating_add(1);
                    }
                    InboundEvent::BlockDataMissing { hash } => {
                        let mut fallback_getblock_sent = false;
                        if let Some(hash) = hash.as_ref() {
                            let now = now_unix();
                            block_requests.note_not_found(hash, now);
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_headers(&[], Some(hash), 128) {
                                    warn!(error = %e, block_hash = %hash, "failed issuing fallback headers after BlockData not-found");
                                }
                                if block_requests.should_issue_getblock_for_peers(
                                    hash,
                                    now,
                                    active_peer_ids_from_handle(p2p),
                                ) {
                                    if let Err(e) = p2p.request_block(hash) {
                                        warn!(error = %e, block_hash = %hash, "failed issuing fallback GetBlock after BlockData not-found");
                                    } else {
                                        fallback_getblock_sent = true;
                                    }
                                }
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.sync_state = "degraded".to_string();
                        rt.sync_failures = rt.sync_failures.saturating_add(1);
                        rt.blockdata_not_found = rt.blockdata_not_found.saturating_add(1);
                        rt.block_request_fallbacks = rt.block_request_fallbacks.saturating_add(1);
                        rt.missing_parent_request_fallbacks =
                            rt.missing_parent_request_fallbacks.saturating_add(1);
                        if fallback_getblock_sent {
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                        }
                        if hash.is_some() {
                            rt.header_requests_sent = rt.header_requests_sent.saturating_add(1);
                        }
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                        rt.block_fetch_scheduler_queue_depth = fetch_scheduler.queue_depth();
                        rt.block_fetch_scheduler_inflight_by_peer =
                            block_requests.inflight_by_peer();
                        warn!(requested_hash = ?hash, "peer returned empty BlockData; cleared inflight and issued fallback request");
                    }
                    InboundEvent::PeerConnected(peer) => {
                        let peers_connected = p2p
                            .as_ref()
                            .and_then(|h| h.status().ok().map(|s| s.connected_peers));
                        if let Some(ref p2p) = p2p {
                            if let Err(e) = p2p.request_tips() {
                                warn!(error = %e, peer = %peer, "failed issuing GetTips on peer connect");
                            }
                        }
                        let mut rt = runtime.write().await;
                        rt.sync_state = "requesting_tips".to_string();
                        rt.tips_requested = rt.tips_requested.saturating_add(1);
                        drop(rt);
                        info!(peer = %peer, peers_connected = ?peers_connected, "p2p peer connected; requested tips");
                    }
                }
                let counters = block_requests.take_fetch_counters();
                if counters.suppressed > 0 || counters.queued > 0 || counters.dropped > 0 {
                    let mut rt = runtime.write().await;
                    rt.block_request_backpressure_suppressed = rt
                        .block_request_backpressure_suppressed
                        .saturating_add(counters.suppressed);
                    rt.block_request_fetches_queued = rt
                        .block_request_fetches_queued
                        .saturating_add(counters.queued);
                    rt.block_request_fetches_dropped = rt
                        .block_request_fetches_dropped
                        .saturating_add(counters.dropped);
                    rt.block_fetch_duplicate_inflight_suppressed = rt
                        .block_fetch_duplicate_inflight_suppressed
                        .saturating_add(counters.suppressed);
                    rt.pending_block_requests = block_requests.pending.len();
                    rt.inflight_block_requests = block_requests.pending.len();
                    rt.pending_block_request_hashes = block_requests.pending_hashes();
                    warn!(
                        suppressed = counters.suppressed,
                        queued = counters.queued,
                        dropped = counters.dropped,
                        max_pending_block_requests = block_requests.max_pending(),
                        max_pending_block_requests_per_peer = block_requests.max_pending_per_peer(),
                        pending_block_requests = block_requests.pending.len(),
                        "observed bounded GetBlock fetch counters"
                    );
                }
            }
        });
    }

    {
        let chain = app_state.chain.clone();
        let runtime = app_state.runtime.clone();
        let storage = app_state.storage.clone();
        let p2p = app_state.p2p.clone();
        tokio::spawn(async move {
            let mut previous_best_height = 0u64;
            let mut previous_accepted_p2p_blocks = 0u64;
            let mut previous_accepted_mined_blocks = 0u64;
            loop {
                sleep(Duration::from_secs(15)).await;
                let (issue_count, best_height, best_tip_hash, orphan_count, mempool_size) = {
                    let guard = chain.read().await;
                    (
                        pulsedag_core::dag_consistency_issues(&guard).len(),
                        guard.dag.best_height,
                        pulsedag_core::preferred_tip_hash(&guard)
                            .unwrap_or_else(|| guard.dag.genesis_hash.clone()),
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
                    active_alerts.push(format!(
                        "[mempool_pressure] high orphan count: {}",
                        orphan_count
                    ));
                }
                if mempool_size >= 512 {
                    active_alerts.push(format!(
                        "[mempool_pressure] high mempool size: {}",
                        mempool_size
                    ));
                }
                let stagnation_secs = rt
                    .last_height_change_unix
                    .map(|ts| now.saturating_sub(ts))
                    .unwrap_or(0);
                if stagnation_secs >= 600 {
                    active_alerts.push(format!(
                        "[tip_stagnation] height stagnant for {} seconds",
                        stagnation_secs
                    ));
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

                let p2p_status = if let Some(ref p2p_handle) = p2p {
                    p2p_handle.status().ok()
                } else {
                    None
                };
                let connected_peers = p2p_status
                    .as_ref()
                    .map(|status| status.connected_peers.len())
                    .unwrap_or(0);
                let connected_peers_semantics = p2p_status
                    .as_ref()
                    .map(|status| pulsedag_p2p::connected_peers_semantics(&status.mode).to_string())
                    .unwrap_or_else(|| "p2p disabled".to_string());
                let sync_health = if rt.sync_pipeline.last_error.is_some() {
                    "degraded".to_string()
                } else if rt.sync_pipeline.phase == pulsedag_core::SyncPhase::Idle {
                    "idle".to_string()
                } else {
                    "active".to_string()
                };
                let snapshot_status = if let Some(h) = rt.last_snapshot_height {
                    format!(
                        "last_snapshot_height={} last_snapshot_unix={:?}",
                        h, rt.last_snapshot_unix
                    )
                } else {
                    "last_snapshot_height=none".to_string()
                };
                let prune_status = format!(
                    "auto_prune={} every_blocks={} keep_recent={} last_prune_height={:?}",
                    rt.auto_prune_enabled,
                    rt.auto_prune_every_blocks,
                    rt.prune_keep_recent_blocks,
                    rt.last_prune_height
                );
                let rollup = build_operator_console_rollup(
                    &OperatorConsoleInputs {
                        best_height,
                        best_tip_hash,
                        startup_path: rt.startup_path.clone(),
                        startup_summary: rt.startup_status_summary.clone(),
                        sync_phase: format!("{:?}", rt.sync_pipeline.phase),
                        sync_health,
                        connected_peers,
                        connected_peers_semantics,
                        mempool_size,
                        orphan_count,
                        active_alerts: rt.active_alerts.clone(),
                        last_height_change_unix: rt.last_height_change_unix,
                        now_unix: now,
                        accepted_p2p_blocks: rt.accepted_p2p_blocks,
                        accepted_mined_blocks: rt.accepted_mined_blocks,
                        snapshot_status,
                        prune_status,
                    },
                    previous_best_height,
                    previous_accepted_p2p_blocks,
                    previous_accepted_mined_blocks,
                );
                info!("{}", rollup.line);
                if rollup.height_changed {
                    info!(
                        best_height,
                        tip = %rollup.tip_hash_short,
                        "operator signal: chain height advanced"
                    );
                }
                if rollup.accepted_p2p_blocks_delta > 0 {
                    info!(
                        accepted_inbound_blocks_delta = rollup.accepted_p2p_blocks_delta,
                        "operator signal: new inbound blocks accepted"
                    );
                }
                if rollup.accepted_mined_blocks_delta > 0 {
                    info!(
                        accepted_mined_blocks_delta = rollup.accepted_mined_blocks_delta,
                        "operator signal: locally mined block accepted"
                    );
                }
                if rollup.stagnation_secs >= 600 {
                    warn!(
                        stagnation_secs = rollup.stagnation_secs,
                        best_height, "operator signal: height stagnation detected"
                    );
                }

                previous_best_height = best_height;
                previous_accepted_p2p_blocks = rt.accepted_p2p_blocks;
                previous_accepted_mined_blocks = rt.accepted_mined_blocks;
            }
        });
    }

    {
        let chain = app_state.chain.clone();
        let runtime = app_state.runtime.clone();
        let storage = app_state.storage.clone();
        let chain_id = cfg.chain_id.clone();
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
                                    .replay_from_validated_snapshot_and_delta(Some(&chain_id))
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

    let rpc_profile = match cfg.api_profile {
        config::ApiExposureProfile::LocalDev => RpcApiExposureProfile::LocalDev,
        config::ApiExposureProfile::PrivateOperator => RpcApiExposureProfile::PrivateOperator,
        config::ApiExposureProfile::PublicSafe => RpcApiExposureProfile::PublicSafe,
        config::ApiExposureProfile::DisabledAdmin => RpcApiExposureProfile::DisabledAdmin,
    };
    let hardening_limits = RpcHardeningLimits {
        request_body_limit_bytes: cfg.rpc_request_body_limit_bytes,
        rate_limit: if cfg.rpc_rate_limit_requests_per_minute == 0 {
            None
        } else {
            Some(RateLimitConfig {
                requests_per_window: cfg.rpc_rate_limit_requests_per_minute,
                window_secs: 60,
                per_ip: cfg.rpc_rate_limit_per_ip,
            })
        },
    };
    let cors_layer = build_cors_layer(&cfg)?;
    let app: Router = router_with_profile(
        rpc_profile,
        cfg.admin_enabled,
        cfg.operator_auth_token.clone(),
        Some(hardening_limits),
    )
    .layer(cors_layer)
    .with_state(app_state);
    let addr: SocketAddr = cfg.rpc_bind.parse()?;
    let listener = TcpListener::bind(addr).await?;

    if cfg.admin_enabled {
        warn!(rpc_bind = %cfg.rpc_bind, api_profile = ?cfg.api_profile, "admin RPC endpoints are ENABLED; restrict access and avoid unauthenticated exposure");
    }
    if !config::is_local_rpc_bind(&cfg.rpc_bind) || cfg.rpc_bind.starts_with("0.0.0.0:") {
        warn!(rpc_bind = %cfg.rpc_bind, "RPC is bound beyond localhost; verify firewall rules, auth controls, and API profile before exposing this port");
    }

    info!(p2p_enabled = cfg.p2p_enabled, p2p_mode = %cfg.p2p_mode, admin_enabled = cfg.admin_enabled, operator_auth_configured = cfg.operator_auth_token.is_some(), api_profile = ?cfg.api_profile, auto_rebuild_on_start = cfg.auto_rebuild_on_start, persist_snapshot_on_start = cfg.persist_snapshot_on_start, snapshot_auto_every_blocks = cfg.snapshot_auto_every_blocks, auto_prune_enabled = cfg.auto_prune_enabled, auto_prune_every_blocks = cfg.auto_prune_every_blocks, prune_keep_recent_blocks = cfg.prune_keep_recent_blocks, prune_require_snapshot = cfg.prune_require_snapshot, target_block_interval_secs = cfg.target_block_interval_secs, difficulty_window = cfg.difficulty_window, max_future_drift_secs = cfg.max_future_drift_secs, "pulsedagd RPC listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_cors_layer(cfg: &Config) -> Result<CorsLayer> {
    let any = cfg.rpc_cors_allowlist.iter().any(|o| o.trim() == "*");
    if any {
        return Err(anyhow::anyhow!(
            "invalid CORS policy: wildcard origin is not allowed for RPC; use an explicit allowlist"
        ));
    }
    let origins = cfg
        .rpc_cors_allowlist
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect::<Vec<_>>();
    Ok(CorsLayer::new().allow_origin(AllowOrigin::list(origins)))
}
