mod app_state;
mod block_request;
mod config;

use std::{
    collections::{BTreeMap, HashSet},
    io::{BufReader, BufWriter},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use app_state::{
    build_operator_console_rollup, build_startup_lifecycle_events, derive_startup_path_report,
    new_runtime_stats, short_hash, AppState, OperatorConsoleInputs,
};
use axum::Router;
use block_request::{
    BlockRequestTracker, DependencyAwareFetchScheduler, GetBlockRequestReadiness,
    HeaderFetchCandidate,
};
use config::Config;
use pulsedag_core::accept::{
    accept_transaction_with_result, AcceptSource, BlockAcceptanceResult, TxAcceptanceResult,
};
use pulsedag_core::reconcile_mempool;
use pulsedag_p2p::{
    build_p2p_stack, messages::HeaderInventory, InboundEvent, Libp2pConfig, Libp2pRuntimeMode,
    P2pHandle, P2pMode,
};
use pulsedag_rpc::api::{
    capture_and_store_node_rpc_snapshot, NodeRpcSnapshotStore, NodeRuntimeStats,
};
use pulsedag_rpc::routes::{
    router_with_profile, ApiExposureProfile as RpcApiExposureProfile, RateLimitConfig,
    RpcHardeningLimits,
};
use pulsedag_storage::Storage;
use tokio::sync::{oneshot, RwLock};
use tokio::time::{sleep, Duration};

const MAX_INFLIGHT_BLOCK_REQUESTS: usize = 64;
const MAX_INFLIGHT_BLOCK_REQUESTS_PER_PEER: usize = 16;
const MAX_FETCH_SCHEDULER_QUEUE_DEPTH: usize = 512;

const ORPHAN_RECOVERY_ROOT_REQUEST_LIMIT: usize = 16;
const ORPHAN_RECOVERY_REVALIDATE_EVICT_LIMIT: usize = 32;
const DEDICATED_RPC_RUNTIME_WORKER_THREADS: usize = 2;
const FINAL_QUIESCENCE_NO_PROGRESS_SECS: u64 = 45;
const FINAL_QUIESCENCE_CLEANUP_LIMIT: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FinalQuiescenceCleanupResult {
    reprocess_attempts: usize,
    reprocess_success: usize,
    terminalized_orphans: usize,
    terminalized_missing_parents: usize,
    quarantined_missing_parents: usize,
    active_missing_parent_entries: usize,
    terminal_missing_parent_entries: usize,
    quarantined_missing_parent_entries: usize,
}

fn run_final_quiescence_orphan_cleanup(
    chain: &mut pulsedag_core::ChainState,
    now_ms: u64,
    no_progress_ms: u64,
    limit: usize,
) -> FinalQuiescenceCleanupResult {
    if chain.orphan_blocks.is_empty() || limit == 0 {
        return FinalQuiescenceCleanupResult {
            active_missing_parent_entries: chain.orphan_parent_index.len(),
            terminal_missing_parent_entries: chain.terminal_missing_parents.len(),
            quarantined_missing_parent_entries: pulsedag_core::quarantined_missing_parent_count(
                chain,
            ),
            ..FinalQuiescenceCleanupResult::default()
        };
    }

    pulsedag_core::rebuild_orphan_parent_index(chain);
    let first = pulsedag_core::adopt_ready_orphans_with_result(chain, AcceptSource::P2p, None);
    let residual = pulsedag_core::terminalize_residual_waiting_missing_parents(
        chain,
        now_ms,
        no_progress_ms,
        limit,
    );
    let second = if residual.transitioned_parents > 0 || first.accepted > 0 {
        pulsedag_core::rebuild_orphan_parent_index(chain);
        pulsedag_core::adopt_ready_orphans_with_result(chain, AcceptSource::P2p, None)
    } else {
        pulsedag_core::OrphanAdoptionResult {
            accepted: 0,
            rejected: 0,
            retried: 0,
            accepted_hashes: Vec::new(),
            failure_reasons: BTreeMap::new(),
        }
    };

    FinalQuiescenceCleanupResult {
        reprocess_attempts: first.retried.saturating_add(second.retried),
        reprocess_success: first.accepted.saturating_add(second.accepted),
        terminalized_orphans: residual.evicted_orphans,
        terminalized_missing_parents: residual.transitioned_parents,
        quarantined_missing_parents: residual.transitioned_parents,
        active_missing_parent_entries: chain.orphan_parent_index.len(),
        terminal_missing_parent_entries: chain.terminal_missing_parents.len(),
        quarantined_missing_parent_entries: pulsedag_core::quarantined_missing_parent_count(chain),
    }
}

fn final_quiescence_reconcile_pending(total: u64, success: u64, blocked: u64) -> bool {
    total > success.saturating_add(blocked)
}

fn final_height_reconcile_rejection_reason(acceptance: &BlockAcceptanceResult) -> &'static str {
    match acceptance {
        BlockAcceptanceResult::MissingParent => "parent_missing_after_fetch",
        BlockAcceptanceResult::Rejected(message)
            if message.contains("storage") || message.contains("persist") =>
        {
            "storage_missing"
        }
        BlockAcceptanceResult::Rejected(_) | BlockAcceptanceResult::InvalidPow => {
            "validation_rejected"
        }
        _ => "block_received_but_rejected",
    }
}

fn final_same_height_reconcile_rejection_reason(
    acceptance: &BlockAcceptanceResult,
) -> &'static str {
    match acceptance {
        BlockAcceptanceResult::MissingParent => "same_height_block_received_but_rejected",
        BlockAcceptanceResult::Rejected(message)
            if message.contains("validation") || message.contains("invalid") =>
        {
            "same_height_candidate_validation_failed"
        }
        BlockAcceptanceResult::Rejected(_) | BlockAcceptanceResult::InvalidPow => {
            "same_height_candidate_validation_failed"
        }
        _ => "same_height_block_received_but_rejected",
    }
}

fn final_quiescence_request_suppression_reason(
    readiness: GetBlockRequestReadiness,
    pending_len: usize,
    max_pending: usize,
) -> &'static str {
    match readiness {
        GetBlockRequestReadiness::Requestable => "unknown",
        GetBlockRequestReadiness::AlreadyPending => "request_already_in_flight",
        GetBlockRequestReadiness::RateLimited if pending_len >= max_pending => "request_queue_full",
        GetBlockRequestReadiness::RateLimited => "peer_rate_limited",
    }
}

struct DedicatedRpcServer {
    local_addr: SocketAddr,
    worker_threads: usize,
    shutdown: Option<oneshot::Sender<()>>,
    join: Option<std::thread::JoinHandle<Result<()>>>,
}

impl DedicatedRpcServer {
    #[cfg(test)]
    fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    #[cfg(test)]
    fn worker_threads(&self) -> usize {
        self.worker_threads
    }

    #[cfg(test)]
    fn shutdown_and_join(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let join = self
            .join
            .take()
            .ok_or_else(|| anyhow::anyhow!("dedicated RPC runtime thread already joined"))?;
        join_rpc_runtime_thread(join)
    }
}

impl Drop for DedicatedRpcServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

fn spawn_dedicated_rpc_server(
    addr: SocketAddr,
    app: Router,
    worker_threads: usize,
) -> Result<DedicatedRpcServer> {
    let worker_threads = worker_threads.max(1);
    let std_listener = std::net::TcpListener::bind(addr)
        .with_context(|| format!("failed to bind RPC listener on {addr}"))?;
    std_listener
        .set_nonblocking(true)
        .context("failed to set RPC listener nonblocking before handing it to Tokio")?;
    let local_addr = std_listener
        .local_addr()
        .context("failed to read RPC listener local address")?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join = std::thread::Builder::new()
        .name("pulsedagd-rpc-runtime".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(worker_threads)
                .thread_name("pulsedagd-rpc-worker")
                .build()
                .context("failed to build dedicated RPC Tokio runtime")?;

            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(std_listener)
                    .context("failed to move std RPC listener into dedicated Tokio runtime")?;
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await
                    .context("dedicated RPC axum server exited with an error")?;
                Ok(())
            })
        })?;

    Ok(DedicatedRpcServer {
        local_addr,
        worker_threads,
        shutdown: Some(shutdown_tx),
        join: Some(join),
    })
}

