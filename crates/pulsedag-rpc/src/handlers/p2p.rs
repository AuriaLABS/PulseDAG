use axum::{extract::State, Json};
use pulsedag_p2p::mode_connected_peers_are_real_network;

use crate::api::{ApiResponse, RpcStateLike};

pub async fn get_p2p_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => Json(ApiResponse::ok(serde_json::json!({
                "mode": status.mode,
                "connected_peers_are_real_network": mode_connected_peers_are_real_network(&status.mode),
                "peer_id": status.peer_id,
                "listening": status.listening,
                "connected_peers": status.connected_peers,
                "topics": status.topics,
                "mdns": status.mdns,
                "kademlia": status.kademlia,
                "broadcasted_messages": status.broadcasted_messages,
                "publish_attempts": status.publish_attempts,
                "seen_message_ids": status.seen_message_ids,
                "queued_messages": status.queued_messages,
                "inbound_messages": status.inbound_messages,
                "runtime_started": status.runtime_started,
                "runtime_mode_detail": status.runtime_mode_detail,
                "swarm_events_seen": status.swarm_events_seen,
                "subscriptions_active": status.subscriptions_active,
                "last_message_kind": status.last_message_kind,
                "last_swarm_event": status.last_swarm_event,
                "per_topic_publishes": status.per_topic_publishes,
                "inbound_decode_failed": status.inbound_decode_failed,
                "inbound_chain_mismatch_dropped": status.inbound_chain_mismatch_dropped,
                "inbound_duplicates_suppressed": status.inbound_duplicates_suppressed,
                "last_drop_reason": status.last_drop_reason,
                "peer_reconnect_attempts": status.peer_reconnect_attempts,
                "peer_recovery_success_count": status.peer_recovery_success_count,
                "last_peer_recovery_unix": status.last_peer_recovery_unix,
                "peer_cooldown_suppressed_count": status.peer_cooldown_suppressed_count,
                "peer_flap_suppressed_count": status.peer_flap_suppressed_count,
                "peers_under_cooldown": status.peers_under_cooldown,
                "peers_under_flap_guard": status.peers_under_flap_guard,
                "peer_recovery": status.peer_recovery.into_iter().map(|peer| serde_json::json!({
                    "peer_id": peer.peer_id,
                    "score": peer.score,
                    "fail_streak": peer.fail_streak,
                    "connected": peer.connected,
                    "next_retry_unix": peer.next_retry_unix,
                    "reconnect_attempts": peer.reconnect_attempts,
                    "recovery_success_count": peer.recovery_success_count,
                    "last_recovery_unix": peer.last_recovery_unix,
                    "cooldown_suppressed_count": peer.cooldown_suppressed_count,
                    "flap_suppressed_count": peer.flap_suppressed_count,
                    "flap_events": peer.flap_events,
                    "suppression_until_unix": peer.suppression_until_unix
                })).collect::<Vec<_>>()
            }))),
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
