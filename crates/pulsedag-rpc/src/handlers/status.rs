use crate::{
    api::fresh_or_cached_node_rpc_snapshot, api::p2p_status_for_rpc, api::read_chain_for_rpc,
    api::read_runtime_for_rpc, api::ApiResponse, api::NodeRpcSnapshot, api::RpcStateLike,
};
use axum::{extract::State, Json};
use pulsedag_core::state::ChainState;
use pulsedag_p2p::{connected_peers_semantics, mode_connected_peers_are_real_network};
use std::{
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct P2pPeerHealthSummary {
    pub healthy: usize,
    pub degraded: usize,
    pub cooldown: usize,
    pub recovering: usize,
    pub reconnect_attempts: u64,
    pub recovery_successes: u64,
    pub suppressed_dials: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeStatusData {
    pub rpc_response_degraded: bool,
    pub rpc_response_stale: bool,
    pub rpc_response_degraded_reason: Option<String>,
    pub network_id: String,
    pub peer_summary: String,
    pub service: String,
    pub version: String,
    pub chain_id: String,
    pub best_height: u64,
    pub uptime_secs: u64,
    pub block_count: usize,
    pub selected_tip: Option<String>,
    pub selected_height: Option<u64>,
    pub selected_blue_score: Option<u64>,
    pub ordered_dag_tip: Option<String>,
    pub ordering_version: String,
    pub ordered_dag_rebuild_total: u64,
    pub ordered_dag_rebuild_failed_total: u64,
    pub ordered_dag_state_root: Option<String>,
    pub consensus_mode: String,
    pub ghostdag_metadata_active: bool,
    pub high_cadence_allowed: bool,
    pub tip_count: usize,
    pub orphan_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub snapshot_exists: bool,
    pub snapshot_height: Option<u64>,
    pub captured_at_unix: Option<u64>,
    pub persisted_block_count: usize,
    pub recommended_keep_from_height: u64,
    pub p2p_enabled: bool,
    pub p2p_mode: Option<String>,
    pub p2p_runtime_mode_detail: Option<String>,
    pub connected_peers_are_real_network: bool,
    pub connected_peers_semantics: String,
    pub peer_count: usize,
    pub p2p_peer_health: Option<P2pPeerHealthSummary>,
    pub p2p_status_stale: bool,
    pub p2p_status_degraded: bool,
    pub p2p_status_degraded_reason: Option<String>,
    pub p2p_status_captured_at_unix: Option<u64>,
    pub sync_state: String,
    pub storage_backend: String,
    pub last_block_hash: Option<String>,
    pub contracts_prepared: bool,
    pub contracts_enabled: bool,
    pub contracts_vm_version: String,
    pub chain_state_generation: u64,
    pub accepted_commit_generation_conflict_total: u64,
    pub accepted_commit_reprepare_total: u64,
    pub accepted_commit_serialized_total: u64,
    pub accepted_commit_publish_mismatch_total: u64,
    pub accepted_commit_last_hash: Option<String>,
    pub accepted_commit_last_source: Option<String>,
    pub chain_state_mutation_generation: u64,
    pub chain_state_mutation_source: Option<String>,
    pub chain_state_mutation_conflict_total: u64,
    pub chain_state_reprepare_total: u64,
    pub accepted_hash_lost_from_memory_total: u64,
    pub accepted_hash_terminalization_prevented_total: u64,
    pub accepted_storage_repair_total: u64,
    pub last_lost_accepted_hash: Option<String>,
    pub last_lost_accepted_height: Option<u64>,
}

static STATUS_RESPONSE_CACHE: OnceLock<Mutex<Option<NodeStatusData>>> = OnceLock::new();

fn cached_status_response(reason: String) -> Option<NodeStatusData> {
    STATUS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|cache| cache.clone())
        .map(|mut data| {
            data.rpc_response_degraded = true;
            data.rpc_response_stale = true;
            data.rpc_response_degraded_reason = Some(reason);
            data
        })
}

fn cache_status_response(data: &NodeStatusData) {
    if let Ok(mut cache) = STATUS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *cache = Some(data.clone());
    }
}

fn repo_version() -> String {
    include_str!("../../../../VERSION").trim().to_string()
}

