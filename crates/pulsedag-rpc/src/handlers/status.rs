use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_p2p::{connected_peers_semantics, mode_connected_peers_are_real_network};

#[derive(Debug, serde::Serialize)]
pub struct NodeStatusData {
    pub service: String,
    pub version: String,
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
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
    pub last_block_hash: Option<String>,
    pub contracts_prepared: bool,
    pub contracts_enabled: bool,
    pub contracts_vm_version: String,
}

fn repo_version() -> String {
    include_str!("../../../../VERSION").trim().to_string()
}

pub async fn get_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<NodeStatusData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let contracts_prepared = state.storage().contract_namespaces_ready();
    let captured_at_unix = match state.storage().snapshot_captured_at_unix() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let keep_recent = runtime.prune_keep_recent_blocks.max(1);
    let recommended_keep_from_height = chain
        .dag
        .best_height
        .saturating_sub(keep_recent.saturating_sub(1));
    let p2p_status = state.p2p().and_then(|p| p.status().ok()).map(|s| {
        let peers_are_real = mode_connected_peers_are_real_network(&s.mode);
        let mode = s.mode.clone();
        (
            mode.clone(),
            s.runtime_mode_detail,
            peers_are_real,
            connected_peers_semantics(&mode).to_string(),
            s.connected_peers.len(),
        )
    });
    let (
        p2p_mode,
        p2p_runtime_mode_detail,
        connected_peers_are_real_network,
        connected_peers_semantics,
        peer_count,
    ) = p2p_status.unwrap_or((
        String::new(),
        String::new(),
        false,
        connected_peers_semantics("").to_string(),
        0,
    ));
    let p2p_enabled = state.p2p().is_some();
    let last_block_hash = chain
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| b.hash.clone());

    Json(ApiResponse::ok(NodeStatusData {
        service: "pulsedagd".into(),
        version: repo_version(),
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        address_count: chain.utxo.address_index.len(),
        snapshot_exists,
        snapshot_height: if snapshot_exists {
            Some(chain.dag.best_height)
        } else {
            None
        },
        captured_at_unix,
        persisted_block_count: persisted_blocks.len(),
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
        last_block_hash,
        contracts_prepared,
        contracts_enabled: chain.contracts.config.enabled,
        contracts_vm_version: chain.contracts.config.vm_version.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::get_status;
    use crate::api::{NodeRuntimeStats, RpcStateLike};
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
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: Some(Arc::new(TestP2pHandle { status })),
        }
    }

    fn base_status(mode: &str) -> P2pStatus {
        P2pStatus {
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
            queue_non_block_fair_picks: 0,
            queue_starvation_relief_picks: 0,
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
            tx_outbound_duplicates_suppressed: 0,
            tx_outbound_first_seen_relayed: 0,
            tx_outbound_recovery_relayed: 0,
            tx_outbound_priority_relayed: 0,
            tx_outbound_budget_suppressed: 0,
            block_outbound_duplicates_suppressed: 0,
            block_outbound_first_seen_relayed: 0,
            block_outbound_recovery_relayed: 0,
            last_drop_reason: None,
            peer_reconnect_attempts: 0,
            peer_recovery_success_count: 0,
            last_peer_recovery_unix: None,
            peer_cooldown_suppressed_count: 0,
            peer_flap_suppressed_count: 0,
            peers_under_cooldown: 0,
            peers_under_flap_guard: 0,
            peer_lifecycle_healthy: 0,
            peer_lifecycle_watch: 0,
            peer_lifecycle_degraded: 0,
            peer_lifecycle_cooldown: 0,
            peer_lifecycle_recovering: 0,
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
        }
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
