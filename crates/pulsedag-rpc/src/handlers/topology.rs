use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_p2p::{
    connected_peers_semantics, mode_connected_peers_are_real_network, PeerRecoveryStatus,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, serde::Serialize)]
pub struct TopologyPeerHealthCounts {
    pub total: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub recovering: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct TopologyRecoverySummary {
    pub reconnect_attempts: u64,
    pub recovery_success_count: u64,
    pub last_recovery_unix: Option<u64>,
    pub peers_with_recent_failures: usize,
    pub peers_under_cooldown: usize,
    pub peers_under_flap_guard: usize,
    pub cooldown_suppressed_count: u64,
    pub flap_suppressed_count: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct TopologyData {
    pub p2p_enabled: bool,
    pub mode: Option<String>,
    pub runtime_mode_detail: Option<String>,
    pub connected_peers_are_real_network: bool,
    pub connected_peers_semantics: String,
    pub peer_count: usize,
    pub topic_count: usize,
    pub peers: Vec<String>,
    pub topics: Vec<String>,
    pub peer_health_counts: TopologyPeerHealthCounts,
    pub recovery_summary: TopologyRecoverySummary,
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_peer_recovering(peer: &PeerRecoveryStatus, now_unix: u64) -> bool {
    if !peer.connected || peer.fail_streak > 0 {
        return true;
    }
    if peer
        .suppression_until_unix
        .is_some_and(|until| until > now_unix)
    {
        return true;
    }
    peer.next_retry_unix > now_unix
}

fn is_peer_degraded(peer: &PeerRecoveryStatus) -> bool {
    peer.score < 80 || peer.flap_events > 0 || !peer.recent_failures_unix.is_empty()
}

fn classify_peer_health(
    peer_recovery: &[PeerRecoveryStatus],
    now_unix: u64,
) -> TopologyPeerHealthCounts {
    let mut healthy = 0usize;
    let mut degraded = 0usize;
    let mut recovering = 0usize;
    for peer in peer_recovery {
        if is_peer_recovering(peer, now_unix) {
            recovering = recovering.saturating_add(1);
        } else if is_peer_degraded(peer) {
            degraded = degraded.saturating_add(1);
        } else {
            healthy = healthy.saturating_add(1);
        }
    }
    TopologyPeerHealthCounts {
        total: peer_recovery.len(),
        healthy,
        degraded,
        recovering,
    }
}

pub async fn get_topology<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<TopologyData>> {
    match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => {
                let now_unix = unix_now_secs();
                let peer_health_counts = classify_peer_health(&status.peer_recovery, now_unix);
                let peers_with_recent_failures = status
                    .peer_recovery
                    .iter()
                    .filter(|peer| !peer.recent_failures_unix.is_empty())
                    .count();
                Json(ApiResponse::ok(TopologyData {
                    p2p_enabled: true,
                    connected_peers_are_real_network: mode_connected_peers_are_real_network(
                        &status.mode,
                    ),
                    connected_peers_semantics: connected_peers_semantics(&status.mode).to_string(),
                    mode: Some(status.mode),
                    runtime_mode_detail: Some(status.runtime_mode_detail),
                    peer_count: status.connected_peers.len(),
                    topic_count: status.topics.len(),
                    peers: status.connected_peers,
                    topics: status.topics,
                    peer_health_counts,
                    recovery_summary: TopologyRecoverySummary {
                        reconnect_attempts: status.peer_reconnect_attempts,
                        recovery_success_count: status.peer_recovery_success_count,
                        last_recovery_unix: status.last_peer_recovery_unix,
                        peers_with_recent_failures,
                        peers_under_cooldown: status.peers_under_cooldown,
                        peers_under_flap_guard: status.peers_under_flap_guard,
                        cooldown_suppressed_count: status.peer_cooldown_suppressed_count,
                        flap_suppressed_count: status.peer_flap_suppressed_count,
                    },
                }))
            }
            Err(e) => Json(ApiResponse::err("P2P_ERROR", e.to_string())),
        },
        None => Json(ApiResponse::ok(TopologyData {
            p2p_enabled: false,
            mode: None,
            runtime_mode_detail: None,
            connected_peers_are_real_network: false,
            connected_peers_semantics: connected_peers_semantics("").to_string(),
            peer_count: 0,
            topic_count: 0,
            peers: Vec::new(),
            topics: Vec::new(),
            peer_health_counts: TopologyPeerHealthCounts {
                total: 0,
                healthy: 0,
                degraded: 0,
                recovering: 0,
            },
            recovery_summary: TopologyRecoverySummary {
                reconnect_attempts: 0,
                recovery_success_count: 0,
                last_recovery_unix: None,
                peers_with_recent_failures: 0,
                peers_under_cooldown: 0,
                peers_under_flap_guard: 0,
                cooldown_suppressed_count: 0,
                flap_suppressed_count: 0,
            },
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::get_topology;
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::{extract::State, Json};
    use pulsedag_core::ChainState;
    use pulsedag_p2p::{
        P2pHandle, P2pStatus, PeerRecoveryStatus, P2P_MODE_LIBP2P_REAL, P2P_MODE_MEMORY_SIMULATED,
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
        let path = temp_db_path("topology");
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
            connected_peers: vec!["p1".into(), "p2".into()],
            topics: vec!["blocks".into()],
            mdns: false,
            kademlia: true,
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
            subscriptions_active: 1,
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
            peer_reconnect_attempts: 7,
            peer_recovery_success_count: 2,
            last_peer_recovery_unix: Some(1_700_000_000),
            peer_cooldown_suppressed_count: 3,
            peer_flap_suppressed_count: 1,
            peers_under_cooldown: 1,
            peers_under_flap_guard: 1,
            peer_recovery: vec![],
            sync_candidates: vec![],
            selected_sync_peer: None,
            connection_slot_budget: 0,
            connected_slots_in_use: 0,
            available_connection_slots: 0,
            sync_selection_sticky_until_unix: None,
        }
    }

    fn peer(
        peer_id: &str,
        connected: bool,
        fail_streak: u32,
        score: i32,
        flap_events: u32,
        next_retry_unix: u64,
        suppression_until_unix: Option<u64>,
        recent_failures_unix: Vec<u64>,
    ) -> PeerRecoveryStatus {
        PeerRecoveryStatus {
            peer_id: peer_id.to_string(),
            score,
            fail_streak,
            connected,
            last_seen_unix: None,
            last_successful_connect_unix: None,
            next_retry_unix,
            reconnect_attempts: 0,
            recovery_success_count: 0,
            last_recovery_unix: None,
            recent_failures_unix,
            cooldown_suppressed_count: 0,
            flap_suppressed_count: 0,
            flap_events,
            suppression_until_unix,
        }
    }

    #[tokio::test]
    async fn topology_output_reflects_healthy_degraded_recovering_counts_correctly() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut status = base_status(P2P_MODE_LIBP2P_REAL);
        status.peer_recovery = vec![
            peer("healthy", true, 0, 100, 0, 0, None, vec![]),
            peer(
                "degraded",
                true,
                0,
                70,
                1,
                0,
                None,
                vec![now.saturating_sub(30)],
            ),
            peer(
                "recovering",
                false,
                2,
                40,
                2,
                now.saturating_add(30),
                Some(now.saturating_add(20)),
                vec![now.saturating_sub(10)],
            ),
        ];
        let Json(resp) = get_topology(State(mk_state(status))).await;
        let data = resp.data.expect("topology data");
        assert_eq!(data.peer_health_counts.total, 3);
        assert_eq!(data.peer_health_counts.healthy, 1);
        assert_eq!(data.peer_health_counts.degraded, 1);
        assert_eq!(data.peer_health_counts.recovering, 1);
    }

    #[tokio::test]
    async fn topology_mode_distinctions_remain_truthful() {
        let mut real_status = base_status(P2P_MODE_LIBP2P_REAL);
        real_status.peer_recovery = vec![peer("r", true, 0, 100, 0, 0, None, vec![])];
        let Json(real_resp) = get_topology(State(mk_state(real_status))).await;
        let real_data = real_resp.data.unwrap();
        assert!(real_data.connected_peers_are_real_network);
        assert_eq!(
            real_data.connected_peers_semantics,
            "real-network-connected-peers"
        );

        let mut dev_status = base_status(P2P_MODE_MEMORY_SIMULATED);
        dev_status.peer_recovery = vec![peer("d", true, 0, 100, 0, 0, None, vec![])];
        let Json(dev_resp) = get_topology(State(mk_state(dev_status))).await;
        let dev_data = dev_resp.data.unwrap();
        assert!(!dev_data.connected_peers_are_real_network);
        assert_eq!(
            dev_data.connected_peers_semantics,
            "simulated-or-internal-peer-observations"
        );
    }

    #[tokio::test]
    async fn topology_cooldown_and_recovery_summary_is_coherent_under_peer_churn() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut status = base_status(P2P_MODE_MEMORY_SIMULATED);
        status.peer_recovery = vec![
            peer(
                "cooldown",
                true,
                1,
                55,
                2,
                now.saturating_add(60),
                Some(now.saturating_add(60)),
                vec![now.saturating_sub(3)],
            ),
            peer("flap", true, 0, 75, 2, 0, None, vec![now.saturating_sub(2)]),
            peer("ok", true, 0, 99, 0, 0, None, vec![]),
        ];
        let Json(resp) = get_topology(State(mk_state(status))).await;
        let data = resp.data.unwrap();
        assert_eq!(data.recovery_summary.peers_under_cooldown, 1);
        assert_eq!(data.recovery_summary.peers_under_flap_guard, 1);
        assert_eq!(data.recovery_summary.cooldown_suppressed_count, 3);
        assert_eq!(data.recovery_summary.flap_suppressed_count, 1);
        assert_eq!(data.recovery_summary.reconnect_attempts, 7);
        assert_eq!(data.recovery_summary.recovery_success_count, 2);
        assert_eq!(data.recovery_summary.peers_with_recent_failures, 2);
        assert_eq!(
            data.peer_health_counts.healthy
                + data.peer_health_counts.degraded
                + data.peer_health_counts.recovering,
            data.peer_health_counts.total
        );
    }
}