fn status_from_rpc_snapshot(snapshot: NodeRpcSnapshot) -> NodeStatusData {
    let peer_summary = format!(
        "peer_count={} semantics=cached_snapshot",
        snapshot.peer_count
    );
    NodeStatusData {
        rpc_response_degraded: true,
        rpc_response_stale: true,
        rpc_response_degraded_reason: snapshot.degraded_reason.clone(),
        network_id: snapshot.chain_id.clone(),
        peer_summary,
        service: "pulsedagd".into(),
        version: repo_version(),
        chain_id: snapshot.chain_id,
        best_height: snapshot.height,
        uptime_secs: 0,
        block_count: 0,
        selected_tip: snapshot.tip.clone(),
        selected_height: Some(snapshot.height),
        selected_blue_score: None,
        ordered_dag_tip: snapshot.tip.clone(),
        ordering_version: "legacy".to_string(),
        ordered_dag_rebuild_total: 0,
        ordered_dag_rebuild_failed_total: 0,
        ordered_dag_state_root: None,
        consensus_mode: pulsedag_core::ConsensusMode::Legacy.to_string(),
        ghostdag_metadata_active: false,
        high_cadence_allowed: false,
        tip_count: snapshot.tip.as_ref().map(|_| 1).unwrap_or(0),
        orphan_count: snapshot.orphan_count,
        mempool_size: 0,
        utxo_count: 0,
        address_count: 0,
        snapshot_exists: false,
        snapshot_height: Some(snapshot.height),
        captured_at_unix: Some(snapshot.last_updated_ms / 1_000),
        persisted_block_count: 0,
        recommended_keep_from_height: snapshot.height,
        p2p_enabled: snapshot.peer_count > 0,
        p2p_mode: None,
        p2p_runtime_mode_detail: Some("cached_node_rpc_snapshot".to_string()),
        connected_peers_are_real_network: false,
        connected_peers_semantics: "cached_snapshot".to_string(),
        peer_count: snapshot.peer_count,
        p2p_peer_health: None,
        p2p_status_stale: true,
        p2p_status_degraded: true,
        p2p_status_degraded_reason: snapshot.degraded_reason,
        p2p_status_captured_at_unix: Some(snapshot.last_updated_ms / 1_000),
        sync_state: snapshot.sync_state,
        storage_backend: "rocksdb".to_string(),
        last_block_hash: snapshot.tip,
        contracts_prepared: false,
        contracts_enabled: false,
        contracts_vm_version: "unknown".to_string(),
        chain_state_generation: 0,
        accepted_commit_generation_conflict_total: 0,
        accepted_commit_reprepare_total: 0,
        accepted_commit_serialized_total: 0,
        accepted_commit_publish_mismatch_total: 0,
        accepted_commit_last_hash: None,
        accepted_commit_last_source: None,
        chain_state_mutation_generation: 0,
        chain_state_mutation_source: None,
        chain_state_mutation_conflict_total: 0,
        chain_state_reprepare_total: 0,
        accepted_hash_lost_from_memory_total: 0,
        accepted_hash_terminalization_prevented_total: 0,
        accepted_storage_repair_total: 0,
        last_lost_accepted_hash: None,
        last_lost_accepted_height: None,
    }
}

struct StatusStateSnapshot {
    chain_id: String,
    best_height: u64,
    block_count: usize,
    selected_tip: Option<String>,
    selected_height: Option<u64>,
    selected_blue_score: Option<u64>,
    ordered_dag_tip: Option<String>,
    ordering_version: String,
    ordered_dag_rebuild_total: u64,
    ordered_dag_rebuild_failed_total: u64,
    ordered_dag_state_root: Option<String>,
    consensus_mode: String,
    ghostdag_metadata_active: bool,
    high_cadence_allowed: bool,
    tip_count: usize,
    orphan_count: usize,
    mempool_size: usize,
    utxo_count: usize,
    address_count: usize,
    last_block_hash: Option<String>,
    contracts_enabled: bool,
    contracts_vm_version: String,
    chain_state_generation: u64,
    accepted_commit_generation_conflict_total: u64,
    accepted_commit_reprepare_total: u64,
    accepted_commit_serialized_total: u64,
    accepted_commit_publish_mismatch_total: u64,
    accepted_commit_last_hash: Option<String>,
    accepted_commit_last_source: Option<String>,
    chain_state_mutation_generation: u64,
    chain_state_mutation_source: Option<String>,
    chain_state_mutation_conflict_total: u64,
    chain_state_reprepare_total: u64,
    accepted_hash_lost_from_memory_total: u64,
    accepted_hash_terminalization_prevented_total: u64,
    accepted_storage_repair_total: u64,
    last_lost_accepted_hash: Option<String>,
    last_lost_accepted_height: Option<u64>,
}

