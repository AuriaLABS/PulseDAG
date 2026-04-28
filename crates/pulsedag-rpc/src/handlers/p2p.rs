use axum::{extract::State, Json};
use pulsedag_p2p::{
    connected_peers_semantics, mode_connected_peers_are_real_network, PeerRecoveryStatus,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::{ApiResponse, RpcStateLike};

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

fn peer_health_counts(
    peer_recovery: &[PeerRecoveryStatus],
    now_unix: u64,
) -> (usize, usize, usize) {
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
    (healthy, degraded, recovering)
}

pub async fn get_p2p_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => {
                let now_unix = unix_now_secs();
                let (healthy_count, degraded_count, recovering_count) =
                    peer_health_counts(&status.peer_recovery, now_unix);
                let peers_with_recent_failures = status
                    .peer_recovery
                    .iter()
                    .filter(|peer| !peer.recent_failures_unix.is_empty())
                    .count();
                let sync_candidates = status
                    .sync_candidates
                    .iter()
                    .map(|candidate| {
                        serde_json::json!({
                            "peer_id": candidate.peer_id,
                            "rank_score": candidate.rank_score,
                            "excluded_until_unix": candidate.excluded_until_unix
                        })
                    })
                    .collect::<Vec<_>>();
                let peer_recovery = status
                    .peer_recovery
                    .iter()
                    .map(|peer| {
                        serde_json::json!({
                            "peer_id": peer.peer_id,
                            "score": peer.score,
                            "fail_streak": peer.fail_streak,
                            "lifecycle_tier": peer.lifecycle_tier,
                            "recovery_tier": peer.recovery_tier,
                            "connected": peer.connected,
                            "last_seen_unix": peer.last_seen_unix,
                            "last_successful_connect_unix": peer.last_successful_connect_unix,
                            "next_retry_unix": peer.next_retry_unix,
                            "reconnect_attempts": peer.reconnect_attempts,
                            "recovery_success_count": peer.recovery_success_count,
                            "last_recovery_unix": peer.last_recovery_unix,
                            "recent_failures_unix": peer.recent_failures_unix,
                            "cooldown_suppressed_count": peer.cooldown_suppressed_count,
                            "flap_suppressed_count": peer.flap_suppressed_count,
                            "flap_events": peer.flap_events,
                            "suppression_until_unix": peer.suppression_until_unix
                        })
                    })
                    .collect::<Vec<_>>();
                let mut payload = serde_json::Map::new();
                payload.insert("mode".into(), serde_json::json!(status.mode));
                payload.insert(
                    "connected_peers_are_real_network".into(),
                    serde_json::json!(mode_connected_peers_are_real_network(&status.mode)),
                );
                payload.insert(
                    "connected_peers_semantics".into(),
                    serde_json::json!(connected_peers_semantics(&status.mode)),
                );
                payload.insert("peer_id".into(), serde_json::json!(status.peer_id));
                payload.insert("listening".into(), serde_json::json!(status.listening));
                payload.insert(
                    "connected_peers".into(),
                    serde_json::json!(status.connected_peers),
                );
                payload.insert("topics".into(), serde_json::json!(status.topics));
                payload.insert("mdns".into(), serde_json::json!(status.mdns));
                payload.insert("kademlia".into(), serde_json::json!(status.kademlia));
                payload.insert(
                    "broadcasted_messages".into(),
                    serde_json::json!(status.broadcasted_messages),
                );
                payload.insert(
                    "publish_attempts".into(),
                    serde_json::json!(status.publish_attempts),
                );
                payload.insert(
                    "seen_message_ids".into(),
                    serde_json::json!(status.seen_message_ids),
                );
                payload.insert(
                    "queued_messages".into(),
                    serde_json::json!(status.queued_messages),
                );
                payload.insert(
                    "inbound_messages".into(),
                    serde_json::json!(status.inbound_messages),
                );
                payload.insert(
                    "runtime_started".into(),
                    serde_json::json!(status.runtime_started),
                );
                payload.insert(
                    "runtime_mode_detail".into(),
                    serde_json::json!(status.runtime_mode_detail),
                );
                payload.insert(
                    "swarm_events_seen".into(),
                    serde_json::json!(status.swarm_events_seen),
                );
                payload.insert(
                    "subscriptions_active".into(),
                    serde_json::json!(status.subscriptions_active),
                );
                payload.insert(
                    "last_message_kind".into(),
                    serde_json::json!(status.last_message_kind),
                );
                payload.insert(
                    "last_swarm_event".into(),
                    serde_json::json!(status.last_swarm_event),
                );
                payload.insert(
                    "per_topic_publishes".into(),
                    serde_json::json!(status.per_topic_publishes),
                );
                payload.insert(
                    "inbound_decode_failed".into(),
                    serde_json::json!(status.inbound_decode_failed),
                );
                payload.insert(
                    "inbound_chain_mismatch_dropped".into(),
                    serde_json::json!(status.inbound_chain_mismatch_dropped),
                );
                payload.insert(
                    "inbound_duplicates_suppressed".into(),
                    serde_json::json!(status.inbound_duplicates_suppressed),
                );
                payload.insert(
                    "last_drop_reason".into(),
                    serde_json::json!(status.last_drop_reason),
                );
                payload.insert(
                    "peer_reconnect_attempts".into(),
                    serde_json::json!(status.peer_reconnect_attempts),
                );
                payload.insert(
                    "peer_recovery_success_count".into(),
                    serde_json::json!(status.peer_recovery_success_count),
                );
                payload.insert(
                    "last_peer_recovery_unix".into(),
                    serde_json::json!(status.last_peer_recovery_unix),
                );
                payload.insert(
                    "peer_cooldown_suppressed_count".into(),
                    serde_json::json!(status.peer_cooldown_suppressed_count),
                );
                payload.insert(
                    "peer_flap_suppressed_count".into(),
                    serde_json::json!(status.peer_flap_suppressed_count),
                );
                payload.insert(
                    "peers_under_cooldown".into(),
                    serde_json::json!(status.peers_under_cooldown),
                );
                payload.insert(
                    "peers_under_flap_guard".into(),
                    serde_json::json!(status.peers_under_flap_guard),
                );
                payload.insert(
                    "degraded_mode".into(),
                    serde_json::json!(status.degraded_mode),
                );
                payload.insert(
                    "connection_shaping_active".into(),
                    serde_json::json!(status.connection_shaping_active),
                );
                payload.insert(
                    "peer_state_summary".into(),
                    serde_json::json!({
                        "total": status.peer_recovery.len(),
                        "healthy": status.peer_lifecycle_healthy,
                        "watch": status.peer_lifecycle_watch,
                        "degraded": status.peer_lifecycle_degraded,
                        "cooldown": status.peer_lifecycle_cooldown,
                        "recovering": status.peer_lifecycle_recovering,
                        "derived_healthy_legacy": healthy_count,
                        "derived_degraded_legacy": degraded_count,
                        "derived_recovering_legacy": recovering_count,
                        "peers_with_recent_failures": peers_with_recent_failures
                    }),
                );
                payload.insert(
                    "recovery_activity_summary".into(),
                    serde_json::json!({
                        "reconnect_attempts": status.peer_reconnect_attempts,
                        "recovery_success_count": status.peer_recovery_success_count,
                        "last_recovery_unix": status.last_peer_recovery_unix,
                        "cooldown_suppressed_count": status.peer_cooldown_suppressed_count,
                        "flap_suppressed_count": status.peer_flap_suppressed_count,
                        "peers_under_cooldown": status.peers_under_cooldown,
                        "peers_under_flap_guard": status.peers_under_flap_guard
                    }),
                );
                payload.insert(
                    "selected_sync_peer".into(),
                    serde_json::json!(status.selected_sync_peer),
                );
                payload.insert(
                    "connection_slot_budget".into(),
                    serde_json::json!(status.connection_slot_budget),
                );
                payload.insert(
                    "connected_slots_in_use".into(),
                    serde_json::json!(status.connected_slots_in_use),
                );
                payload.insert(
                    "available_connection_slots".into(),
                    serde_json::json!(status.available_connection_slots),
                );
                payload.insert(
                    "sync_selection_sticky_until_unix".into(),
                    serde_json::json!(status.sync_selection_sticky_until_unix),
                );
                payload.insert(
                    "topology_bucket_count".into(),
                    serde_json::json!(status.topology_bucket_count),
                );
                payload.insert(
                    "topology_distinct_buckets".into(),
                    serde_json::json!(status.topology_distinct_buckets),
                );
                payload.insert(
                    "topology_dominant_bucket_share_bps".into(),
                    serde_json::json!(status.topology_dominant_bucket_share_bps),
                );
                payload.insert(
                    "topology_diversity_score_bps".into(),
                    serde_json::json!(status.topology_diversity_score_bps),
                );
                payload.insert("sync_candidates".into(), serde_json::json!(sync_candidates));
                payload.insert("peer_recovery".into(), serde_json::json!(peer_recovery));
                Json(ApiResponse::ok(serde_json::Value::Object(payload)))
            }
            Err(e) => Json(ApiResponse::err("P2P_ERROR", e.to_string())),
        },
        None => Json(ApiResponse::err("P2P_DISABLED", "p2p is disabled")),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct P2pPeerItem {
    pub peer_id: String,
    pub connected: bool,
    pub source_mode: String,
}