fn join_rpc_runtime_thread(join: std::thread::JoinHandle<Result<()>>) -> Result<()> {
    match join.join() {
        Ok(result) => result,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "dedicated RPC runtime thread panicked".to_string());
            Err(anyhow::anyhow!(message))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OrphanRecoveryRootClassification {
    requestable: Vec<String>,
    already_pending: Vec<String>,
    rate_limited: Vec<String>,
    peer_not_found: Vec<String>,
    peer_timeout: Vec<String>,
    all_peers_exhausted: Vec<String>,
    unknown_peerless: Vec<String>,
    stale: Vec<String>,
    evictable: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OrphanRecoveryTickResult {
    roots_discovered: usize,
    roots_requested: usize,
    roots_rate_limited: usize,
    backlog_reindexed: bool,
    backlog_revalidated: bool,
    backlog_evicted: usize,
    backlog_stale: usize,
    forced_reindex: bool,
    unactionable_state: bool,
    classified_after_reindex: usize,
    evicted_after_unactionable: usize,
    residual_waiting_terminal: usize,
    residual_waiting_evicted: usize,
    root_classification_counts: BTreeMap<String, usize>,
    orphan_count: usize,
    pending_missing: usize,
    ages: (u64, u64),
    orphan_backlog: pulsedag_core::OrphanBacklogClassification,
    adopted: usize,
    retried: usize,
    persist_failed: bool,
    failure_reasons: std::collections::BTreeMap<String, usize>,
}

fn classify_orphan_recovery_roots(
    roots: Vec<String>,
    block_requests: &BlockRequestTracker,
    active_peers: &[String],
    has_p2p: bool,
    now_unix: u64,
) -> OrphanRecoveryRootClassification {
    let mut classification = OrphanRecoveryRootClassification::default();
    for root in roots {
        if !has_p2p || active_peers.is_empty() {
            classification.unknown_peerless.push(root);
            continue;
        }
        match block_requests.classify_getblock_for_peers(&root, now_unix, active_peers.to_vec()) {
            GetBlockRequestReadiness::Requestable => classification.requestable.push(root),
            GetBlockRequestReadiness::AlreadyPending => classification.already_pending.push(root),
            GetBlockRequestReadiness::RateLimited => {
                if block_requests.is_all_peers_exhausted(&root) {
                    classification.all_peers_exhausted.push(root);
                } else if block_requests.has_timed_out_peer(&root) {
                    classification.peer_timeout.push(root);
                } else if block_requests.has_not_found_peer(&root) {
                    classification.peer_not_found.push(root);
                } else {
                    classification.rate_limited.push(root);
                }
            }
        }
    }
    classification.sort();
    classification
}

impl OrphanRecoveryRootClassification {
    fn sort(&mut self) {
        self.requestable.sort();
        self.already_pending.sort();
        self.rate_limited.sort();
        self.peer_not_found.sort();
        self.peer_timeout.sort();
        self.all_peers_exhausted.sort();
        self.unknown_peerless.sort();
        self.stale.sort();
        self.evictable.sort();
    }

    fn total_classified(&self) -> usize {
        self.requestable.len()
            + self.already_pending.len()
            + self.rate_limited.len()
            + self.peer_not_found.len()
            + self.peer_timeout.len()
            + self.all_peers_exhausted.len()
            + self.unknown_peerless.len()
            + self.stale.len()
            + self.evictable.len()
    }

    fn counters(&self) -> BTreeMap<String, usize> {
        BTreeMap::from([
            ("requestable".to_string(), self.requestable.len()),
            ("already_pending".to_string(), self.already_pending.len()),
            ("rate_limited".to_string(), self.rate_limited.len()),
            ("peer_not_found".to_string(), self.peer_not_found.len()),
            ("peer_timeout".to_string(), self.peer_timeout.len()),
            (
                "all_peers_exhausted".to_string(),
                self.all_peers_exhausted.len(),
            ),
            ("unknown_peerless".to_string(), self.unknown_peerless.len()),
            ("stale".to_string(), self.stale.len()),
            ("evictable".to_string(), self.evictable.len()),
        ])
    }
}

async fn terminally_handle_exhausted_missing_parent(
    chain: &Arc<RwLock<pulsedag_core::ChainState>>,
    runtime: &Arc<RwLock<NodeRuntimeStats>>,
    hash: &str,
    exhausted_peers: Vec<String>,
) {
    let terminal = {
        let mut guard = chain.write().await;
        pulsedag_core::terminally_exhaust_missing_parent(
            &mut guard,
            &hash.to_string(),
            exhausted_peers,
            now_unix().saturating_mul(1_000),
            true,
        )
    };
    let mut rt = runtime.write().await;
    if terminal.transitioned {
        rt.missing_parent_terminal_exhausted_total =
            rt.missing_parent_terminal_exhausted_total.saturating_add(1);
        rt.orphan_missing_parent_terminal_evicted_total = rt
            .orphan_missing_parent_terminal_evicted_total
            .saturating_add(terminal.evicted_orphans as u64);
    } else {
        rt.missing_parent_retry_suppressed_exhausted_total = rt
            .missing_parent_retry_suppressed_exhausted_total
            .saturating_add(1);
    }
}

fn final_quiescence_reachable_peer_count(status: &pulsedag_p2p::P2pStatus) -> usize {
    let connected_peer_count = status.connected_peers.len();
    let known_peer_count = status
        .peer_recovery
        .len()
        .max(status.bootnodes_configured.len())
        .max(connected_peer_count);

    if status.connection_slot_budget == 0 {
        known_peer_count
    } else if known_peer_count == 0 {
        status.connection_slot_budget
    } else {
        status.connection_slot_budget.min(known_peer_count)
    }
}

fn final_quiescence_all_reachable_peers_connected(status: &pulsedag_p2p::P2pStatus) -> bool {
    let expected_peer_count = final_quiescence_reachable_peer_count(status);
    expected_peer_count == 0 || status.connected_peers.len() >= expected_peer_count
}

fn should_force_orphan_missing_parent_reindex(
    chain: &pulsedag_core::state::ChainState,
    inv_hashes_requested: usize,
) -> bool {
    let orphan_count = chain.orphan_blocks.len();
    let pending_missing = pulsedag_core::pending_missing_parent_count(chain);
    let missing_parent_entries = chain.orphan_missing_parents.len();
    (orphan_count > 0 || pending_missing > 0)
        && missing_parent_entries == 0
        && inv_hashes_requested == 0
}

fn classify_stale_or_evictable_roots(
    chain: &pulsedag_core::state::ChainState,
    now_ms: u64,
    max_age_ms: u64,
    limit: usize,
    root_classes: &mut OrphanRecoveryRootClassification,
) {
    if limit == 0 {
        return;
    }
    let mut stale_roots = Vec::new();
    let mut evictable_roots = Vec::new();
    let mut stale_orphans = chain
        .orphan_received_at_ms
        .iter()
        .filter_map(|(hash, received_at)| {
            (now_ms.saturating_sub(*received_at) > max_age_ms).then_some((hash, received_at))
        })
        .collect::<Vec<_>>();
    stale_orphans.sort_by(|(left_hash, left_ts), (right_hash, right_ts)| {
        left_ts
            .cmp(right_ts)
            .then_with(|| left_hash.cmp(right_hash))
    });
    for (index, (hash, _)) in stale_orphans.into_iter().enumerate() {
        for root in pulsedag_core::orphan_missing_roots(chain, hash) {
            if index < limit {
                evictable_roots.push(root.clone());
            }
            stale_roots.push(root);
        }
    }
    stale_roots.sort();
    stale_roots.dedup();
    evictable_roots.sort();
    evictable_roots.dedup();
    root_classes.stale = stale_roots;
    root_classes.evictable = evictable_roots;
    root_classes.sort();
}

fn orphan_recovery_roots(chain: &pulsedag_core::ChainState) -> Vec<String> {
    let known = chain.dag.blocks.keys().cloned().collect::<HashSet<_>>();
    let mut roots = chain
        .orphan_blocks
        .keys()
        .flat_map(|hash| pulsedag_core::orphan_missing_roots(chain, hash))
        .filter(|root| !known.contains(root))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SnapshotBundleCommand {
    Export(PathBuf),
    Import(PathBuf),
}

fn parse_snapshot_bundle_command(args: &[String]) -> Result<Option<SnapshotBundleCommand>> {
    let mut command = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--snapshot-export" => {
                let path = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--snapshot-export requires a path"))?;
                if command.is_some() {
                    anyhow::bail!("only one snapshot bundle command may be provided");
                }
                command = Some(SnapshotBundleCommand::Export(PathBuf::from(path)));
            }
            "--snapshot-import" => {
                let path = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--snapshot-import requires a path"))?;
                if command.is_some() {
                    anyhow::bail!("only one snapshot bundle command may be provided");
                }
                command = Some(SnapshotBundleCommand::Import(PathBuf::from(path)));
            }
            _ => {}
        }
    }
    Ok(command)
}

fn run_snapshot_bundle_command(
    storage: &Storage,
    chain_id: &str,
    command: SnapshotBundleCommand,
) -> Result<()> {
    match command {
        SnapshotBundleCommand::Export(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let (bundle, report) = storage.export_snapshot_bundle(Some(chain_id))?;
            let file = std::fs::File::create(&path)?;
            bincode::serialize_into(BufWriter::new(file), &bundle)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "action": "snapshot_export",
                    "path": path,
                    "verification": report,
                }))?
            );
        }
        SnapshotBundleCommand::Import(path) => {
            let file = std::fs::File::open(&path)?;
            let bundle: pulsedag_storage::SnapshotExportBundle =
                bincode::deserialize_from(BufReader::new(file))?;
            let report = storage.import_snapshot_bundle(bundle, Some(chain_id))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "action": "snapshot_import",
                    "path": path,
                    "verification": report,
                }))?
            );
        }
    }
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
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
            let mut peers = status
                .active_connections_by_peer
                .into_iter()
                .filter_map(|(peer, connections)| (connections > 0).then_some(peer))
                .collect::<Vec<_>>();
            if peers.is_empty() {
                peers = status.connected_peers;
            }
            peers.sort();
            peers.dedup();
            peers
        })
        .unwrap_or_default()
}

fn update_orphan_backlog_classification(
    runtime: &mut pulsedag_rpc::api::NodeRuntimeStats,
    chain: &pulsedag_core::ChainState,
) {
    let classification = pulsedag_core::classify_orphan_backlog(chain);
    runtime.orphan_backlog_retryable_ready = classification.retryable_ready;
    runtime.orphan_backlog_waiting_missing_parent = classification.waiting_missing_parent;
    runtime.orphan_backlog_stale_missing_parent_entries =
        classification.stale_missing_parent_entries;
    runtime.orphan_backlog_unindexed_missing_parent_entries =
        classification.unindexed_missing_parent_entries;
}