fn snapshot_chain(chain: &ChainState) -> StatusStateSnapshot {
    let last_block_hash = chain
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| b.hash.clone());
    let selected_tip = chain.dag.selected_chain.last().cloned();
    let selected_block = selected_tip
        .as_ref()
        .and_then(|hash| chain.dag.blocks.get(hash));

    StatusStateSnapshot {
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        selected_tip,
        selected_height: selected_block.map(|block| block.header.height),
        selected_blue_score: selected_block.map(|block| block.header.blue_score),
        ordered_dag_tip: pulsedag_core::ordered_dag_tip(chain),
        ordering_version: chain.dag.ordering_version.clone(),
        ordered_dag_rebuild_total: chain.dag.ordered_dag_rebuild_total,
        ordered_dag_rebuild_failed_total: chain.dag.ordered_dag_rebuild_failed_total,
        ordered_dag_state_root: chain.dag.ordered_dag_state_root.clone(),
        consensus_mode: chain.dag.consensus_mode.to_string(),
        ghostdag_metadata_active: chain.dag.consensus_mode.ghostdag_metadata_active(),
        high_cadence_allowed: chain.dag.consensus_mode.high_cadence_allowed(),
        tip_count: chain.dag.tips.len(),
        orphan_count: chain.orphan_blocks.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        address_count: chain.utxo.address_index.len(),
        last_block_hash,
        contracts_enabled: chain.contracts.config.enabled,
        contracts_vm_version: chain.contracts.config.vm_version.clone(),
        chain_state_generation: chain.chain_state_generation,
        accepted_commit_generation_conflict_total: chain.accepted_commit_generation_conflict_total,
        accepted_commit_reprepare_total: chain.accepted_commit_reprepare_total,
        accepted_commit_serialized_total: chain.accepted_commit_serialized_total,
        accepted_commit_publish_mismatch_total: chain.accepted_commit_publish_mismatch_total,
        accepted_commit_last_hash: chain.accepted_commit_last_hash.clone(),
        accepted_commit_last_source: chain.accepted_commit_last_source.clone(),
        chain_state_mutation_generation: chain.chain_state_mutation_generation,
        chain_state_mutation_source: chain.chain_state_mutation_source.clone(),
        chain_state_mutation_conflict_total: chain.chain_state_mutation_conflict_total,
        chain_state_reprepare_total: chain.chain_state_reprepare_total,
        accepted_hash_lost_from_memory_total: chain.accepted_hash_lost_from_memory_total,
        accepted_hash_terminalization_prevented_total: chain
            .accepted_hash_terminalization_prevented_total,
        accepted_storage_repair_total: chain.accepted_storage_repair_total,
        last_lost_accepted_hash: chain.last_lost_accepted_hash.clone(),
        last_lost_accepted_height: chain.last_lost_accepted_height,
    }
}