#[derive(Debug, serde::Serialize)]
pub struct P2pPeersData {
    pub count: usize,
    pub peers: Vec<P2pPeerItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct P2pTopicsData {
    pub count: usize,
    pub topics: Vec<String>,
    pub per_topic_publishes: std::collections::HashMap<String, usize>,
}

pub async fn get_p2p_peers<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<P2pPeersData>> {
    match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => {
                let connected_peers_are_real_network =
                    mode_connected_peers_are_real_network(&status.mode);
                let peers = status
                    .connected_peers
                    .into_iter()
                    .map(|peer_id| P2pPeerItem {
                        peer_id,
                        connected: connected_peers_are_real_network,
                        source_mode: status.mode.clone(),
                    })
                    .collect::<Vec<_>>();
                Json(ApiResponse::ok(P2pPeersData {
                    count: peers.len(),
                    peers,
                }))
            }
            Err(e) => Json(ApiResponse::err("P2P_ERROR", e.to_string())),
        },
        None => Json(ApiResponse::err("P2P_DISABLED", "p2p is disabled")),
    }
}

pub async fn get_p2p_topics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<P2pTopicsData>> {
    match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => Json(ApiResponse::ok(P2pTopicsData {
                count: status.topics.len(),
                topics: status.topics,
                per_topic_publishes: status.per_topic_publishes,
            })),
            Err(e) => Json(ApiResponse::err("P2P_ERROR", e.to_string())),
        },
        None => Json(ApiResponse::err("P2P_DISABLED", "p2p is disabled")),
    }
}

