use axum::{extract::State, Json};
use pulsedag_p2p::{
    connected_peers_semantics, mode_connected_peers_are_real_network, P2pStatus, PeerRecoveryStatus,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::{
    p2p_status_for_rpc, read_chain_for_rpc, read_runtime_for_rpc, ApiResponse, RpcStateLike,
};

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

fn p2p_readiness_reasons(
    enabled: bool,
    status: Option<&P2pStatus>,
    runtime: &crate::api::NodeRuntimeStats,
    pending_missing_parents: usize,
    orphan_count: usize,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if !enabled {
        reasons.push("p2p is disabled".to_string());
        return reasons;
    }
    let Some(status) = status else {
        reasons.push("p2p status unavailable".to_string());
        return reasons;
    };
    if !mode_connected_peers_are_real_network(&status.mode) {
        reasons.push(format!(
            "p2p mode {} does not represent real network peers",
            status.mode
        ));
    }
    if status.connected_peers.is_empty() {
        reasons.push("no connected peers".to_string());
    }
    if status.listening.is_empty() {
        reasons.push("no listening addresses reported".to_string());
    }
    if runtime.pending_block_requests > 0 {
        reasons.push(format!(
            "{} pending block request(s)",
            runtime.pending_block_requests
        ));
    }
    if pending_missing_parents > 0 {
        reasons.push(format!(
            "{} pending missing parent(s)",
            pending_missing_parents
        ));
    }
    if orphan_count > 0 {
        reasons.push(format!("{} orphan block(s) queued", orphan_count));
    }
    if runtime.sync_pipeline.last_error.is_some() || runtime.sync_state == "degraded" {
        reasons.push("sync pipeline reports degraded state".to_string());
    }
    reasons
}

fn disabled_p2p_payload(
    runtime: &crate::api::NodeRuntimeStats,
    pending_missing_parents: usize,
    orphan_count: usize,
) -> serde_json::Value {
    let reasons =
        p2p_readiness_reasons(false, None, runtime, pending_missing_parents, orphan_count);
    serde_json::json!({
        "p2p_enabled": false,
        "chain_id": null,
        "p2p_mode": "disabled",
        "mode": "disabled",
        "local_node_id": null,
        "peer_count": 0,
        "connected_peers": [],
        "listening_addresses": [],
        "listening": [],
        "topics": [],
        "pending_block_requests": runtime.pending_block_requests,
        "inflight_block_requests": runtime.inflight_block_requests,
        "pending_block_request_hashes": runtime.pending_block_request_hashes,
        "duplicate_block_requests_suppressed": runtime.duplicate_block_requests_suppressed,
        "pending_missing_parents": pending_missing_parents,
        "orphan_count": orphan_count,
        "sync_state": runtime.sync_state,
        "selected_sync_peer": runtime.sync_pipeline.selected_peer,
        "last_accepted_peer_block": runtime.last_accepted_peer_block,
        "last_rejected_peer_block_reason": runtime.last_rejected_peer_block_reason,
        "inbound_chain_mismatch_dropped": 0,
        "last_drop_reason": null,
        "duplicate_suppression_counters": {
            "p2p_blocks": runtime.duplicate_p2p_blocks,
            "p2p_txs": runtime.duplicate_p2p_txs,
            "inbound_messages": 0,
            "outbound_messages": 0,
            "tx_outbound": 0,
            "block_outbound": 0
        },
        "peer_state_summary": {
            "total": 0,
            "healthy": 0,
            "watch": 0,
            "degraded": 0,
            "cooldown": 0,
            "recovering": 0,
            "peers_with_recent_failures": 0
        },
        "recovery_activity_summary": {
            "reconnect_attempts": 0,
            "recovery_success_count": 0,
            "last_recovery_unix": null,
            "cooldown_suppressed_count": 0,
            "flap_suppressed_count": 0,
            "message_rate_limited_count": 0,
            "suppressed_dial_count": 0,
            "suppressed_dials": 0,
            "peers_under_cooldown": 0,
            "peers_under_flap_guard": 0
        },
        "sync_candidates": [],
        "peer_recovery": [],
        "p2p_ready_for_private_rehearsal": false,
        "readiness_reasons": reasons
    })
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
    match p2p_status_for_rpc(state.p2p(), "/p2p/status").await {
        Ok(Some(snapshot)) => {
            let status = snapshot.status;
            let p2p_status_stale = snapshot.stale;
            let p2p_status_degraded_reason = snapshot.degraded_reason;
            let p2p_status_captured_at_unix = snapshot.captured_at_unix;
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
                        "chain_id": peer.chain_id,
                        "chain_id_compatible": peer.chain_id_compatible,
                        "last_activity_unix": peer.last_activity_unix,
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
            payload.insert("chain_id".into(), serde_json::json!(status.chain_id));
            payload.insert(
                "p2p_status_stale".into(),
                serde_json::json!(p2p_status_stale),
            );
            payload.insert(
                "p2p_status_degraded".into(),
                serde_json::json!(p2p_status_stale || p2p_status_degraded_reason.is_some()),
            );
            payload.insert(
                "p2p_status_degraded_reason".into(),
                serde_json::json!(p2p_status_degraded_reason),
            );
            payload.insert(
                "p2p_status_captured_at_unix".into(),
                serde_json::json!(p2p_status_captured_at_unix),
            );
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
            payload.insert("local_node_id".into(), serde_json::json!(status.peer_id));
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
                "outbound_duplicates_suppressed".into(),
                serde_json::json!(status.outbound_duplicates_suppressed),
            );
            payload.insert(
                "inv_blocks_received".into(),
                serde_json::json!(status.inv_blocks_received),
            );
            payload.insert(
                "inv_hashes_known".into(),
                serde_json::json!(status.inv_hashes_known),
            );
            payload.insert(
                "inv_hashes_requested".into(),
                serde_json::json!(status.inv_hashes_requested),
            );
            payload.insert(
                "header_requests_received".into(),
                serde_json::json!(status.header_requests_received),
            );
            payload.insert(
                "header_requests_sent".into(),
                serde_json::json!(status.header_requests_sent),
            );
            payload.insert(
                "headers_received".into(),
                serde_json::json!(status.headers_received),
            );
            payload.insert(
                "headers_sent".into(),
                serde_json::json!(status.headers_sent),
            );
            payload.insert(
                "headers_announced".into(),
                serde_json::json!(status.headers_announced),
            );
            payload.insert(
                "dependency_fetches_scheduled".into(),
                serde_json::json!(status.dependency_fetches_scheduled),
            );
            payload.insert(
                "parent_first_fetches".into(),
                serde_json::json!(status.parent_first_fetches),
            );
            payload.insert(
                "relay_loop_prevented".into(),
                serde_json::json!(status.relay_loop_prevented),
            );
            payload.insert(
                "relay_settings".into(),
                serde_json::json!({
                    "seen_cache_ttl_secs": status.seen_cache_ttl_secs,
                    "recovery_rebroadcast_ttl_secs": status.recovery_rebroadcast_ttl_secs,
                    "max_inventory_length": status.max_inventory_length,
                    "max_request_fanout": status.max_request_fanout
                }),
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
                "peer_message_rate_limited_count".into(),
                serde_json::json!(status.peer_message_rate_limited_count),
            );
            payload.insert(
                "peer_suppressed_dial_count".into(),
                serde_json::json!(status.peer_suppressed_dial_count),
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
                            "chain_compatible": status.peer_recovery.iter().filter(|peer| peer.chain_id_compatible).count(),
                            "chain_incompatible_or_unknown": status.peer_recovery.iter().filter(|peer| !peer.chain_id_compatible).count(),
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
                    "message_rate_limited_count": status.peer_message_rate_limited_count,
                    "suppressed_dial_count": status.peer_suppressed_dial_count,
                    "suppressed_dials": status.peer_suppressed_dial_count,
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
            let runtime_handle = state.runtime();
            let runtime = match read_runtime_for_rpc(&runtime_handle, "/p2p/status").await {
                Ok(runtime) => runtime,
                Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
            };
            let chain_handle = state.chain();
            let chain = match read_chain_for_rpc(&chain_handle, "/p2p/status").await {
                Ok(chain) => chain,
                Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
            };
            let orphan_count = chain.orphan_blocks.len();
            let missing_parent_entry_count = chain.orphan_missing_parents.len();
            let missing_parent_index_entry_count = chain.orphan_parent_index.len();
            let pending_missing_parents = pulsedag_core::pending_missing_parent_count(&chain);
            let missing_parent_index_mismatch = orphan_count > 0
                && pending_missing_parents > 0
                && missing_parent_entry_count == 0
                && missing_parent_index_entry_count == 0;
            let readiness_reasons = p2p_readiness_reasons(
                true,
                Some(&status),
                &runtime,
                pending_missing_parents,
                orphan_count,
            );
            payload.insert("p2p_enabled".into(), serde_json::json!(true));
            payload.insert("p2p_mode".into(), serde_json::json!(status.mode));
            payload.insert(
                "peer_count".into(),
                serde_json::json!(status.connected_peers.len()),
            );
            payload.insert(
                "listening_addresses".into(),
                serde_json::json!(status.listening),
            );
            payload.insert(
                "pending_block_requests".into(),
                serde_json::json!(runtime.pending_block_requests),
            );
            payload.insert(
                "inflight_block_requests".into(),
                serde_json::json!(runtime.inflight_block_requests),
            );
            payload.insert(
                "pending_block_request_hashes".into(),
                serde_json::json!(runtime.pending_block_request_hashes),
            );
            payload.insert(
                "duplicate_block_requests_suppressed".into(),
                serde_json::json!(runtime.duplicate_block_requests_suppressed),
            );
            payload.insert(
                "pending_missing_parents".into(),
                serde_json::json!(pending_missing_parents),
            );
            payload.insert(
                "missing_parent_entry_count".into(),
                serde_json::json!(missing_parent_entry_count),
            );
            payload.insert(
                "missing_parent_index_entry_count".into(),
                serde_json::json!(missing_parent_index_entry_count),
            );
            payload.insert(
                "missing_parent_index_mismatch".into(),
                serde_json::json!(missing_parent_index_mismatch),
            );
            payload.insert("orphan_count".into(), serde_json::json!(orphan_count));
            payload.insert("sync_state".into(), serde_json::json!(runtime.sync_state));
            payload.insert(
                "last_accepted_peer_block".into(),
                serde_json::json!(runtime.last_accepted_peer_block),
            );
            payload.insert(
                "last_rejected_peer_block_reason".into(),
                serde_json::json!(runtime.last_rejected_peer_block_reason),
            );
            payload.insert(
                "tx_propagation_counters".into(),
                serde_json::json!({
                    "inbound_received": runtime.tx_inbound_received,
                    "inbound_accepted": runtime.tx_inbound_accepted,
                    "inbound_duplicate": runtime.tx_inbound_duplicate,
                    "inbound_invalid": runtime.tx_inbound_invalid,
                    "relayed": runtime.tx_relayed,
                    "relay_suppressed_budget": runtime.tx_relay_suppressed_budget,
                    "relay_suppressed_duplicate": runtime.tx_relay_suppressed_duplicate,
                    "outbound_duplicates_suppressed": status.tx_outbound_duplicates_suppressed
                }),
            );
            payload.insert("block_propagation_counters".into(), serde_json::json!({
                    "announces_received": runtime.block_announces_received,
                    "getblock_sent": runtime.getblock_sent,
                    "getblock_received": runtime.getblock_received,
                    "blockdata_sent": runtime.blockdata_sent,
                    "blockdata_received": runtime.blockdata_received,
                    "blockdata_accepted": runtime.blockdata_accepted,
                    "blockdata_duplicate": runtime.blockdata_duplicate,
                    "blockdata_missing_parent": runtime.blockdata_missing_parent,
                    "blockdata_not_found": runtime.blockdata_not_found,
                    "missing_parent_requests_sent": runtime.missing_parent_requests_sent,
                    "missing_parent_responses_received": runtime.missing_parent_responses_received,
                    "missing_parent_request_timeouts": runtime.missing_parent_request_timeouts,
                    "missing_parent_request_retries": runtime.missing_parent_request_retries,
                    "missing_parent_request_fallbacks": runtime.missing_parent_request_fallbacks,
                    "block_request_retries": runtime.block_request_retries,
                    "block_request_fallbacks": runtime.block_request_fallbacks,
                    "orphan_blocks_queued": runtime.orphan_blocks_queued,
                    "orphan_blocks_retried": runtime.orphan_blocks_retried,
                    "orphan_blocks_resolved": runtime.orphan_blocks_resolved,
                    "orphan_blocks_evicted": runtime.orphan_blocks_evicted,
                    "block_request_timeouts": runtime.block_request_timeouts,
                    "duplicate_block_requests_suppressed": runtime.duplicate_block_requests_suppressed,
                    "pending_block_requests": runtime.pending_block_requests,
                    "inflight_block_requests": runtime.inflight_block_requests,
                    "pending_block_request_hashes": runtime.pending_block_request_hashes,
                    "scheduler_queue_depth": runtime.block_fetch_scheduler_queue_depth,
                    "inflight_by_peer": runtime.block_fetch_scheduler_inflight_by_peer,
                    "pending_missing_parents": pending_missing_parents,
                    "missing_parent_entry_count": missing_parent_entry_count,
                    "missing_parent_index_entry_count": missing_parent_index_entry_count,
                    "missing_parent_index_mismatch": missing_parent_index_mismatch,
                    "max_orphan_age_secs": runtime.max_orphan_age_secs,
                    "oldest_missing_parent_age_secs": runtime.oldest_missing_parent_age_secs,
                    "orphan_reprocess_attempts": runtime.orphan_reprocess_attempts,
                    "orphan_reprocess_success": runtime.orphan_reprocess_success,
                    "orphan_reprocess_failed_missing_parent": runtime.orphan_reprocess_failed_missing_parent,
                    "outbound_duplicates_suppressed": status.block_outbound_duplicates_suppressed
                }));
            payload.insert(
                "duplicate_suppression_counters".into(),
                serde_json::json!({
                    "p2p_blocks": runtime.duplicate_p2p_blocks,
                    "p2p_txs": runtime.duplicate_p2p_txs,
                    "inbound_messages": status.inbound_duplicates_suppressed,
                    "outbound_messages": status.outbound_duplicates_suppressed,
                    "tx_outbound": status.tx_outbound_duplicates_suppressed,
                    "block_outbound": status.block_outbound_duplicates_suppressed
                }),
            );
            payload.insert(
                "p2p_ready_for_private_rehearsal".into(),
                serde_json::json!(readiness_reasons.is_empty()),
            );
            payload.insert(
                "readiness_reasons".into(),
                serde_json::json!(readiness_reasons),
            );
            payload.insert("sync_candidates".into(), serde_json::json!(sync_candidates));
            payload.insert("peer_recovery".into(), serde_json::json!(peer_recovery));
            Json(ApiResponse::ok(serde_json::Value::Object(payload)))
        }
        Ok(None) => {
            let runtime_handle = state.runtime();
            let runtime = match read_runtime_for_rpc(&runtime_handle, "/p2p/status").await {
                Ok(runtime) => runtime,
                Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
            };
            let chain_handle = state.chain();
            let chain = match read_chain_for_rpc(&chain_handle, "/p2p/status").await {
                Ok(chain) => chain,
                Err(e) => return Json(ApiResponse::err("STATE_LOCK_BUSY", e)),
            };
            Json(ApiResponse::ok(disabled_p2p_payload(
                &runtime,
                pulsedag_core::pending_missing_parent_count(&chain),
                chain.orphan_blocks.len(),
            )))
        }
        Err(e) => Json(ApiResponse::err("P2P_STATUS_BUSY", e)),
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
        None => Json(ApiResponse::ok(P2pPeersData {
            count: 0,
            peers: Vec::new(),
        })),
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
        None => Json(ApiResponse::ok(P2pTopicsData {
            count: 0,
            topics: Vec::new(),
            per_topic_publishes: std::collections::HashMap::new(),
        })),
    }
}

pub async fn get_p2p_propagation<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<serde_json::Value>> {
    let runtime_handle = state.runtime();
    let runtime = read_runtime_for_rpc(&runtime_handle, "/p2p/propagation")
        .await
        .map_err(|e| Json(ApiResponse::err("STATE_LOCK_BUSY", e)));
    let runtime = match runtime {
        Ok(runtime) => runtime,
        Err(response) => return response,
    };
    let mut payload = serde_json::json!({
        "p2p_enabled": false,
        "p2p_mode": "disabled",
        "peer_count": 0,
        "tx_propagation_counters": {
            "inbound_received": runtime.tx_inbound_received,
            "inbound_accepted": runtime.tx_inbound_accepted,
            "inbound_duplicate": runtime.tx_inbound_duplicate,
            "inbound_invalid": runtime.tx_inbound_invalid,
            "relayed": runtime.tx_relayed,
            "relay_suppressed_budget": runtime.tx_relay_suppressed_budget,
            "relay_suppressed_duplicate": runtime.tx_relay_suppressed_duplicate,
            "rebroadcast_attempts": runtime.tx_rebroadcast_attempts,
            "rebroadcast_success": runtime.tx_rebroadcast_success,
            "rebroadcast_failed": runtime.tx_rebroadcast_failed
        },
        "block_propagation_counters": {
            "announces_received": runtime.block_announces_received,
            "getblock_sent": runtime.getblock_sent,
            "getblock_received": runtime.getblock_received,
            "blockdata_sent": runtime.blockdata_sent,
            "blockdata_received": runtime.blockdata_received,
            "blockdata_accepted": runtime.blockdata_accepted,
            "blockdata_duplicate": runtime.blockdata_duplicate,
            "blockdata_missing_parent": runtime.blockdata_missing_parent,
            "blockdata_not_found": runtime.blockdata_not_found,
            "missing_parent_requests_sent": runtime.missing_parent_requests_sent,
            "missing_parent_responses_received": runtime.missing_parent_responses_received,
            "missing_parent_request_timeouts": runtime.missing_parent_request_timeouts,
            "missing_parent_request_retries": runtime.missing_parent_request_retries,
            "missing_parent_request_fallbacks": runtime.missing_parent_request_fallbacks,
            "block_request_retries": runtime.block_request_retries,
            "block_request_fallbacks": runtime.block_request_fallbacks,
            "orphan_blocks_queued": runtime.orphan_blocks_queued,
            "orphan_blocks_retried": runtime.orphan_blocks_retried,
            "orphan_blocks_resolved": runtime.orphan_blocks_resolved,
            "orphan_blocks_evicted": runtime.orphan_blocks_evicted,
            "block_request_timeouts": runtime.block_request_timeouts,
            "duplicate_block_requests_suppressed": runtime.duplicate_block_requests_suppressed,
            "pending_block_requests": runtime.pending_block_requests,
            "inflight_block_requests": runtime.inflight_block_requests,
            "pending_block_request_hashes": runtime.pending_block_request_hashes,
            "scheduler_queue_depth": runtime.block_fetch_scheduler_queue_depth,
            "inflight_by_peer": runtime.block_fetch_scheduler_inflight_by_peer,
            "max_orphan_age_secs": runtime.max_orphan_age_secs,
            "oldest_missing_parent_age_secs": runtime.oldest_missing_parent_age_secs,
            "orphan_reprocess_attempts": runtime.orphan_reprocess_attempts,
            "orphan_reprocess_success": runtime.orphan_reprocess_success,
            "orphan_reprocess_failed_missing_parent": runtime.orphan_reprocess_failed_missing_parent
        },
        "duplicate_suppression_counters": {
            "p2p_blocks": runtime.duplicate_p2p_blocks,
            "p2p_txs": runtime.duplicate_p2p_txs
        }
    });
    drop(runtime);
    match p2p_status_for_rpc(state.p2p(), "/p2p/propagation").await {
        Ok(Some(snapshot)) => {
            let status = snapshot.status;
            payload["p2p_enabled"] = serde_json::json!(true);
            payload["p2p_mode"] = serde_json::json!(status.mode);
            payload["peer_count"] = serde_json::json!(status.connected_peers.len());
            payload["p2p_status_stale"] = serde_json::json!(snapshot.stale);
            payload["p2p_status_degraded"] =
                serde_json::json!(snapshot.stale || snapshot.degraded_reason.is_some());
            payload["p2p_status_degraded_reason"] = serde_json::json!(snapshot.degraded_reason);
            payload["p2p_status_captured_at_unix"] = serde_json::json!(snapshot.captured_at_unix);
            payload["duplicate_suppression_counters"]["inbound_messages"] =
                serde_json::json!(status.inbound_duplicates_suppressed);
            payload["duplicate_suppression_counters"]["outbound_messages"] =
                serde_json::json!(status.outbound_duplicates_suppressed);
            payload["duplicate_suppression_counters"]["tx_outbound"] =
                serde_json::json!(status.tx_outbound_duplicates_suppressed);
            payload["duplicate_suppression_counters"]["block_outbound"] =
                serde_json::json!(status.block_outbound_duplicates_suppressed);
        }
        Ok(None) => {}
        Err(e) => {
            payload["p2p_status_busy"] = serde_json::json!(true);
            payload["p2p_status_error"] = serde_json::json!(e);
        }
    }
    Json(ApiResponse::ok(payload))
}

#[cfg(test)]
mod tests {
    use super::{get_p2p_propagation, get_p2p_status};
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::{extract::State, Json};
    use pulsedag_core::ChainState;
    use pulsedag_p2p::{
        P2pHandle, P2pStatus, PeerRecoveryStatus, P2P_MODE_LIBP2P_DEV_LOOPBACK_SKELETON,
        P2P_MODE_LIBP2P_REAL, P2P_MODE_MEMORY_SIMULATED,
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

    fn mk_state_without_p2p() -> TestState {
        let path = temp_db_path("p2p-disabled");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: None,
        }
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
            chain_id: "testnet-dev".into(),
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
            queue_backpressure_drops: 0,
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
            peer_reconnect_attempts: 12,
            peer_recovery_success_count: 3,
            last_peer_recovery_unix: Some(now.saturating_sub(10)),
            peer_cooldown_suppressed_count: 2,
            peer_flap_suppressed_count: 1,
            peer_message_rate_limited_count: 2,
            peer_suppressed_dial_count: 1,
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
                    chain_id: Some("testnet-dev".into()),
                    chain_id_compatible: true,
                    last_activity_unix: Some(now),
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
                    chain_id: Some("testnet-dev".into()),
                    chain_id_compatible: true,
                    last_activity_unix: Some(now),
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
            bootnode_next_redial_at: std::collections::HashMap::new(),
            bootnode_redial_backoff_secs: std::collections::HashMap::new(),
            last_bootnode_dial_error: None,
            gossipsub_peer_count: 0,
            subscribed_topics: Vec::new(),
            connection_established_total: 0,
            connection_closed_total: 0,
            last_connection_closed_reason: None,
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
        assert_eq!(data["p2p_enabled"], true);
        assert_eq!(data["p2p_mode"], P2P_MODE_MEMORY_SIMULATED);
        assert_eq!(data["peer_count"], 1);
        assert!(data["tx_propagation_counters"].is_object());
        assert!(data["block_propagation_counters"].is_object());
        assert!(data["duplicate_suppression_counters"].is_object());
        assert!(data["sync_candidates"].is_array());
        assert!(data["peer_recovery"].is_array());
        assert!(data["block_propagation_counters"]["pending_block_requests"].is_number());
        assert!(data["block_propagation_counters"]["pending_missing_parents"].is_number());
        assert!(data["p2p_ready_for_private_rehearsal"].is_boolean());
        let text = serde_json::to_string(&data).unwrap();
        assert!(!text.contains("private_key"));
        assert!(!text.contains("secret"));
    }

    #[tokio::test]
    async fn p2p_disabled_mode_reports_structured_diagnostics() {
        let Json(resp) = get_p2p_status(State(mk_state_without_p2p())).await;
        assert!(resp.ok);
        let data = resp.data.expect("disabled p2p status data");
        assert_eq!(data["p2p_enabled"], false);
        assert_eq!(data["p2p_mode"], "disabled");
        assert_eq!(data["peer_count"], 0);
        assert_eq!(data["p2p_ready_for_private_rehearsal"], false);
        assert_eq!(data["inbound_chain_mismatch_dropped"], 0);
        assert!(data["duplicate_suppression_counters"].is_object());
        assert!(data["peer_state_summary"].is_object());
        assert!(data["recovery_activity_summary"].is_object());
        assert!(data["sync_candidates"].is_array());
        assert!(data["peer_recovery"].is_array());
        assert!(data["readiness_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "p2p is disabled"));
    }

    #[tokio::test]
    async fn p2p_propagation_returns_structured_counters_without_secrets() {
        let Json(resp) = get_p2p_propagation(State(mk_state_without_p2p())).await;
        assert!(resp.ok);
        let data = resp.data.expect("propagation data");
        assert!(data["tx_propagation_counters"].is_object());
        assert!(data["block_propagation_counters"].is_object());
        assert!(data["duplicate_suppression_counters"].is_object());
        let text = serde_json::to_string(&data).unwrap();
        assert!(!text.contains("private_key"));
        assert!(!text.contains("secret"));
    }

    #[tokio::test]
    async fn p2p_status_mode_semantics_guardrails_cover_simulated_dev_and_real_modes() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
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
            let status = P2pStatus {
                chain_id: "testnet-dev".into(),
                mode: mode.to_string(),
                peer_id: "self".into(),
                listening: vec!["memory://local".into()],
                connected_peers: vec!["peer-a".into()],
                topics: vec!["blocks".into()],
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
                last_peer_recovery_unix: Some(now),
                peer_cooldown_suppressed_count: 0,
                peer_flap_suppressed_count: 0,
                peer_message_rate_limited_count: 0,
                peer_suppressed_dial_count: 0,
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
                bootnode_next_redial_at: std::collections::HashMap::new(),
                bootnode_redial_backoff_secs: std::collections::HashMap::new(),
                last_bootnode_dial_error: None,
                gossipsub_peer_count: 0,
                subscribed_topics: Vec::new(),
                connection_established_total: 0,
                connection_closed_total: 0,
                last_connection_closed_reason: None,
            };
            let Json(resp) = get_p2p_status(State(mk_state(status.clone()))).await;
            let data = resp.data.expect("p2p status data");
            assert_eq!(data["mode"], mode);
            assert_eq!(data["runtime_mode_detail"], status.runtime_mode_detail);
            assert_eq!(data["connected_peers_are_real_network"], expect_real);
            assert_eq!(data["connected_peers_semantics"], expect_semantics);
        }
    }
}