pub async fn get_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<NodeStatusData>> {
    let liveness_snapshot = fresh_or_cached_node_rpc_snapshot(&state, "/status").await;
    if liveness_snapshot.degraded || liveness_snapshot.stale {
        return Json(ApiResponse::ok(status_from_rpc_snapshot(liveness_snapshot)));
    }
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let contracts_prepared = state.storage().contract_namespaces_ready();
    let captured_at_unix = match state.storage().snapshot_captured_at_unix() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_block_count = match state.storage().block_count() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let mut p2p_status_stale = false;
    let mut p2p_status_degraded_reason = None;
    let mut p2p_status_captured_at_unix = None;
    let p2p_status = match p2p_status_for_rpc(state.p2p(), "/status").await {
        Ok(status) => status.map(|snapshot| {
            p2p_status_stale = snapshot.stale;
            p2p_status_degraded_reason = snapshot.degraded_reason.clone();
            p2p_status_captured_at_unix = snapshot.captured_at_unix;
            let s = snapshot.status;
            let peers_are_real = mode_connected_peers_are_real_network(&s.mode);
            let mode = s.mode.clone();
            let peer_health = P2pPeerHealthSummary {
                healthy: s.peer_lifecycle_healthy,
                degraded: s.peer_lifecycle_degraded,
                cooldown: s.peer_lifecycle_cooldown,
                recovering: s.peer_lifecycle_recovering,
                reconnect_attempts: s.peer_reconnect_attempts,
                recovery_successes: s.peer_recovery_success_count,
                suppressed_dials: s.peer_suppressed_dial_count,
            };
            (
                mode.clone(),
                s.runtime_mode_detail,
                peers_are_real,
                connected_peers_semantics(&mode).to_string(),
                s.connected_peers.len(),
                peer_health,
            )
        }),
        Err(e) => {
            p2p_status_stale = true;
            p2p_status_degraded_reason = Some(e);
            None
        }
    };
    let (
        p2p_mode,
        p2p_runtime_mode_detail,
        connected_peers_are_real_network,
        connected_peers_semantics,
        peer_count,
        p2p_peer_health,
    ) = p2p_status.unwrap_or((
        String::new(),
        String::new(),
        false,
        connected_peers_semantics("").to_string(),
        0,
        P2pPeerHealthSummary {
            healthy: 0,
            degraded: 0,
            cooldown: 0,
            recovering: 0,
            reconnect_attempts: 0,
            recovery_successes: 0,
            suppressed_dials: 0,
        },
    ));
    let p2p_status_degraded = p2p_status_stale || p2p_status_degraded_reason.is_some();
    let p2p_enabled = state.p2p().is_some();

    let chain_handle = state.chain();
    let chain_snapshot = {
        let chain = match read_chain_for_rpc(&chain_handle, "/status").await {
            Ok(chain) => chain,
            Err(e) => {
                if let Some(data) = cached_status_response(e.clone()) {
                    return Json(ApiResponse::ok(data));
                }
                return Json(ApiResponse::err("STATE_LOCK_BUSY", e));
            }
        };
        snapshot_chain(&chain)
    };
    let runtime_handle = state.runtime();
    let (keep_recent, uptime_secs, sync_state) = {
        let runtime = match read_runtime_for_rpc(&runtime_handle, "/status").await {
            Ok(runtime) => runtime,
            Err(e) => {
                if let Some(data) = cached_status_response(e.clone()) {
                    return Json(ApiResponse::ok(data));
                }
                return Json(ApiResponse::err("STATE_LOCK_BUSY", e));
            }
        };
        let keep_recent = runtime.prune_keep_recent_blocks.max(1);
        let uptime_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .saturating_sub(runtime.started_at_unix);
        (keep_recent, uptime_secs, runtime.sync_state.clone())
    };
    let recommended_keep_from_height = chain_snapshot
        .best_height
        .saturating_sub(keep_recent.saturating_sub(1));

    let peer_summary = format!(
        "peer_count={} semantics={}",
        peer_count, connected_peers_semantics
    );
    let data = NodeStatusData {
        rpc_response_degraded: false,
        rpc_response_stale: false,
        rpc_response_degraded_reason: None,
        network_id: chain_snapshot.chain_id.clone(),
        peer_summary,
        service: "pulsedagd".into(),
        version: repo_version(),
        chain_id: chain_snapshot.chain_id,
        best_height: chain_snapshot.best_height,
        uptime_secs,
        block_count: chain_snapshot.block_count,
        selected_tip: chain_snapshot.selected_tip,
        selected_height: chain_snapshot.selected_height,
        selected_blue_score: chain_snapshot.selected_blue_score,
        ordered_dag_tip: chain_snapshot.ordered_dag_tip,
        ordering_version: chain_snapshot.ordering_version,
        ordered_dag_rebuild_total: chain_snapshot.ordered_dag_rebuild_total,
        ordered_dag_rebuild_failed_total: chain_snapshot.ordered_dag_rebuild_failed_total,
        ordered_dag_state_root: chain_snapshot.ordered_dag_state_root,
        consensus_mode: chain_snapshot.consensus_mode,
        ghostdag_metadata_active: chain_snapshot.ghostdag_metadata_active,
        high_cadence_allowed: chain_snapshot.high_cadence_allowed,
        tip_count: chain_snapshot.tip_count,
        orphan_count: chain_snapshot.orphan_count,
        mempool_size: chain_snapshot.mempool_size,
        utxo_count: chain_snapshot.utxo_count,
        address_count: chain_snapshot.address_count,
        snapshot_exists,
        snapshot_height: if snapshot_exists {
            Some(chain_snapshot.best_height)
        } else {
            None
        },
        captured_at_unix,
        persisted_block_count,
        recommended_keep_from_height,
        p2p_enabled,
        p2p_mode: if p2p_enabled && !p2p_mode.is_empty() {
            Some(p2p_mode)
        } else {
            None
        },
        p2p_runtime_mode_detail: if p2p_enabled && !p2p_runtime_mode_detail.is_empty() {
            Some(p2p_runtime_mode_detail)
        } else {
            None
        },
        connected_peers_are_real_network,
        connected_peers_semantics,
        peer_count,
        p2p_peer_health: p2p_enabled.then_some(p2p_peer_health),
        p2p_status_stale,
        p2p_status_degraded,
        p2p_status_degraded_reason,
        p2p_status_captured_at_unix,
        sync_state,
        storage_backend: "rocksdb".to_string(),
        last_block_hash: chain_snapshot.last_block_hash,
        contracts_prepared,
        contracts_enabled: chain_snapshot.contracts_enabled,
        contracts_vm_version: chain_snapshot.contracts_vm_version,
        chain_state_generation: chain_snapshot.chain_state_generation,
        accepted_commit_generation_conflict_total: chain_snapshot
            .accepted_commit_generation_conflict_total,
        accepted_commit_reprepare_total: chain_snapshot.accepted_commit_reprepare_total,
        accepted_commit_serialized_total: chain_snapshot.accepted_commit_serialized_total,
        accepted_commit_publish_mismatch_total: chain_snapshot
            .accepted_commit_publish_mismatch_total,
        accepted_commit_last_hash: chain_snapshot.accepted_commit_last_hash,
        accepted_commit_last_source: chain_snapshot.accepted_commit_last_source,
        chain_state_mutation_generation: chain_snapshot.chain_state_mutation_generation,
        chain_state_mutation_source: chain_snapshot.chain_state_mutation_source,
        chain_state_mutation_conflict_total: chain_snapshot.chain_state_mutation_conflict_total,
        chain_state_reprepare_total: chain_snapshot.chain_state_reprepare_total,
        accepted_hash_lost_from_memory_total: chain_snapshot.accepted_hash_lost_from_memory_total,
        accepted_hash_terminalization_prevented_total: chain_snapshot
            .accepted_hash_terminalization_prevented_total,
        accepted_storage_repair_total: chain_snapshot.accepted_storage_repair_total,
        last_lost_accepted_hash: chain_snapshot.last_lost_accepted_hash,
        last_lost_accepted_height: chain_snapshot.last_lost_accepted_height,
    };
    cache_status_response(&data);
    Json(ApiResponse::ok(data))
}