#[cfg(test)]
mod tests {
    use super::get_p2p_status;
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::{extract::State, Json};
    use pulsedag_core::ChainState;
    use pulsedag_p2p::{P2pHandle, P2pStatus, PeerRecoveryStatus, P2P_MODE_MEMORY_SIMULATED};
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
        let path = temp_db_path("p2p-status");
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

    #[tokio::test]
    async fn p2p_status_includes_existing_and_new_operator_summary_fields() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let status = P2pStatus {
            mode: P2P_MODE_MEMORY_SIMULATED.to_string(),
            peer_id: "self".into(),
            listening: vec!["memory://local".into()],
            connected_peers: vec!["peer-a".into()],
            topics: vec!["blocks".into()],
            mdns: false,
            kademlia: false,
            broadcasted_messages: 4,
            publish_attempts: 5,
            seen_message_ids: 6,
            queued_messages: 7,
            queued_block_messages: 3,
            queued_non_block_messages: 4,
            queue_max_depth: 9,
            dequeued_block_messages: 2,
            dequeued_non_block_messages: 5,
            queue_block_priority_picks: 2,
            queue_priority_tx_lane_picks: 0,
            queue_standard_tx_lane_picks: 0,
            queue_non_block_fair_picks: 3,
            queue_starvation_relief_picks: 1,
            inbound_messages: 8,
            runtime_started: true,
            runtime_mode_detail: "in-process-dispatch".into(),
            swarm_events_seen: 9,
            subscriptions_active: 1,
            last_message_kind: Some("block".into()),
            last_swarm_event: Some("connection-established".into()),
            per_topic_publishes: HashMap::from([("blocks".into(), 4usize)]),
            inbound_decode_failed: 0,
            inbound_chain_mismatch_dropped: 0,
            inbound_duplicates_suppressed: 0,
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
            peer_reconnect_attempts: 12,
            peer_recovery_success_count: 3,
            last_peer_recovery_unix: Some(now.saturating_sub(10)),
            peer_cooldown_suppressed_count: 2,
            peer_flap_suppressed_count: 1,
            peers_under_cooldown: 1,
            peers_under_flap_guard: 1,
            peer_lifecycle_healthy: 1,
            peer_lifecycle_watch: 0,
            peer_lifecycle_degraded: 0,
            peer_lifecycle_cooldown: 0,
            peer_lifecycle_recovering: 1,
            degraded_mode: "normal".into(),
            connection_shaping_active: false,
            peer_recovery: vec![
                PeerRecoveryStatus {
                    peer_id: "healthy".into(),
                    score: 100,
                    fail_streak: 0,
                    lifecycle_tier: "healthy".into(),
                    recovery_tier: "steady".into(),
                    connected: true,
                    last_seen_unix: Some(now),
                    last_successful_connect_unix: Some(now),
                    next_retry_unix: 0,
                    reconnect_attempts: 1,
                    recovery_success_count: 1,
                    last_recovery_unix: Some(now),
                    recent_failures_unix: vec![],
                    cooldown_suppressed_count: 0,
                    flap_suppressed_count: 0,
                    flap_events: 0,
                    suppression_until_unix: None,
                },
                PeerRecoveryStatus {
                    peer_id: "recovering".into(),
                    score: 65,
                    fail_streak: 1,
                    lifecycle_tier: "recovering".into(),
                    recovery_tier: "assisted".into(),
                    connected: false,
                    last_seen_unix: Some(now.saturating_sub(60)),
                    last_successful_connect_unix: Some(now.saturating_sub(120)),
                    next_retry_unix: now.saturating_add(20),
                    reconnect_attempts: 6,
                    recovery_success_count: 1,
                    last_recovery_unix: Some(now.saturating_sub(70)),
                    recent_failures_unix: vec![now.saturating_sub(30)],
                    cooldown_suppressed_count: 1,
                    flap_suppressed_count: 1,
                    flap_events: 2,
                    suppression_until_unix: Some(now.saturating_add(10)),
                },
            ],
            sync_candidates: vec![],
            selected_sync_peer: Some("peer-a".into()),
            connection_slot_budget: 8,
            connected_slots_in_use: 2,
            available_connection_slots: 6,
            sync_selection_sticky_until_unix: Some(now.saturating_add(30)),
            topology_bucket_count: 8,
            topology_distinct_buckets: 1,
            topology_dominant_bucket_share_bps: 10_000,
            topology_diversity_score_bps: 625,
        };

        let Json(resp) = get_p2p_status(State(mk_state(status))).await;
        let data = resp.data.expect("p2p status data");
        assert!(data.get("connected_peers").is_some());
        assert_eq!(
            data["connected_peers_semantics"],
            "simulated-or-internal-peer-observations"
        );
        assert!(data.get("peer_recovery").is_some());
        assert_eq!(data["peer_state_summary"]["total"], 2);
        assert_eq!(data["peer_state_summary"]["healthy"], 1);
        assert_eq!(data["peer_state_summary"]["recovering"], 1);
        assert_eq!(data["recovery_activity_summary"]["reconnect_attempts"], 12);
        assert!(data["recovery_activity_summary"]["last_recovery_unix"].is_number());
    }
}