fn usage() -> &'static str {
    "usage: pulsedagd [--network dev|testnet|mainnet] [--rpc-listen HOST:PORT] [--p2p-listen MULTIADDR] [--bootnode MULTIADDR] [--peer MULTIADDR] [--snapshot-export PATH|--snapshot-import PATH] [--help] [--version]"
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

    let snapshot_bundle_command = parse_snapshot_bundle_command(&cli_args)?;
    let mut cfg = Config::from_env()?;
    cfg.apply_cli_args(cli_args)?;
    let config_safety_summary = cfg.config_safety_summary();
    if config_safety_summary.contains("warning") {
        warn!(summary = %config_safety_summary, "config safety summary");
    } else {
        info!(summary = %config_safety_summary, "config safety summary");
    }
    let storage = Arc::new(Storage::open(&cfg.rocksdb_path)?);
    if let Some(command) = snapshot_bundle_command {
        run_snapshot_bundle_command(&storage, &cfg.chain_id, command)?;
        return Ok(());
    }

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
        rpc_snapshot: NodeRpcSnapshotStore::default(),
    };

    let _ = capture_and_store_node_rpc_snapshot(&app_state).await;

    {
        let snapshot_state = app_state.clone();
        tokio::spawn(async move {
            loop {
                let _ = capture_and_store_node_rpc_snapshot(&snapshot_state).await;
                sleep(Duration::from_millis(500)).await;
            }
        });
    }

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
            let mut final_quiescence_higher_tip_requests: HashSet<String> = HashSet::new();
            let mut final_quiescence_same_height_tip_requests: HashSet<String> = HashSet::new();
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
                        let final_height_retry =
                            final_quiescence_higher_tip_requests.contains(&hash);
                        let final_same_height_retry =
                            final_quiescence_same_height_tip_requests.contains(&hash);
                        let outcome = block_requests.retry_after_timeout(
                            &hash,
                            now_unix(),
                            active_peer_ids(&p2p),
                        );
                        let mut sent = false;
                        if outcome.retry {
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_block(&hash) {
                                    warn!(error = %e, block_hash = %hash, "failed retrying timed-out GetBlock request on next peer");
                                } else {
                                    sent = true;
                                }
                            }
                        }
                        if outcome.all_peers_exhausted {
                            terminally_handle_exhausted_missing_parent(
                                &chain,
                                &runtime,
                                &hash,
                                active_peer_ids(&p2p),
                            )
                            .await;
                        }
                        let mut rt = runtime.write().await;
                        if final_height_retry && outcome.all_peers_exhausted {
                            final_quiescence_higher_tip_requests.remove(&hash);
                            rt.final_quiescence_height_reconcile_blocked_total = rt
                                .final_quiescence_height_reconcile_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_height_reconcile_blocked_reason =
                                Some("block_request_timed_out".to_string());
                        }
                        if final_same_height_retry && outcome.all_peers_exhausted {
                            final_quiescence_same_height_tip_requests.remove(&hash);
                            rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                .final_quiescence_same_height_reconcile_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_same_height_reconcile_blocked_reason =
                                Some("same_height_block_request_timed_out".to_string());
                        }
                        rt.missing_parent_peer_timeout_total =
                            rt.missing_parent_peer_timeout_total.saturating_add(1);
                        if outcome.retry {
                            rt.missing_parent_retry_next_peer_total =
                                rt.missing_parent_retry_next_peer_total.saturating_add(1);
                            rt.missing_parent_retry_peer_total =
                                rt.missing_parent_retry_peer_total.saturating_add(1);
                        }
                        if outcome.all_peers_exhausted {
                            rt.missing_parent_all_peers_exhausted_total = rt
                                .missing_parent_all_peers_exhausted_total
                                .saturating_add(1);
                        }
                        if sent {
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                            rt.missing_parent_requests_sent =
                                rt.missing_parent_requests_sent.saturating_add(1);
                            rt.missing_parent_request_started_total =
                                rt.missing_parent_request_started_total.saturating_add(1);
                        }
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                    }
                    for hash in timed_out.expired {
                        let final_height_expired =
                            final_quiescence_higher_tip_requests.remove(&hash);
                        let final_same_height_expired =
                            final_quiescence_same_height_tip_requests.remove(&hash);
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
                            if final_height_expired {
                                final_quiescence_higher_tip_requests.insert(hash.clone());
                            }
                            if final_same_height_expired {
                                final_quiescence_same_height_tip_requests.insert(hash.clone());
                            }
                            let mut rt = runtime.write().await;
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                            rt.missing_parent_requests_sent =
                                rt.missing_parent_requests_sent.saturating_add(1);
                            rt.missing_parent_request_started_total =
                                rt.missing_parent_request_started_total.saturating_add(1);
                            rt.pending_block_requests = block_requests.pending.len();
                            rt.inflight_block_requests = block_requests.pending.len();
                            rt.pending_block_request_hashes = block_requests.pending_hashes();
                        } else {
                            terminally_handle_exhausted_missing_parent(
                                &chain,
                                &runtime,
                                &hash,
                                active_peer_ids(&p2p),
                            )
                            .await;
                            let mut rt = runtime.write().await;
                            if final_height_expired {
                                rt.final_quiescence_height_reconcile_blocked_total = rt
                                    .final_quiescence_height_reconcile_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_height_reconcile_blocked_reason =
                                    Some("block_request_timed_out".to_string());
                            }
                            if final_same_height_expired {
                                rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                    .final_quiescence_same_height_reconcile_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_same_height_reconcile_blocked_reason =
                                    Some("same_height_block_request_timed_out".to_string());
                            }
                            rt.missing_parent_all_peers_exhausted_total = rt
                                .missing_parent_all_peers_exhausted_total
                                .saturating_add(1);
                            warn!(block_hash = %hash, "GetBlock request expired after retry limit; clearing inflight state");
                        }
                    }
                }
                recovery_tick = recovery_tick.saturating_add(1);
                if recovery_tick.is_multiple_of(5) {
                    let tick_started = Instant::now();
                    let active_peers = active_peer_ids(&p2p);
                    let has_p2p = p2p.is_some();
                    let inv_hashes_requested = p2p
                        .as_ref()
                        .and_then(|handle| handle.status().ok())
                        .map(|status| status.inv_hashes_requested)
                        .unwrap_or(0);
                    let mut tick = {
                        let mut guard = chain.write().await;
                        let forced_reindex = should_force_orphan_missing_parent_reindex(
                            &guard,
                            inv_hashes_requested,
                        );
                        if guard.orphan_blocks.is_empty() {
                            OrphanRecoveryTickResult {
                                ages: orphan_age_metrics(&guard, now_unix()),
                                orphan_backlog: pulsedag_core::classify_orphan_backlog(&guard),
                                ..OrphanRecoveryTickResult::default()
                            }
                        } else {
                            let rebuilt = pulsedag_core::rebuild_orphan_parent_index(&mut guard);
                            info!(
                                event = if forced_reindex {
                                    "orphan_parent_index_forced_reindex"
                                } else {
                                    "orphan_parent_index_rebuilt"
                                },
                                retryable_ready = rebuilt.retryable_ready,
                                waiting_missing_parent = rebuilt.waiting_missing_parent,
                                stale_missing_parent_entries = rebuilt.stale_missing_parent_entries,
                                unindexed_missing_parent_entries =
                                    rebuilt.unindexed_missing_parent_entries,
                                "rebuilt orphan parent index from queued orphan block parents"
                            );
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

                            let roots = orphan_recovery_roots(&guard);
                            let roots_discovered = roots.len();
                            let mut root_classes = classify_orphan_recovery_roots(
                                roots,
                                &block_requests,
                                &active_peers,
                                has_p2p,
                                now_unix(),
                            );
                            classify_stale_or_evictable_roots(
                                &guard,
                                now_millis(),
                                pulsedag_core::DEFAULT_ORPHAN_MAX_AGE_MS,
                                ORPHAN_RECOVERY_REVALIDATE_EVICT_LIMIT,
                                &mut root_classes,
                            );
                            let root_classification_counts = root_classes.counters();
                            let classified_after_reindex = root_classes.total_classified();
                            let no_requestable_roots = root_classes.requestable.is_empty();
                            let mut stale = rebuilt.stale_missing_parent_entries;
                            let mut evicted = 0usize;
                            let mut revalidated = false;
                            let mut residual_waiting_terminal = 0usize;
                            let mut residual_waiting_evicted = 0usize;
                            if no_requestable_roots {
                                warn!(
                                    requestable = *root_classification_counts
                                        .get("requestable")
                                        .unwrap_or(&0),
                                    already_pending = *root_classification_counts
                                        .get("already_pending")
                                        .unwrap_or(&0),
                                    rate_limited = *root_classification_counts
                                        .get("rate_limited")
                                        .unwrap_or(&0),
                                    peer_not_found = *root_classification_counts
                                        .get("peer_not_found")
                                        .unwrap_or(&0),
                                    peer_timeout = *root_classification_counts
                                        .get("peer_timeout")
                                        .unwrap_or(&0),
                                    all_peers_exhausted = *root_classification_counts
                                        .get("all_peers_exhausted")
                                        .unwrap_or(&0),
                                    unknown_peerless = *root_classification_counts
                                        .get("unknown_peerless")
                                        .unwrap_or(&0),
                                    stale = *root_classification_counts.get("stale").unwrap_or(&0),
                                    evictable =
                                        *root_classification_counts.get("evictable").unwrap_or(&0),
                                    "orphan missing-parent reindex found no requestable roots"
                                );
                                let revalidated_backlog =
                                    pulsedag_core::revalidate_orphan_backlog(&mut guard);
                                stale = stale.saturating_add(
                                    revalidated_backlog.stale_missing_parent_entries,
                                );
                                revalidated = true;
                                evicted = pulsedag_core::evict_stale_orphans_bounded(
                                    &mut guard,
                                    now_millis(),
                                    pulsedag_core::DEFAULT_ORPHAN_MAX_AGE_MS,
                                    ORPHAN_RECOVERY_REVALIDATE_EVICT_LIMIT,
                                );
                                let residual =
                                    pulsedag_core::terminalize_residual_waiting_missing_parents(
                                        &mut guard,
                                        now_millis(),
                                        pulsedag_core::DEFAULT_ORPHAN_MAX_AGE_MS,
                                        ORPHAN_RECOVERY_REVALIDATE_EVICT_LIMIT,
                                    );
                                residual_waiting_terminal = residual.transitioned_parents;
                                residual_waiting_evicted = residual.evicted_orphans;
                                if evicted > 0 || residual_waiting_terminal > 0 {
                                    match storage.persist_chain_state(&guard) {
                                        Ok(()) => {}
                                        Err(e) => {
                                            warn!(error = %e, evicted, residual_waiting_terminal, "failed persisting chain state after residual missing-parent eviction")
                                        }
                                    }
                                }
                            }
                            let ages = orphan_age_metrics(&guard, now_unix());
                            OrphanRecoveryTickResult {
                                roots_discovered,
                                roots_requested: 0,
                                roots_rate_limited: root_classes.rate_limited.len(),
                                backlog_reindexed: true,
                                backlog_revalidated: revalidated,
                                backlog_evicted: evicted,
                                backlog_stale: stale,
                                forced_reindex,
                                unactionable_state: forced_reindex || no_requestable_roots,
                                classified_after_reindex,
                                evicted_after_unactionable: if no_requestable_roots {
                                    evicted
                                } else {
                                    0
                                },
                                residual_waiting_terminal,
                                residual_waiting_evicted,
                                root_classification_counts,
                                orphan_count: guard.orphan_blocks.len(),
                                pending_missing: pulsedag_core::pending_missing_parent_count(
                                    &guard,
                                ),
                                ages,
                                orphan_backlog: pulsedag_core::classify_orphan_backlog(&guard),
                                adopted,
                                retried,
                                persist_failed,
                                failure_reasons,
                            }
                        }
                    };
                    let roots = {
                        let guard = chain.read().await;
                        orphan_recovery_roots(&guard)
                    };
                    let root_classes = classify_orphan_recovery_roots(
                        roots,
                        &block_requests,
                        &active_peers,
                        has_p2p,
                        now_unix(),
                    );
                    for parent in root_classes
                        .requestable
                        .into_iter()
                        .take(ORPHAN_RECOVERY_ROOT_REQUEST_LIMIT)
                    {
                        if block_requests.should_issue_getblock_for_peers(
                            &parent,
                            now_unix(),
                            active_peers.clone(),
                        ) {
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_block(&parent) {
                                    warn!(error = %e, missing_parent = %parent, "failed issuing recovery-tick orphan-root GetBlock request");
                                }
                            }
                            tick.roots_requested = tick.roots_requested.saturating_add(1);
                        }
                    }
                    if tick.retried > 0
                        || tick.roots_discovered > 0
                        || tick.pending_missing > 0
                        || tick.backlog_reindexed
                        || tick.backlog_revalidated
                        || tick.backlog_evicted > 0
                    {
                        let mut rt = runtime.write().await;
                        let reprocess_attempts = if tick.retried > 0 {
                            tick.retried as u64
                        } else {
                            1
                        };
                        rt.orphan_reprocess_attempts = rt
                            .orphan_reprocess_attempts
                            .saturating_add(reprocess_attempts);
                        rt.orphan_reprocess_success = rt
                            .orphan_reprocess_success
                            .saturating_add(tick.adopted as u64);
                        rt.orphan_reprocess_failed_missing_parent =
                            rt.orphan_reprocess_failed_missing_parent.saturating_add(
                                tick.failure_reasons
                                    .get("missing_parent")
                                    .copied()
                                    .unwrap_or(0) as u64,
                            );
                        record_orphan_reprocess_failures(&mut rt, &tick.failure_reasons);
                        if tick.retried == 0 && tick.pending_missing > 0 {
                            let entry = rt
                                .orphan_reprocess_failures_by_reason
                                .entry("waiting_missing_parent".to_string())
                                .or_insert(0);
                            *entry = entry.saturating_add(tick.pending_missing as u64);
                            rt.last_orphan_reprocess_failure_reason =
                                Some("waiting_missing_parent".to_string());
                        }
                        rt.orphan_blocks_retried =
                            rt.orphan_blocks_retried.saturating_add(tick.retried as u64);
                        rt.orphan_blocks_resolved = rt
                            .orphan_blocks_resolved
                            .saturating_add(tick.adopted as u64);
                        if tick.persist_failed {
                            rt.orphan_reprocess_failed_persist =
                                rt.orphan_reprocess_failed_persist.saturating_add(1);
                        }
                        rt.orphan_roots_discovered_total = rt
                            .orphan_roots_discovered_total
                            .saturating_add(tick.roots_discovered as u64);
                        rt.orphan_roots_requested_total = rt
                            .orphan_roots_requested_total
                            .saturating_add(tick.roots_requested as u64);
                        rt.orphan_roots_rate_limited_total = rt
                            .orphan_roots_rate_limited_total
                            .saturating_add(tick.roots_rate_limited as u64);
                        if tick.backlog_reindexed {
                            rt.orphan_backlog_reindexed_total =
                                rt.orphan_backlog_reindexed_total.saturating_add(1);
                        }
                        if tick.backlog_revalidated {
                            rt.orphan_backlog_revalidated_total =
                                rt.orphan_backlog_revalidated_total.saturating_add(1);
                        }
                        rt.orphan_backlog_evicted_total = rt
                            .orphan_backlog_evicted_total
                            .saturating_add(tick.backlog_evicted as u64);
                        rt.orphan_backlog_stale_total = rt
                            .orphan_backlog_stale_total
                            .saturating_add(tick.backlog_stale as u64);
                        rt.missing_parent_residual_waiting_terminal_total = rt
                            .missing_parent_residual_waiting_terminal_total
                            .saturating_add(tick.residual_waiting_terminal as u64);
                        rt.orphan_missing_parent_residual_evicted_total = rt
                            .orphan_missing_parent_residual_evicted_total
                            .saturating_add(tick.residual_waiting_evicted as u64);
                        if tick.forced_reindex {
                            rt.orphan_missing_parent_forced_reindex_total = rt
                                .orphan_missing_parent_forced_reindex_total
                                .saturating_add(1);
                        }
                        if tick.unactionable_state {
                            rt.orphan_missing_parent_unactionable_state_total = rt
                                .orphan_missing_parent_unactionable_state_total
                                .saturating_add(1);
                        }
                        rt.orphan_missing_parent_classified_after_reindex_total = rt
                            .orphan_missing_parent_classified_after_reindex_total
                            .saturating_add(tick.classified_after_reindex as u64);
                        rt.orphan_missing_parent_evicted_after_unactionable_total = rt
                            .orphan_missing_parent_evicted_after_unactionable_total
                            .saturating_add(tick.evicted_after_unactionable as u64);
                        rt.orphan_missing_parent_stale_evicted_total = rt
                            .orphan_missing_parent_stale_evicted_total
                            .saturating_add(tick.backlog_evicted as u64);
                        let recovery_progress = tick
                            .adopted
                            .saturating_add(tick.roots_requested)
                            .saturating_add(tick.backlog_evicted);
                        rt.orphan_missing_parent_recovery_progress_total = rt
                            .orphan_missing_parent_recovery_progress_total
                            .saturating_add(recovery_progress as u64);
                        rt.missing_parent_request_already_pending_total = rt
                            .missing_parent_request_already_pending_total
                            .saturating_add(
                                tick.root_classification_counts
                                    .get("already_pending")
                                    .copied()
                                    .unwrap_or(0) as u64,
                            );
                        rt.missing_parent_all_peers_exhausted_total =
                            rt.missing_parent_all_peers_exhausted_total.saturating_add(
                                tick.root_classification_counts
                                    .get("all_peers_exhausted")
                                    .copied()
                                    .unwrap_or(0) as u64,
                            );
                        for (classification, count) in &tick.root_classification_counts {
                            if *count == 0 {
                                continue;
                            }
                            let entry = rt
                                .orphan_reprocess_failures_by_reason
                                .entry(format!("missing_parent_{classification}"))
                                .or_insert(0);
                            *entry = entry.saturating_add(*count as u64);
                        }
                        rt.orphan_recovery_tick_duration_ms =
                            tick_started.elapsed().as_millis() as u64;
                        rt.orphan_blocks_evicted = rt
                            .orphan_blocks_evicted
                            .saturating_add(tick.backlog_evicted as u64);
                        rt.getblock_sent =
                            rt.getblock_sent.saturating_add(tick.roots_requested as u64);
                        rt.missing_parent_requests_sent = rt
                            .missing_parent_requests_sent
                            .saturating_add(tick.roots_requested as u64);
                        rt.missing_parent_request_started_total = rt
                            .missing_parent_request_started_total
                            .saturating_add(tick.roots_requested as u64);
                        rt.block_request_fallbacks = rt
                            .block_request_fallbacks
                            .saturating_add(tick.roots_requested as u64);
                        rt.missing_parent_request_fallbacks = rt
                            .missing_parent_request_fallbacks
                            .saturating_add(tick.roots_requested as u64);
                        rt.pending_missing_parents = tick.pending_missing;
                        rt.orphan_backlog_retryable_ready = tick.orphan_backlog.retryable_ready;
                        rt.orphan_backlog_waiting_missing_parent =
                            tick.orphan_backlog.waiting_missing_parent;
                        rt.orphan_backlog_stale_missing_parent_entries =
                            tick.orphan_backlog.stale_missing_parent_entries;
                        rt.orphan_backlog_unindexed_missing_parent_entries =
                            tick.orphan_backlog.unindexed_missing_parent_entries;
                        rt.max_orphan_age_secs = tick.ages.0;
                        rt.oldest_orphan_age_secs = tick.ages.0;
                        rt.oldest_missing_parent_age_secs = tick
                            .ages
                            .1
                            .max(block_requests.oldest_pending_age_secs(now_unix()));
                        rt.pending_block_requests = block_requests.pending.len();
                        rt.inflight_block_requests = block_requests.pending.len();
                        rt.pending_block_request_hashes = block_requests.pending_hashes();
                        rt.block_fetch_scheduler_queue_depth = fetch_scheduler.queue_depth();
                        rt.block_fetch_scheduler_inflight_by_peer =
                            block_requests.inflight_by_peer();
                        rt.sync_state = if tick.orphan_count == 0 {
                            "synced"
                        } else {
                            "catching_up"
                        }
                        .to_string();
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
                        let final_height_reconcile_block =
                            final_quiescence_higher_tip_requests.contains(&block.hash);
                        let final_same_height_reconcile_block =
                            final_quiescence_same_height_tip_requests.contains(&block.hash);
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
                                rt.missing_parent_peer_response_success_total = rt
                                    .missing_parent_peer_response_success_total
                                    .saturating_add(1);
                            }
                        }
                        info!(event = "peer_block_received", block_hash = %block.hash, parent_count = block.header.parents.len(), "received inbound p2p block payload");
                        let height_before_accept = guard.dag.best_height;
                        let final_height_gap_before =
                            block.header.height.saturating_sub(height_before_accept);
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
                            if final_height_reconcile_block {
                                final_quiescence_higher_tip_requests.remove(&block.hash);
                                rt.final_quiescence_higher_tip_fetch_success_total = rt
                                    .final_quiescence_higher_tip_fetch_success_total
                                    .saturating_add(1);
                                rt.final_quiescence_height_reconcile_blocked_total = rt
                                    .final_quiescence_height_reconcile_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_height_reconcile_blocked_reason = Some(
                                    final_height_reconcile_rejection_reason(&acceptance)
                                        .to_string(),
                                );
                                rt.final_quiescence_height_gap_before = final_height_gap_before;
                                rt.final_quiescence_height_gap_after = final_height_gap_before;
                                rt.final_quiescence_worst_lag_before = final_height_gap_before;
                                rt.final_quiescence_worst_lag_after = final_height_gap_before;
                            }
                            if final_same_height_reconcile_block {
                                final_quiescence_same_height_tip_requests.remove(&block.hash);
                                rt.final_quiescence_same_height_competing_tip_fetch_success_total =
                                    rt.final_quiescence_same_height_competing_tip_fetch_success_total
                                        .saturating_add(1);
                                rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                    .final_quiescence_same_height_reconcile_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_same_height_reconcile_blocked_reason = Some(
                                    final_same_height_reconcile_rejection_reason(&acceptance)
                                        .to_string(),
                                );
                            }
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
                                update_orphan_backlog_classification(&mut rt, &guard);
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
                                update_orphan_backlog_classification(&mut rt, &guard);
                            }
                            if let Err(e) = storage.persist_chain_state(&guard) {
                                warn!(error = %e, "failed persisting chain state after orphan queue");
                            }
                        } else if !acceptance.is_accepted() {
                            let invalid_state_root_diagnostics = match &acceptance {
                                BlockAcceptanceResult::Rejected(message)
                                    if message.contains("invalid state root") =>
                                {
                                    pulsedag_core::validation::compute_post_state_root(
                                        &block, &guard,
                                    )
                                    .ok()
                                    .map(|computed| {
                                        pulsedag_core::invalid_state_root_diagnostics(
                                            &block, &guard, computed,
                                        )
                                    })
                                }
                                _ => None,
                            };
                            if let Some(diagnostics) = &invalid_state_root_diagnostics {
                                warn!(
                                    event = "invalid_state_root_rejected",
                                    block_hash = %diagnostics.block_hash,
                                    height = diagnostics.height,
                                    parents = ?diagnostics.parent_hashes,
                                    supplied_state_root = %diagnostics.supplied_state_root,
                                    computed_state_root = %diagnostics.computed_state_root,
                                    tx_count = diagnostics.tx_count,
                                    coinbase_miner = ?diagnostics.coinbase_miner_address,
                                    selected_tip = ?diagnostics.selected_tip,
                                    selected_tip_height = ?diagnostics.selected_tip_height,
                                    current_tips = ?diagnostics.current_tips,
                                    stale_template = diagnostics.stale_template,
                                    unknown_context = diagnostics.unknown_context,
                                    classification = diagnostics.classification.as_str(),
                                    "rejected inbound p2p block with invalid state root"
                                );
                            }
                            let mut rt = runtime.write().await;
                            if final_height_reconcile_block {
                                final_quiescence_higher_tip_requests.remove(&block.hash);
                                rt.final_quiescence_higher_tip_fetch_success_total = rt
                                    .final_quiescence_higher_tip_fetch_success_total
                                    .saturating_add(1);
                                rt.final_quiescence_higher_tip_apply_rejected_total = rt
                                    .final_quiescence_higher_tip_apply_rejected_total
                                    .saturating_add(1);
                                rt.final_quiescence_missing_segment_apply_rejected_total = rt
                                    .final_quiescence_missing_segment_apply_rejected_total
                                    .saturating_add(1);
                                rt.final_quiescence_selected_sync_blocked_total = rt
                                    .final_quiescence_selected_sync_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_selected_sync_blocked_reason =
                                    Some("block_received_but_validation_failed".to_string());
                                rt.final_quiescence_height_reconcile_blocked_total = rt
                                    .final_quiescence_height_reconcile_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_height_reconcile_blocked_reason = Some(
                                    final_height_reconcile_rejection_reason(&acceptance)
                                        .to_string(),
                                );
                                rt.final_quiescence_height_gap_before = final_height_gap_before;
                                rt.final_quiescence_height_gap_after = final_height_gap_before;
                                rt.final_quiescence_worst_lag_before = final_height_gap_before;
                                rt.final_quiescence_worst_lag_after = final_height_gap_before;
                            }
                            if final_same_height_reconcile_block {
                                final_quiescence_same_height_tip_requests.remove(&block.hash);
                                rt.final_quiescence_same_height_competing_tip_fetch_success_total =
                                    rt.final_quiescence_same_height_competing_tip_fetch_success_total
                                        .saturating_add(1);
                                rt.final_quiescence_same_height_competing_tip_apply_rejected_total =
                                    rt.final_quiescence_same_height_competing_tip_apply_rejected_total
                                        .saturating_add(1);
                                rt.final_quiescence_missing_segment_apply_rejected_total = rt
                                    .final_quiescence_missing_segment_apply_rejected_total
                                    .saturating_add(1);
                                rt.final_quiescence_selected_sync_blocked_total = rt
                                    .final_quiescence_selected_sync_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_selected_sync_blocked_reason =
                                    Some("block_received_but_validation_failed".to_string());
                                rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                    .final_quiescence_same_height_reconcile_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_same_height_reconcile_blocked_reason = Some(
                                    final_same_height_reconcile_rejection_reason(&acceptance)
                                        .to_string(),
                                );
                            }
                            rt.blockdata_received = rt.blockdata_received.saturating_add(1);
                            if let Some(diagnostics) = &invalid_state_root_diagnostics {
                                rt.record_invalid_state_root(diagnostics);
                            }
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
                            block_requests.resolve(&block.hash);
                            let (adopted, retried_orphans, adopted_hashes, failure_reasons) = {
                                let mut adopted_guard = guard.clone();
                                if adopted_guard.orphan_parent_index.is_empty()
                                    && !adopted_guard.orphan_blocks.is_empty()
                                {
                                    let rebuilt = pulsedag_core::rebuild_orphan_parent_index(
                                        &mut adopted_guard,
                                    );
                                    info!(
                                        event = "orphan_parent_index_rebuilt",
                                        accepted_parent = %block.hash,
                                        retryable_ready = rebuilt.retryable_ready,
                                        waiting_missing_parent = rebuilt.waiting_missing_parent,
                                        stale_missing_parent_entries = rebuilt.stale_missing_parent_entries,
                                        unindexed_missing_parent_entries = rebuilt.unindexed_missing_parent_entries,
                                        "rebuilt orphan parent index before inbound orphan adoption"
                                    );
                                }
                                let targeted = pulsedag_core::adopt_ready_orphans_with_result(
                                    &mut adopted_guard,
                                    AcceptSource::P2p,
                                    Some(&block.hash),
                                );
                                let mut adopted = targeted.accepted;
                                let mut retried = targeted.retried;
                                let mut failure_reasons = targeted.failure_reasons;
                                let mut adopted_hashes = targeted.accepted_hashes;
                                if !adopted_guard.orphan_blocks.is_empty() {
                                    let bounded = pulsedag_core::adopt_ready_orphans_with_result(
                                        &mut adopted_guard,
                                        AcceptSource::P2p,
                                        None,
                                    );
                                    adopted = adopted.saturating_add(bounded.accepted);
                                    retried = retried.saturating_add(bounded.retried);
                                    adopted_hashes.extend(bounded.accepted_hashes);
                                    for (reason, count) in bounded.failure_reasons {
                                        let entry = failure_reasons.entry(reason).or_insert(0);
                                        *entry = entry.saturating_add(count);
                                    }
                                }
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
                            let final_height_gap_after =
                                block.header.height.saturating_sub(accepted_height);
                            let accepted_tip = pulsedag_core::preferred_tip_hash(&guard)
                                .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                            {
                                let mut rt = runtime.write().await;
                                if final_height_reconcile_block {
                                    final_quiescence_higher_tip_requests.remove(&block.hash);
                                    rt.final_quiescence_higher_tip_fetch_success_total = rt
                                        .final_quiescence_higher_tip_fetch_success_total
                                        .saturating_add(1);
                                    rt.final_quiescence_higher_tip_apply_success_total = rt
                                        .final_quiescence_higher_tip_apply_success_total
                                        .saturating_add(1);
                                    rt.final_quiescence_missing_segment_apply_success_total = rt
                                        .final_quiescence_missing_segment_apply_success_total
                                        .saturating_add(1);
                                    rt.final_quiescence_selected_sync_success_total = rt
                                        .final_quiescence_selected_sync_success_total
                                        .saturating_add(1);
                                    rt.final_quiescence_height_reconcile_success_total = rt
                                        .final_quiescence_height_reconcile_success_total
                                        .saturating_add(1);
                                    rt.final_quiescence_height_gap_before = final_height_gap_before;
                                    rt.final_quiescence_height_gap_after = final_height_gap_after;
                                    rt.final_quiescence_worst_lag_before = final_height_gap_before;
                                    rt.final_quiescence_worst_lag_after = final_height_gap_after;
                                    rt.final_quiescence_height_reconcile_blocked_reason = None;
                                }
                                if final_same_height_reconcile_block {
                                    final_quiescence_same_height_tip_requests.remove(&block.hash);
                                    rt.final_quiescence_same_height_competing_tip_fetch_success_total =
                                        rt.final_quiescence_same_height_competing_tip_fetch_success_total
                                            .saturating_add(1);
                                    rt.final_quiescence_same_height_competing_tip_apply_success_total =
                                        rt.final_quiescence_same_height_competing_tip_apply_success_total
                                            .saturating_add(1);
                                    rt.final_quiescence_same_height_candidate_apply_total = rt
                                        .final_quiescence_same_height_candidate_apply_total
                                        .saturating_add(1);
                                    let selected_tip = pulsedag_core::preferred_tip_hash(&guard)
                                        .unwrap_or_else(|| guard.dag.genesis_hash.clone());
                                    if selected_tip == accepted_tip {
                                        rt.final_quiescence_same_height_reconcile_success_total =
                                            rt.final_quiescence_same_height_reconcile_success_total
                                                .saturating_add(1);
                                        rt.final_quiescence_same_height_reconcile_blocked_reason =
                                            None;
                                    } else {
                                        rt.final_quiescence_same_height_reconcile_blocked_total =
                                            rt.final_quiescence_same_height_reconcile_blocked_total
                                                .saturating_add(1);
                                        rt.final_quiescence_same_height_reconcile_blocked_reason =
                                            Some(
                                                "same_height_selected_tip_not_applied".to_string(),
                                            );
                                    }
                                    rt.final_quiescence_distinct_tips_after =
                                        guard.dag.tips.len() as u64;
                                }
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
                                update_orphan_backlog_classification(&mut rt, &guard);
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
                        let planned_request_count = plan.requests.len() as u64;
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
                        if !headers.is_empty() {
                            rt.final_quiescence_selected_locator_success_total = rt
                                .final_quiescence_selected_locator_success_total
                                .saturating_add(1);
                            rt.final_quiescence_highest_common_found_total = rt
                                .final_quiescence_highest_common_found_total
                                .saturating_add(1);
                        } else if final_quiescence_reconcile_pending(
                            rt.final_quiescence_selected_sync_total,
                            rt.final_quiescence_selected_sync_success_total,
                            rt.final_quiescence_selected_sync_blocked_total,
                        ) {
                            rt.final_quiescence_selected_locator_empty_total = rt
                                .final_quiescence_selected_locator_empty_total
                                .saturating_add(1);
                            rt.final_quiescence_selected_sync_blocked_total = rt
                                .final_quiescence_selected_sync_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_selected_sync_blocked_reason =
                                Some("selected_locator_empty".to_string());
                        }
                        rt.final_quiescence_missing_segment_request_total = rt
                            .final_quiescence_missing_segment_request_total
                            .saturating_add(planned_request_count);
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
                            let final_reconcile_pending = final_quiescence_reconcile_pending(
                                rt.final_quiescence_tip_reconcile_total,
                                rt.final_quiescence_tip_reconcile_success_total,
                                rt.final_quiescence_tip_reconcile_blocked_total,
                            );
                            let same_height_reconcile_pending = final_quiescence_reconcile_pending(
                                rt.final_quiescence_same_height_reconcile_total,
                                rt.final_quiescence_same_height_reconcile_success_total,
                                rt.final_quiescence_same_height_reconcile_blocked_total,
                            );
                            if final_reconcile_pending {
                                if unknown_tips.is_empty() {
                                    rt.final_quiescence_tip_reconcile_blocked_total = rt
                                        .final_quiescence_tip_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_tip_reconcile_blocked_reason = Some(
                                        "no_connected_peer_with_better_or_competing_tip"
                                            .to_string(),
                                    );
                                    if final_quiescence_reconcile_pending(
                                        rt.final_quiescence_height_reconcile_total,
                                        rt.final_quiescence_height_reconcile_success_total,
                                        rt.final_quiescence_height_reconcile_blocked_total,
                                    ) {
                                        rt.final_quiescence_height_reconcile_blocked_total = rt
                                            .final_quiescence_height_reconcile_blocked_total
                                            .saturating_add(1);
                                        rt.final_quiescence_height_reconcile_blocked_reason = Some(
                                            "no_connected_peer_with_better_or_competing_tip"
                                                .to_string(),
                                        );
                                    }
                                    if same_height_reconcile_pending {
                                        rt.final_quiescence_same_height_reconcile_blocked_total =
                                            rt.final_quiescence_same_height_reconcile_blocked_total
                                                .saturating_add(1);
                                        rt.final_quiescence_same_height_reconcile_blocked_reason =
                                            Some(
                                                "no_connected_peer_with_better_or_competing_tip"
                                                    .to_string(),
                                            );
                                    }
                                } else {
                                    rt.final_quiescence_tip_reconcile_success_total = rt
                                        .final_quiescence_tip_reconcile_success_total
                                        .saturating_add(1);
                                    rt.final_quiescence_tip_reconcile_blocked_reason = None;
                                }
                            }
                            rt.sync_state = if unknown_tips.is_empty() {
                                "synced"
                            } else {
                                "requesting_blocks"
                            }
                            .to_string();
                        }
                        for tip in unknown_tips {
                            let final_height_pending = {
                                let rt = runtime.read().await;
                                final_quiescence_reconcile_pending(
                                    rt.final_quiescence_height_reconcile_total,
                                    rt.final_quiescence_height_reconcile_success_total,
                                    rt.final_quiescence_height_reconcile_blocked_total,
                                ) || final_quiescence_reconcile_pending(
                                    rt.final_quiescence_tip_reconcile_total,
                                    rt.final_quiescence_tip_reconcile_success_total,
                                    rt.final_quiescence_tip_reconcile_blocked_total,
                                )
                            };
                            let final_same_height_pending = {
                                let rt = runtime.read().await;
                                final_quiescence_reconcile_pending(
                                    rt.final_quiescence_same_height_reconcile_total,
                                    rt.final_quiescence_same_height_reconcile_success_total,
                                    rt.final_quiescence_same_height_reconcile_blocked_total,
                                )
                            };
                            if final_height_pending {
                                let mut rt = runtime.write().await;
                                rt.final_quiescence_higher_tip_seen_total =
                                    rt.final_quiescence_higher_tip_seen_total.saturating_add(1);
                                rt.final_quiescence_selected_sync_total =
                                    rt.final_quiescence_selected_sync_total.saturating_add(1);
                            }
                            if final_same_height_pending {
                                let mut rt = runtime.write().await;
                                rt.final_quiescence_same_height_competing_tip_seen_total = rt
                                    .final_quiescence_same_height_competing_tip_seen_total
                                    .saturating_add(1);
                                rt.final_quiescence_same_height_candidate_seen_total = rt
                                    .final_quiescence_same_height_candidate_seen_total
                                    .saturating_add(1);
                                rt.final_quiescence_selected_sync_total =
                                    rt.final_quiescence_selected_sync_total.saturating_add(1);
                            }
                            let readiness = block_requests.classify_getblock_for_peers(
                                &tip,
                                now_unix(),
                                active_peer_ids(&p2p),
                            );
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
                                if final_height_pending {
                                    final_quiescence_higher_tip_requests.insert(tip.clone());
                                    rt.final_quiescence_higher_tip_fetch_attempt_total = rt
                                        .final_quiescence_higher_tip_fetch_attempt_total
                                        .saturating_add(1);
                                    rt.final_quiescence_missing_segment_request_total = rt
                                        .final_quiescence_missing_segment_request_total
                                        .saturating_add(1);
                                    rt.final_quiescence_height_reconcile_blocked_reason = None;
                                }
                                if final_same_height_pending {
                                    final_quiescence_same_height_tip_requests.insert(tip.clone());
                                    rt.final_quiescence_same_height_competing_tip_fetch_attempt_total =
                                        rt.final_quiescence_same_height_competing_tip_fetch_attempt_total
                                            .saturating_add(1);
                                    rt.final_quiescence_same_height_candidate_fetch_total = rt
                                        .final_quiescence_same_height_candidate_fetch_total
                                        .saturating_add(1);
                                    rt.final_quiescence_same_height_reconcile_blocked_reason = None;
                                }
                                rt.final_quiescence_selected_sync_blocked_reason =
                                    Some("block_request_sent".to_string());
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                            } else {
                                let reason = final_quiescence_request_suppression_reason(
                                    readiness,
                                    block_requests.pending.len(),
                                    block_requests.max_pending(),
                                );
                                let mut rt = runtime.write().await;
                                if final_height_pending {
                                    rt.final_quiescence_height_reconcile_blocked_total = rt
                                        .final_quiescence_height_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_height_reconcile_blocked_reason =
                                        Some(reason.to_string());
                                }
                                if final_same_height_pending {
                                    rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                        .final_quiescence_same_height_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_same_height_reconcile_blocked_reason =
                                        Some(reason.to_string());
                                }
                                rt.final_quiescence_selected_sync_blocked_total = rt
                                    .final_quiescence_selected_sync_blocked_total
                                    .saturating_add(1);
                                rt.final_quiescence_selected_sync_blocked_reason =
                                    Some(reason.to_string());
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
                        }
                        .or_else(|| match storage.get_block(&hash) {
                            Ok(block) => block,
                            Err(e) => {
                                warn!(error = %e, block_hash = %hash, "failed loading historical block for GetBlock response");
                                None
                            }
                        });
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
                        let mut retry_next_peer = false;
                        let mut all_peers_exhausted = false;
                        let mut final_height_not_found = false;
                        let mut final_same_height_not_found = false;
                        if let Some(hash) = hash.as_ref() {
                            final_height_not_found =
                                final_quiescence_higher_tip_requests.contains(hash);
                            final_same_height_not_found =
                                final_quiescence_same_height_tip_requests.contains(hash);
                            let now = now_unix();
                            let peers = p2p
                                .as_ref()
                                .map(active_peer_ids_from_handle)
                                .unwrap_or_default();
                            let outcome = block_requests.note_not_found(hash, now, peers);
                            retry_next_peer = outcome.retry;
                            all_peers_exhausted = outcome.all_peers_exhausted;
                            if let Some(ref p2p) = p2p {
                                if let Err(e) = p2p.request_headers(&[], Some(hash), 128) {
                                    warn!(error = %e, block_hash = %hash, "failed issuing fallback headers after BlockData not-found");
                                }
                                if outcome.retry {
                                    if let Err(e) = p2p.request_block(hash) {
                                        warn!(error = %e, block_hash = %hash, "failed issuing fallback GetBlock after BlockData not-found");
                                    } else {
                                        fallback_getblock_sent = true;
                                    }
                                }
                            }
                        }
                        if all_peers_exhausted {
                            if let Some(hash) = hash.as_ref() {
                                let peers = p2p
                                    .as_ref()
                                    .map(active_peer_ids_from_handle)
                                    .unwrap_or_default();
                                terminally_handle_exhausted_missing_parent(
                                    &chain, &runtime, hash, peers,
                                )
                                .await;
                            }
                        }
                        let mut rt = runtime.write().await;
                        if final_height_not_found && all_peers_exhausted {
                            if let Some(hash) = hash.as_ref() {
                                final_quiescence_higher_tip_requests.remove(hash);
                            }
                            rt.final_quiescence_height_reconcile_blocked_total = rt
                                .final_quiescence_height_reconcile_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_height_reconcile_blocked_reason =
                                Some("storage_missing_after_receive".to_string());
                        }
                        if final_same_height_not_found && all_peers_exhausted {
                            if let Some(hash) = hash.as_ref() {
                                final_quiescence_same_height_tip_requests.remove(hash);
                            }
                            rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                .final_quiescence_same_height_reconcile_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_same_height_reconcile_blocked_reason =
                                Some("storage_missing_after_receive".to_string());
                        }
                        rt.sync_state = "degraded".to_string();
                        rt.sync_failures = rt.sync_failures.saturating_add(1);
                        rt.blockdata_not_found = rt.blockdata_not_found.saturating_add(1);
                        rt.block_request_fallbacks = rt.block_request_fallbacks.saturating_add(1);
                        rt.missing_parent_request_fallbacks =
                            rt.missing_parent_request_fallbacks.saturating_add(1);
                        if hash.is_some() {
                            rt.missing_parent_peer_not_found_total =
                                rt.missing_parent_peer_not_found_total.saturating_add(1);
                        }
                        if retry_next_peer {
                            rt.missing_parent_retry_next_peer_total =
                                rt.missing_parent_retry_next_peer_total.saturating_add(1);
                            rt.missing_parent_retry_peer_total =
                                rt.missing_parent_retry_peer_total.saturating_add(1);
                        }
                        if all_peers_exhausted {
                            rt.missing_parent_all_peers_exhausted_total = rt
                                .missing_parent_all_peers_exhausted_total
                                .saturating_add(1);
                        }
                        if fallback_getblock_sent {
                            rt.getblock_sent = rt.getblock_sent.saturating_add(1);
                            rt.missing_parent_requests_sent =
                                rt.missing_parent_requests_sent.saturating_add(1);
                            rt.missing_parent_request_started_total =
                                rt.missing_parent_request_started_total.saturating_add(1);
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
                let (
                    issue_count,
                    best_height,
                    best_tip_hash,
                    orphan_count,
                    local_tip_count,
                    local_missing_parent_entries,
                    local_pending_missing_parents,
                    mempool_size,
                ) = {
                    let guard = chain.read().await;
                    (
                        pulsedag_core::dag_consistency_issues(&guard).len(),
                        guard.dag.best_height,
                        pulsedag_core::preferred_tip_hash(&guard)
                            .unwrap_or_else(|| guard.dag.genesis_hash.clone()),
                        guard.orphan_blocks.len(),
                        guard.dag.tips.len(),
                        guard.orphan_missing_parents.len(),
                        pulsedag_core::pending_missing_parent_count(&guard),
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
                let final_quiescence_due = stagnation_secs >= FINAL_QUIESCENCE_NO_PROGRESS_SECS;
                let pending_block_requests = rt.pending_block_requests;
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
                drop(rt);

                if final_quiescence_due {
                    let cleanup_complete = orphan_count == 0
                        && local_pending_missing_parents == 0
                        && local_missing_parent_entries == 0
                        && pending_block_requests == 0;

                    // Never run final tip reconciliation while peer recovery or orphan cleanup has
                    // work left.  In particular, zero-peer recovery must be allowed to run before
                    // final sync, and selected/same-height sync must not run with peer_count=0.
                    if cleanup_complete {
                        if let Some(ref p2p) = p2p {
                            match p2p.status() {
                                Ok(status) if !status.connected_peers.is_empty() => {
                                    let all_reachable_peers_connected =
                                        final_quiescence_all_reachable_peers_connected(&status);
                                    let same_height_tip_reconcile_ready =
                                        all_reachable_peers_connected && local_tip_count > 1;
                                    let requested = p2p.request_tips().is_ok();
                                    let mut rt = runtime.write().await;
                                    rt.final_quiescence_tip_reconcile_total =
                                        rt.final_quiescence_tip_reconcile_total.saturating_add(1);
                                    let height_reconcile_already_pending =
                                        final_quiescence_reconcile_pending(
                                            rt.final_quiescence_height_reconcile_total,
                                            rt.final_quiescence_height_reconcile_success_total,
                                            rt.final_quiescence_height_reconcile_blocked_total,
                                        );
                                    let same_height_reconcile_already_pending =
                                        final_quiescence_reconcile_pending(
                                            rt.final_quiescence_same_height_reconcile_total,
                                            rt.final_quiescence_same_height_reconcile_success_total,
                                            rt.final_quiescence_same_height_reconcile_blocked_total,
                                        );
                                    if !height_reconcile_already_pending {
                                        rt.final_quiescence_height_reconcile_total = rt
                                            .final_quiescence_height_reconcile_total
                                            .saturating_add(1);
                                    }
                                    if same_height_tip_reconcile_ready
                                        && !height_reconcile_already_pending
                                        && !same_height_reconcile_already_pending
                                    {
                                        rt.final_quiescence_same_height_reconcile_total = rt
                                            .final_quiescence_same_height_reconcile_total
                                            .saturating_add(1);
                                        rt.final_quiescence_distinct_tips_before =
                                            local_tip_count as u64;
                                        rt.final_quiescence_worst_lag_before = 0;
                                    }
                                    if requested {
                                        rt.tips_requested = rt.tips_requested.saturating_add(1);
                                        rt.final_quiescence_tip_reconcile_blocked_reason = None;
                                        rt.final_quiescence_height_reconcile_blocked_reason = None;
                                        if same_height_tip_reconcile_ready {
                                            rt.final_quiescence_same_height_reconcile_blocked_reason = None;
                                        }
                                    } else {
                                        rt.final_quiescence_tip_reconcile_blocked_total = rt
                                            .final_quiescence_tip_reconcile_blocked_total
                                            .saturating_add(1);
                                        rt.final_quiescence_tip_reconcile_blocked_reason =
                                            Some("request_tips_failed".to_string());
                                        rt.final_quiescence_height_reconcile_blocked_total = rt
                                            .final_quiescence_height_reconcile_blocked_total
                                            .saturating_add(1);
                                        rt.final_quiescence_height_reconcile_blocked_reason =
                                            Some("request_tips_failed".to_string());
                                        if same_height_tip_reconcile_ready {
                                            rt.final_quiescence_same_height_reconcile_blocked_total = rt
                                                .final_quiescence_same_height_reconcile_blocked_total
                                                .saturating_add(1);
                                            rt.final_quiescence_same_height_reconcile_blocked_reason =
                                                Some("request_tips_failed".to_string());
                                        }
                                    }
                                }
                                Ok(_) => {
                                    let mut rt = runtime.write().await;
                                    rt.final_quiescence_tip_reconcile_total =
                                        rt.final_quiescence_tip_reconcile_total.saturating_add(1);
                                    rt.final_quiescence_height_reconcile_total = rt
                                        .final_quiescence_height_reconcile_total
                                        .saturating_add(1);
                                    rt.final_quiescence_tip_reconcile_blocked_total = rt
                                        .final_quiescence_tip_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_tip_reconcile_blocked_reason =
                                        Some("no_connected_peers".to_string());
                                    rt.final_quiescence_height_reconcile_blocked_total = rt
                                        .final_quiescence_height_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_height_reconcile_blocked_reason =
                                        Some("no_connected_peers".to_string());
                                }
                                Err(e) => {
                                    let mut rt = runtime.write().await;
                                    rt.final_quiescence_tip_reconcile_total =
                                        rt.final_quiescence_tip_reconcile_total.saturating_add(1);
                                    rt.final_quiescence_height_reconcile_total = rt
                                        .final_quiescence_height_reconcile_total
                                        .saturating_add(1);
                                    rt.final_quiescence_tip_reconcile_blocked_total = rt
                                        .final_quiescence_tip_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_tip_reconcile_blocked_reason =
                                        Some(format!("p2p_status_failed:{e}"));
                                    rt.final_quiescence_height_reconcile_blocked_total = rt
                                        .final_quiescence_height_reconcile_blocked_total
                                        .saturating_add(1);
                                    rt.final_quiescence_height_reconcile_blocked_reason =
                                        Some(format!("p2p_status_failed:{e}"));
                                }
                            }
                        } else {
                            let mut rt = runtime.write().await;
                            rt.final_quiescence_tip_reconcile_total =
                                rt.final_quiescence_tip_reconcile_total.saturating_add(1);
                            rt.final_quiescence_height_reconcile_total =
                                rt.final_quiescence_height_reconcile_total.saturating_add(1);
                            rt.final_quiescence_tip_reconcile_blocked_total = rt
                                .final_quiescence_tip_reconcile_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_tip_reconcile_blocked_reason =
                                Some("p2p_disabled".to_string());
                            rt.final_quiescence_height_reconcile_blocked_total = rt
                                .final_quiescence_height_reconcile_blocked_total
                                .saturating_add(1);
                            rt.final_quiescence_height_reconcile_blocked_reason =
                                Some("p2p_disabled".to_string());
                        }
                    }

                    if orphan_count > 0 && pending_block_requests == 0 {
                        let cleanup = {
                            let mut guard = chain.write().await;
                            let cleanup = run_final_quiescence_orphan_cleanup(
                                &mut guard,
                                now.saturating_mul(1_000),
                                FINAL_QUIESCENCE_NO_PROGRESS_SECS.saturating_mul(1_000),
                                FINAL_QUIESCENCE_CLEANUP_LIMIT,
                            );
                            if cleanup.reprocess_attempts > 0
                                || cleanup.reprocess_success > 0
                                || cleanup.terminalized_missing_parents > 0
                            {
                                if let Err(e) = storage.persist_chain_state(&guard) {
                                    warn!(error = %e, "failed persisting chain state after final quiescence orphan cleanup");
                                }
                            }
                            cleanup
                        };
                        let mut rt = runtime.write().await;
                        rt.final_quiescence_orphan_reprocess_total = rt
                            .final_quiescence_orphan_reprocess_total
                            .saturating_add(cleanup.reprocess_attempts.max(1) as u64);
                        rt.final_quiescence_orphan_reprocess_success_total = rt
                            .final_quiescence_orphan_reprocess_success_total
                            .saturating_add(cleanup.reprocess_success as u64);
                        rt.final_quiescence_orphan_terminalized_total = rt
                            .final_quiescence_orphan_terminalized_total
                            .saturating_add(cleanup.terminalized_orphans as u64);
                        rt.final_quiescence_missing_parent_terminalized_total = rt
                            .final_quiescence_missing_parent_terminalized_total
                            .saturating_add(cleanup.terminalized_missing_parents as u64);
                        rt.final_quiescence_missing_parent_quarantined_total = rt
                            .final_quiescence_missing_parent_quarantined_total
                            .saturating_add(cleanup.quarantined_missing_parents as u64);
                        rt.pending_missing_parents = cleanup.active_missing_parent_entries;
                        rt.missing_parent_index_active_entries =
                            cleanup.active_missing_parent_entries;
                        rt.missing_parent_index_terminal_entries =
                            cleanup.terminal_missing_parent_entries;
                        rt.orphan_missing_parent_quarantined_total = cleanup
                            .quarantined_missing_parent_entries
                            .try_into()
                            .unwrap_or(u64::MAX);
                    }
                }
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
    let runtime_stats_handle = app_state.runtime.clone();
    let app: Router = router_with_profile(
        rpc_profile,
        cfg.admin_enabled,
        cfg.operator_auth_token.clone(),
        Some(hardening_limits),
    )
    .layer(cors_layer)
    .with_state(app_state);
    let addr: SocketAddr = cfg.rpc_bind.parse()?;
    let rpc_server = spawn_dedicated_rpc_server(addr, app, DEDICATED_RPC_RUNTIME_WORKER_THREADS)?;
    let rpc_addr = rpc_server.local_addr;
    let rpc_worker_threads = rpc_server.worker_threads;

    {
        let mut runtime = runtime_stats_handle.write().await;
        runtime.rpc_dedicated_runtime_active = true;
        runtime.rpc_dedicated_runtime_worker_threads = rpc_worker_threads;
    }

    if cfg.admin_enabled {
        warn!(rpc_bind = %cfg.rpc_bind, api_profile = ?cfg.api_profile, "admin RPC endpoints are ENABLED; restrict access and avoid unauthenticated exposure");
    }
    if !config::is_local_rpc_bind(&cfg.rpc_bind) || cfg.rpc_bind.starts_with("0.0.0.0:") {
        warn!(rpc_bind = %cfg.rpc_bind, "RPC is bound beyond localhost; verify firewall rules, auth controls, and API profile before exposing this port");
    }

    info!(p2p_enabled = cfg.p2p_enabled, p2p_mode = %cfg.p2p_mode, admin_enabled = cfg.admin_enabled, operator_auth_configured = cfg.operator_auth_token.is_some(), api_profile = ?cfg.api_profile, auto_rebuild_on_start = cfg.auto_rebuild_on_start, persist_snapshot_on_start = cfg.persist_snapshot_on_start, snapshot_auto_every_blocks = cfg.snapshot_auto_every_blocks, auto_prune_enabled = cfg.auto_prune_enabled, auto_prune_every_blocks = cfg.auto_prune_every_blocks, prune_keep_recent_blocks = cfg.prune_keep_recent_blocks, prune_require_snapshot = cfg.prune_require_snapshot, target_block_interval_secs = cfg.target_block_interval_secs, difficulty_window = cfg.difficulty_window, max_future_drift_secs = cfg.max_future_drift_secs, dedicated_rpc_runtime_active = true, dedicated_rpc_runtime_worker_threads = rpc_worker_threads, "pulsedagd RPC listening on {} using dedicated Tokio runtime", rpc_addr);

    let mut rpc_server = rpc_server;
    let rpc_runtime_join = rpc_server
        .join
        .take()
        .ok_or_else(|| anyhow::anyhow!("dedicated RPC runtime thread already joined"))?;
    let rpc_join = tokio::task::spawn_blocking(move || join_rpc_runtime_thread(rpc_runtime_join));
    match rpc_join.await {
        Ok(result) => result,
        Err(e) => Err(anyhow::anyhow!("dedicated RPC runtime join failed: {e}")),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_orphan(hash: &str, parents: Vec<&str>, height: u64) -> pulsedag_core::Block {
        let header = pulsedag_core::BlockHeader {
            version: 1,
            parents: parents.into_iter().map(str::to_string).collect(),
            timestamp: 1,
            difficulty: 1,
            nonce: 0,
            merkle_root: pulsedag_core::compute_merkle_root(&[]),
            state_root: "state".to_string(),
            blue_score: height,
            height,
        };
        pulsedag_core::Block {
            hash: hash.to_string(),
            header,
            transactions: Vec::new(),
        }
    }

    fn chain_with_unindexed_orphan() -> pulsedag_core::ChainState {
        let mut chain = pulsedag_core::genesis::init_chain_state("test-chain".to_string());
        let orphan = test_orphan("orphan-a", vec!["missing-parent-a"], 1);
        pulsedag_core::queue_orphan_block(&mut chain, orphan, vec!["missing-parent-a".to_string()]);
        chain.orphan_missing_parents.clear();
        chain.orphan_parent_index.clear();
        chain
    }

    #[test]
    fn final_quiescence_caps_slot_budget_to_configured_reachable_peers() {
        let status = pulsedag_p2p::P2pStatus {
            connected_peers: vec![
                "peer-a".to_string(),
                "peer-b".to_string(),
                "peer-c".to_string(),
                "peer-d".to_string(),
            ],
            bootnodes_configured: vec![
                "/ip4/127.0.0.1/tcp/1/p2p/peer-a".to_string(),
                "/ip4/127.0.0.1/tcp/2/p2p/peer-b".to_string(),
                "/ip4/127.0.0.1/tcp/3/p2p/peer-c".to_string(),
                "/ip4/127.0.0.1/tcp/4/p2p/peer-d".to_string(),
            ],
            connection_slot_budget: 8,
            ..pulsedag_p2p::P2pStatus::default()
        };

        assert_eq!(final_quiescence_reachable_peer_count(&status), 4);
        assert!(final_quiescence_all_reachable_peers_connected(&status));
    }

    #[test]
    fn final_quiescence_waits_for_known_reachable_peers_not_just_connected_subset() {
        let status = pulsedag_p2p::P2pStatus {
            connected_peers: vec![
                "peer-a".to_string(),
                "peer-b".to_string(),
                "peer-c".to_string(),
            ],
            bootnodes_configured: vec![
                "/ip4/127.0.0.1/tcp/1/p2p/peer-a".to_string(),
                "/ip4/127.0.0.1/tcp/2/p2p/peer-b".to_string(),
                "/ip4/127.0.0.1/tcp/3/p2p/peer-c".to_string(),
                "/ip4/127.0.0.1/tcp/4/p2p/peer-d".to_string(),
            ],
            connection_slot_budget: 8,
            ..pulsedag_p2p::P2pStatus::default()
        };

        assert_eq!(final_quiescence_reachable_peer_count(&status), 4);
        assert!(!final_quiescence_all_reachable_peers_connected(&status));
    }

    #[test]
    fn orphan_count_with_empty_entries_and_no_inv_triggers_forced_reindex() {
        let chain = chain_with_unindexed_orphan();
        assert!(should_force_orphan_missing_parent_reindex(&chain, 0));

        let mut rebuilt = chain.clone();
        let classification = pulsedag_core::rebuild_orphan_parent_index(&mut rebuilt);
        assert_eq!(classification.waiting_missing_parent, 1);
        assert_eq!(rebuilt.orphan_missing_parents.len(), 1);
        assert_eq!(rebuilt.orphan_parent_index.len(), 1);
    }

    #[test]
    fn pending_missing_with_no_inv_triggers_forced_reindex() {
        let chain = chain_with_unindexed_orphan();
        assert_eq!(pulsedag_core::pending_missing_parent_count(&chain), 1);
        assert!(should_force_orphan_missing_parent_reindex(&chain, 0));
        assert!(!should_force_orphan_missing_parent_reindex(&chain, 1));
    }

    #[test]
    fn forced_reindex_classifies_every_missing_parent_root() {
        let mut chain = chain_with_unindexed_orphan();
        pulsedag_core::rebuild_orphan_parent_index(&mut chain);
        let roots = orphan_recovery_roots(&chain);
        assert_eq!(roots, vec!["missing-parent-a".to_string()]);

        let block_requests = BlockRequestTracker::with_limit(1, 1, 16);
        let classes = classify_orphan_recovery_roots(
            roots,
            &block_requests,
            &["peer-a".to_string()],
            true,
            1,
        );
        assert_eq!(classes.requestable, vec!["missing-parent-a".to_string()]);
        assert_eq!(classes.total_classified(), 1);
    }

    #[test]
    fn exhausted_missing_parent_root_is_classified_terminally() {
        let mut chain = chain_with_unindexed_orphan();
        pulsedag_core::rebuild_orphan_parent_index(&mut chain);
        let mut block_requests = BlockRequestTracker::with_limit(1, 1, 16);
        assert!(block_requests.should_issue_getblock_for_peers("missing-parent-a", 1, ["peer-a"]));
        let outcome = block_requests.note_not_found("missing-parent-a", 2, ["peer-a"]);
        assert!(outcome.all_peers_exhausted);

        let classes = classify_orphan_recovery_roots(
            orphan_recovery_roots(&chain),
            &block_requests,
            &["peer-a".to_string()],
            true,
            2,
        );

        assert_eq!(
            classes.all_peers_exhausted,
            vec!["missing-parent-a".to_string()]
        );
        assert_eq!(classes.total_classified(), 1);
    }

    #[test]
    fn stale_unknown_peerless_backlog_is_classified_and_bounded_evicted() {
        let mut chain = pulsedag_core::genesis::init_chain_state("test-chain".to_string());
        for idx in 0..3 {
            let parent = format!("missing-parent-{idx}");
            let orphan = test_orphan(&format!("orphan-{idx}"), vec![&parent], 1);
            pulsedag_core::queue_orphan_block(&mut chain, orphan, vec![parent]);
        }
        let now_ms = 10_000;
        for received_at in chain.orphan_received_at_ms.values_mut() {
            *received_at = 0;
        }
        let roots = orphan_recovery_roots(&chain);
        let block_requests = BlockRequestTracker::with_limit(1, 1, 16);
        let mut classes = classify_orphan_recovery_roots(roots, &block_requests, &[], false, 10);
        classify_stale_or_evictable_roots(&chain, now_ms, 1, 2, &mut classes);
        assert_eq!(classes.unknown_peerless.len(), 3);
        assert_eq!(classes.stale.len(), 3);
        assert_eq!(classes.evictable.len(), 2);

        let evicted = pulsedag_core::evict_stale_orphans_bounded(&mut chain, now_ms, 1, 2);
        assert_eq!(evicted, 2);
        assert_eq!(chain.orphan_blocks.len(), 1);
    }

    #[test]
    fn recovery_tick_helpers_remain_bounded() {
        let mut chain = pulsedag_core::genesis::init_chain_state("test-chain".to_string());
        for idx in 0..64 {
            let parent = format!("missing-parent-{idx}");
            let orphan = test_orphan(&format!("orphan-{idx}"), vec![&parent], 1);
            pulsedag_core::queue_orphan_block(&mut chain, orphan, vec![parent]);
        }
        let rebuilt = pulsedag_core::rebuild_orphan_parent_index(&mut chain);
        let roots = orphan_recovery_roots(&chain);
        let block_requests = BlockRequestTracker::with_limit(1, 1, 8);
        let classes = classify_orphan_recovery_roots(
            roots.clone(),
            &block_requests,
            &["peer-a".to_string()],
            true,
            1,
        );
        assert_eq!(rebuilt.waiting_missing_parent, 64);
        assert_eq!(roots.len(), 64);
        assert_eq!(classes.requestable.len(), 64);
        assert_eq!(classes.total_classified(), 64);
    }

    #[test]
    fn final_quiescence_cleanup_terminalizes_residual_missing_parents() {
        let mut chain = pulsedag_core::genesis::init_chain_state("test-chain".to_string());
        let orphan = test_orphan("orphan-final", vec!["missing-final"], 1);
        pulsedag_core::queue_orphan_block(&mut chain, orphan, vec!["missing-final".to_string()]);
        for received_at in chain.orphan_received_at_ms.values_mut() {
            *received_at = 1_000;
        }

        let cleanup = run_final_quiescence_orphan_cleanup(&mut chain, 60_000, 45_000, 8);

        assert_eq!(cleanup.terminalized_missing_parents, 1);
        assert_eq!(cleanup.quarantined_missing_parents, 1);
        assert_eq!(cleanup.active_missing_parent_entries, 0);
        assert_eq!(pulsedag_core::pending_missing_parent_count(&chain), 0);
        assert_eq!(chain.orphan_parent_index.len(), 0);
        assert_eq!(pulsedag_core::quarantined_missing_parent_count(&chain), 1);
    }

    #[test]
    fn final_quiescence_cleanup_does_not_evict_fresh_actionable_entries() {
        let mut chain = pulsedag_core::genesis::init_chain_state("test-chain".to_string());
        let orphan = test_orphan("orphan-fresh", vec!["missing-fresh"], 1);
        pulsedag_core::queue_orphan_block(&mut chain, orphan, vec!["missing-fresh".to_string()]);
        for received_at in chain.orphan_received_at_ms.values_mut() {
            *received_at = 50_000;
        }

        let cleanup = run_final_quiescence_orphan_cleanup(&mut chain, 60_000, 45_000, 8);

        assert_eq!(cleanup.terminalized_missing_parents, 0);
        assert_eq!(cleanup.active_missing_parent_entries, 1);
        assert_eq!(pulsedag_core::pending_missing_parent_count(&chain), 1);
        assert!(chain.orphan_parent_index.contains_key("missing-fresh"));
        assert!(chain.terminal_missing_parents.is_empty());
    }

    #[test]
    fn final_quiescence_cleanup_reprocesses_after_missing_parent_state_changes() {
        let mut chain = pulsedag_core::genesis::init_chain_state("test-chain".to_string());
        let genesis = chain.dag.genesis_hash.clone();
        let ready = test_orphan("ready-child", vec![&genesis], 1);
        pulsedag_core::queue_orphan_block(&mut chain, ready, Vec::new());
        let blocked = test_orphan("blocked-child", vec!["missing-blocked"], 1);
        pulsedag_core::queue_orphan_block(&mut chain, blocked, vec!["missing-blocked".to_string()]);
        for received_at in chain.orphan_received_at_ms.values_mut() {
            *received_at = 1_000;
        }

        let cleanup = run_final_quiescence_orphan_cleanup(&mut chain, 60_000, 45_000, 8);

        assert!(cleanup.reprocess_attempts >= 1);
        assert_eq!(cleanup.terminalized_missing_parents, 1);
        assert_eq!(cleanup.active_missing_parent_entries, 0);
        assert!(!chain.orphan_blocks.contains_key("ready-child"));
        assert_eq!(pulsedag_core::pending_missing_parent_count(&chain), 0);
    }

    #[test]
    fn final_height_reconcile_pending_is_bounded_by_completed_attempts() {
        assert!(!final_quiescence_reconcile_pending(0, 0, 0));
        assert!(final_quiescence_reconcile_pending(1, 0, 0));
        assert!(!final_quiescence_reconcile_pending(1, 1, 0));
        assert!(!final_quiescence_reconcile_pending(1, 0, 1));
    }

    #[test]
    fn final_quiescence_never_reports_vague_block_request_not_sent() {
        assert_eq!(
            final_quiescence_request_suppression_reason(
                GetBlockRequestReadiness::AlreadyPending,
                1,
                64,
            ),
            "request_already_in_flight"
        );
        assert_eq!(
            final_quiescence_request_suppression_reason(
                GetBlockRequestReadiness::RateLimited,
                64,
                64,
            ),
            "request_queue_full"
        );
        assert_eq!(
            final_quiescence_request_suppression_reason(
                GetBlockRequestReadiness::RateLimited,
                1,
                64,
            ),
            "peer_rate_limited"
        );
    }

    #[test]
    fn final_height_reconcile_uses_precise_rejection_reasons() {
        assert_eq!(
            final_height_reconcile_rejection_reason(&BlockAcceptanceResult::MissingParent),
            "parent_missing_after_fetch"
        );
        assert_eq!(
            final_height_reconcile_rejection_reason(&BlockAcceptanceResult::Rejected(
                "invalid state root".to_string()
            )),
            "validation_rejected"
        );
        assert_eq!(
            final_height_reconcile_rejection_reason(&BlockAcceptanceResult::Rejected(
                "storage persist failed".to_string()
            )),
            "storage_missing"
        );
        assert_eq!(
            final_height_reconcile_rejection_reason(&BlockAcceptanceResult::InvalidPow),
            "validation_rejected"
        );
    }

    #[test]
    fn final_same_height_reconcile_uses_precise_rejection_reasons() {
        assert_eq!(
            final_same_height_reconcile_rejection_reason(&BlockAcceptanceResult::MissingParent),
            "same_height_block_received_but_rejected"
        );
        assert_eq!(
            final_same_height_reconcile_rejection_reason(&BlockAcceptanceResult::Rejected(
                "invalid state root".to_string()
            )),
            "same_height_candidate_validation_failed"
        );
        assert_eq!(
            final_same_height_reconcile_rejection_reason(&BlockAcceptanceResult::InvalidPow),
            "same_height_candidate_validation_failed"
        );
    }

    async fn runtime_thread_name() -> String {
        std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dedicated_rpc_server_serves_requests_on_isolated_runtime_thread() {
        let app = Router::new().route("/runtime-thread", get(runtime_thread_name));
        let server = spawn_dedicated_rpc_server(
            "127.0.0.1:0".parse().unwrap(),
            app,
            DEDICATED_RPC_RUNTIME_WORKER_THREADS,
        )
        .expect("dedicated RPC server should start");
        assert_eq!(
            server.worker_threads(),
            DEDICATED_RPC_RUNTIME_WORKER_THREADS
        );

        let mut last_error = None;
        let mut response = String::new();
        for _ in 0..50 {
            match tokio::net::TcpStream::connect(server.local_addr()).await {
                Ok(mut stream) => {
                    stream
                        .write_all(
                            b"GET /runtime-thread HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                        )
                        .await
                        .expect("request should write");
                    stream
                        .read_to_string(&mut response)
                        .await
                        .expect("response should read");
                    break;
                }
                Err(e) => {
                    last_error = Some(e);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }

        assert!(
            response.contains("pulsedagd-rpc-worker"),
            "expected response from dedicated RPC worker thread, got {response:?}; last_error={last_error:?}"
        );
        server
            .shutdown_and_join()
            .expect("dedicated RPC server should stop cleanly");
    }
}