#[cfg(test)]
mod tests {
    use super::get_status;
    use crate::{
        api::{
            build_node_rpc_snapshot, NodeRpcSnapshot, NodeRpcSnapshotStore, NodeRuntimeStats,
            RpcStateLike,
        },
        handlers::readiness::get_readiness,
    };
    use axum::{extract::State, Json};
    use pulsedag_core::ChainState;
    use pulsedag_p2p::{
        P2pHandle, P2pStatus, P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON, P2P_MODE_LIBP2P_REAL,
        P2P_MODE_MEMORY_SIMULATED,
    };
    use pulsedag_storage::Storage;
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
        p2p: Option<Arc<dyn P2pHandle>>,
        rpc_snapshot: NodeRpcSnapshotStore,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }
        fn p2p(&self) -> Option<Arc<dyn P2pHandle>> {
            self.p2p.clone()
        }
        fn storage(&self) -> Arc<Storage> {
            self.storage.clone()
        }
        fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>> {
            self.runtime.clone()
        }
        fn rpc_snapshot(&self) -> NodeRpcSnapshotStore {
            self.rpc_snapshot.clone()
        }
    }

    #[derive(Clone)]
    struct TestP2pHandle {
        status: P2pStatus,
    }

    impl P2pHandle for TestP2pHandle {
        fn broadcast_transaction(
            &self,
            _tx: &pulsedag_core::types::Transaction,
        ) -> Result<(), pulsedag_core::errors::PulseError> {
            Ok(())
        }
        fn broadcast_block(
            &self,
            _block: &pulsedag_core::types::Block,
        ) -> Result<(), pulsedag_core::errors::PulseError> {
            Ok(())
        }
        fn status(&self) -> Result<P2pStatus, pulsedag_core::errors::PulseError> {
            Ok(self.status.clone())
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    fn mk_state(status: P2pStatus) -> TestState {
        let path = temp_db_path("status");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let runtime = NodeRuntimeStats {
            sync_state: "idle".to_string(),
            ..NodeRuntimeStats::default()
        };
        let snapshot = build_node_rpc_snapshot(&chain, &runtime, Some(&status));
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: Some(Arc::new(TestP2pHandle { status })),
            rpc_snapshot: NodeRpcSnapshotStore::new(snapshot),
        }
    }

    fn base_status(mode: &str) -> P2pStatus {
        P2pStatus {
            chain_id: "testnet-dev".into(),
            mode: mode.to_string(),
            peer_id: "self".into(),
            listening: vec![],
            connected_peers: vec!["p1".into()],
            topics: vec![],
            mdns: false,
            kademlia: false,
            broadcasted_messages: 0,
            publish_attempts: 0,
            seen_message_ids: 0,
            queued_messages: 0,
            queued_block_messages: 0,
            queued_non_block_messages: 0,
            queue_max_depth: 0,
            dequeued_block_messages: 0,
            dequeued_non_block_messages: 0,
            queue_block_priority_picks: 0,
            queue_priority_tx_lane_picks: 0,
            queue_standard_tx_lane_picks: 0,
            queue_non_block_fair_picks: 0,
            queue_starvation_relief_picks: 0,
            queue_backpressure_drops: 0,
            inbound_messages: 0,
            runtime_started: true,
            runtime_mode_detail: "detail".into(),
            swarm_events_seen: 0,
            subscriptions_active: 0,
            last_message_kind: None,
            last_swarm_event: None,
            per_topic_publishes: HashMap::new(),
            inbound_decode_failed: 0,
            inbound_chain_mismatch_dropped: 0,
            inbound_duplicates_suppressed: 0,
            outbound_duplicates_suppressed: 0,
            inv_blocks_received: 0,
            inv_hashes_known: 0,
            inv_hashes_requested: 0,
            header_requests_received: 0,
            header_requests_sent: 0,
            headers_received: 0,
            headers_sent: 0,
            headers_announced: 0,
            dependency_fetches_scheduled: 0,
            parent_first_fetches: 0,
            relay_loop_prevented: 0,
            seen_cache_ttl_secs: 120,
            recovery_rebroadcast_ttl_secs: 8,
            max_inventory_length: 512,
            max_request_fanout: 64,
            tx_inbound_received: 0,
            tx_inbound_accepted: 0,
            tx_inbound_duplicate: 0,
            tx_inbound_invalid: 0,
            tx_relayed: 0,
            tx_relay_suppressed_budget: 0,
            tx_relay_suppressed_duplicate: 0,
            tx_outbound_duplicates_suppressed: 0,
            tx_outbound_first_seen_relayed: 0,
            tx_outbound_recovery_relayed: 0,
            tx_outbound_priority_relayed: 0,
            tx_outbound_budget_suppressed: 0,
            tx_outbound_recovery_budget_suppressed: 0,
            block_outbound_duplicates_suppressed: 0,
            block_outbound_first_seen_relayed: 0,
            block_outbound_recovery_relayed: 0,
            last_drop_reason: None,
            peer_reconnect_attempts: 0,
            peer_recovery_success_count: 0,
            last_peer_recovery_unix: None,
            peer_cooldown_suppressed_count: 0,
            peer_flap_suppressed_count: 0,
            peer_message_rate_limited_count: 0,
            peer_effective_count: 0,
            peer_min_target_missed_total: 0,
            peer_min_target_reconnect_attempt_total: 0,
            peer_min_target_reconnect_success_total: 0,
            peer_below_target_duration_seconds: 0,
            peer_below_target_blocked_reason: None,
            peer_recovery_state: "healthy".into(),
            peer_cooldown_bypassed_for_connectivity_total: 0,
            peer_rate_limit_recovery_suppressed_total: 0,
            peer_rate_limit_by_kind_total: Default::default(),
            peer_suppressed_dial_count: 0,
            peers_under_cooldown: 0,
            peers_under_flap_guard: 0,
            peer_lifecycle_healthy: 0,
            peer_lifecycle_watch: 0,
            peer_lifecycle_degraded: 0,
            peer_lifecycle_cooldown: 0,
            peer_lifecycle_recovering: 0,
            peer_retention_active_total: 0,
            peer_retention_recovering_total: 0,
            peer_retention_cooldown_total: 0,
            peer_sync_eligible_total: 0,
            peer_sync_suppressed_total: 0,
            degraded_mode: "unknown".into(),
            connection_shaping_active: false,
            peer_recovery: vec![],
            sync_candidates: vec![],
            selected_sync_peer: None,
            connection_slot_budget: 0,
            connected_slots_in_use: 0,
            available_connection_slots: 0,
            sync_selection_sticky_until_unix: None,
            topology_bucket_count: 8,
            topology_distinct_buckets: 0,
            topology_dominant_bucket_share_bps: 0,
            topology_diversity_score_bps: 0,
            blocks_requested: 0,
            blocks_received: 0,
            invalid_blocks_received: 0,
            orphan_blocks_received: 0,
            duplicate_blocks_received: 0,
            peer_penalties: 0,
            active_connections_by_peer: std::collections::HashMap::new(),
            active_connection_total: 0,
            last_connection_established_peer: None,
            last_connection_closed_peer: None,
            last_connection_closed_remaining_count: None,
            last_outgoing_connection_error_peer: None,
            last_incoming_connection_error_peer: None,
            last_dial_error: None,
            last_disconnect_reason: None,
            last_peer_state_transition: None,
            bootstrap_dial_attempts: 0,
            bootstrap_dial_successes: 0,
            bootstrap_dial_failures: 0,
            bootstrap_connected_peer_ids: vec![],
            bootnodes_configured: Vec::new(),
            bootnodes_connected: Vec::new(),
            pending_bootnode_dials: Vec::new(),
            bootnode_redial_attempts: 0,
            bootnode_redial_successes: 0,
            bootnode_redial_failures: 0,
            bootnode_reconnect_scheduled_total: 0,
            bootnode_reconnect_skipped_cooldown_total: 0,
            bootnode_reconnect_forced_from_cooldown_total: 0,
            bootnode_reconnect_success_total: 0,
            isolated_bootnode_reconnect_active: false,
            peer_zero_count_duration_seconds: 0,
            peer_zero_reconnect_attempt_total: 0,
            peer_zero_reconnect_success_total: 0,
            peer_reconnect_suppressed_by_cooldown_total: 0,
            peer_reconnect_suppressed_by_rate_limit_total: 0,
            peer_min_target_recovered_total: 0,
            last_peer_reconnect_blocked_reason: None,
            bootnode_next_redial_at: std::collections::HashMap::new(),
            bootnode_redial_backoff_secs: std::collections::HashMap::new(),
            last_bootnode_dial_error: None,
            gossipsub_peer_count: 0,
            subscribed_topics: Vec::new(),
            connection_established_total: 0,
            connection_closed_total: 0,
            last_connection_closed_reason: None,
            disconnect_reason_counts: std::collections::HashMap::new(),
            peer_lifecycle_event_counters: std::collections::HashMap::new(),
            last_error_by_peer: std::collections::HashMap::new(),
            inbound_peer_final_state: Vec::new(),
            outbound_peer_final_state: Vec::new(),
            asymmetric_connectivity_diagnostics: Vec::new(),
            inbound_connections_not_counted: 0,
            bootnode_peer_accounting_mismatch: false,
            bootnode_inbound_peers_counted: 0,
            bootnode_inbound_connections_seen_total: 0,
            bootnode_inbound_not_promoted_total: 0,
            private_topology_asymmetric_peer_count_total: 0,
            peer_reports_connected_to_bootnode_but_bootnode_reports_zero_total: 0,
            p2p_identity_path: None,
            p2p_identity_loaded_existing: false,
            p2p_identity_created_new: false,
            p2p_peer_id_changed_since_previous_start: false,
            configured_bootnode_peer_ids: Vec::new(),
            ..P2pStatus::default()
        }
    }

    #[tokio::test]
    async fn status_returns_while_chain_lock_is_held() {
        let state = mk_state(base_status(P2P_MODE_MEMORY_SIMULATED));
        let _write_guard = state.chain.write().await;

        let Json(resp) = get_status(State(state.clone())).await;
        let data = resp.data.expect("status data should exist");

        assert!(resp.ok);
        assert!(data.rpc_response_degraded);
        assert!(data.rpc_response_stale);
        assert_eq!(data.chain_id, "testnet-dev");
    }

    #[tokio::test]
    async fn readiness_returns_degraded_snapshot_when_fresh_state_is_unavailable() {
        let state = mk_state(base_status(P2P_MODE_MEMORY_SIMULATED));
        let _write_guard = state.chain.write().await;

        let Json(resp) = get_readiness(State(state.clone())).await;
        let data = resp.data.expect("readiness data should exist");

        assert!(resp.ok);
        assert_eq!(
            data.overall_status,
            crate::handlers::readiness::ReadinessStatus::Warn
        );
        assert!(!data.node_ready);
        assert!(data
            .warnings
            .iter()
            .any(|warning| warning.contains("rpc_degraded_response")));
    }

    #[tokio::test]
    async fn stale_snapshot_is_marked_stale() {
        let state = mk_state(base_status(P2P_MODE_MEMORY_SIMULATED));
        state.rpc_snapshot.store(NodeRpcSnapshot {
            chain_id: "stale-chain".to_string(),
            height: 7,
            tip: Some("stale-tip".to_string()),
            peer_count: 2,
            orphan_count: 3,
            pending_missing_parents: 4,
            missing_parent_entries: Vec::new(),
            terminal_missing_parent_entries: Vec::new(),
            quarantined_missing_parent_entries: Vec::new(),
            inv_hashes_requested: 5,
            last_updated_ms: 1,
            degraded: true,
            degraded_reason: Some("test stale snapshot".to_string()),
            stale: true,
            pending_block_requests: 6,
            inflight_block_requests: 0,
            sync_state: "degraded".to_string(),
            storage_mode: "rocksdb".to_string(),
            startup_mode: "unknown".to_string(),
            last_consistency_audit_ok: true,
            last_consistency_audit_issue_count: 0,
            last_consistency_audit_unix: None,
            active_alert_count: 0,
        });
        let _write_guard = state.chain.write().await;

        let Json(resp) = get_status(State(state.clone())).await;
        let data = resp.data.expect("status data should exist");

        assert!(data.rpc_response_stale);
        assert!(data.rpc_response_degraded);
        assert_eq!(data.best_height, 7);
        assert_eq!(data.rpc_response_degraded_reason.as_deref(), Some("/status avoided waiting for fresh liveness state: node RPC snapshot capture skipped because chain read lock is busy"));
    }

    #[tokio::test]
    async fn status_labels_peer_semantics_for_memory_and_skeleton_and_real_modes() {
        for (mode, expect_real, expect_semantics) in [
            (
                P2P_MODE_MEMORY_SIMULATED,
                false,
                "simulated-or-internal-peer-observations",
            ),
            (
                P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON,
                false,
                "simulated-or-internal-peer-observations",
            ),
            (P2P_MODE_LIBP2P_REAL, true, "real-network-connected-peers"),
        ] {
            let Json(resp) = get_status(State(mk_state(base_status(mode)))).await;
            let data = resp.data.expect("status data should exist");
            assert_eq!(data.p2p_mode.as_deref(), Some(mode));
            assert_eq!(data.connected_peers_are_real_network, expect_real);
            assert_eq!(data.connected_peers_semantics, expect_semantics);
        }
    }
}
