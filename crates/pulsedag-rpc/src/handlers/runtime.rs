use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResponse, RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct RuntimeStatusData {
    pub started_at_unix: u64,
    pub uptime_secs: u64,
    pub burn_in_target_days: u64,
    pub burn_in_elapsed_days: u64,
    pub burn_in_remaining_days: u64,
    pub accepted_p2p_blocks: u64,
    pub rejected_p2p_blocks: u64,
    pub duplicate_p2p_blocks: u64,
    pub queued_orphan_blocks: u64,
    pub adopted_orphan_blocks: u64,
    pub accepted_p2p_txs: u64,
    pub rejected_p2p_txs: u64,
    pub duplicate_p2p_txs: u64,
    pub accepted_mined_blocks: u64,
    pub rejected_mined_blocks: u64,
    pub startup_snapshot_exists: bool,
    pub startup_persisted_block_count: usize,
    pub startup_persisted_max_height: u64,
    pub startup_consistency_issue_count: usize,
    pub startup_recovery_mode: String,
    pub startup_rebuild_reason: Option<String>,
    pub last_self_audit_unix: Option<u64>,
    pub last_self_audit_ok: bool,
    pub last_self_audit_issue_count: usize,
    pub last_self_audit_message: Option<String>,
    pub last_observed_best_height: u64,
    pub last_height_change_unix: Option<u64>,
    pub active_alerts: Vec<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
}

pub async fn get_runtime_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<RuntimeStatusData>> {
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(runtime.started_at_unix);
    let uptime_secs = now.saturating_sub(runtime.started_at_unix);
    let burn_in_target_days: u64 = 30;
    let burn_in_elapsed_days = uptime_secs / 86_400;
    let burn_in_remaining_days = burn_in_target_days.saturating_sub(burn_in_elapsed_days);
    let chain = state.chain().read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    Json(ApiResponse::ok(RuntimeStatusData {
        started_at_unix: runtime.started_at_unix,
        uptime_secs,
        burn_in_target_days,
        burn_in_elapsed_days,
        burn_in_remaining_days,
        accepted_p2p_blocks: runtime.accepted_p2p_blocks,
        rejected_p2p_blocks: runtime.rejected_p2p_blocks,
        duplicate_p2p_blocks: runtime.duplicate_p2p_blocks,
        queued_orphan_blocks: runtime.queued_orphan_blocks,
        adopted_orphan_blocks: runtime.adopted_orphan_blocks,
        accepted_p2p_txs: runtime.accepted_p2p_txs,
        rejected_p2p_txs: runtime.rejected_p2p_txs,
        duplicate_p2p_txs: runtime.duplicate_p2p_txs,
        accepted_mined_blocks: runtime.accepted_mined_blocks,
        rejected_mined_blocks: runtime.rejected_mined_blocks,
        startup_snapshot_exists: runtime.startup_snapshot_exists,
        startup_persisted_block_count: runtime.startup_persisted_block_count,
        startup_persisted_max_height: runtime.startup_persisted_max_height,
        startup_consistency_issue_count: runtime.startup_consistency_issue_count,
        startup_recovery_mode: runtime.startup_recovery_mode.clone(),
        startup_rebuild_reason: runtime.startup_rebuild_reason.clone(),
        last_self_audit_unix: runtime.last_self_audit_unix,
        last_self_audit_ok: runtime.last_self_audit_ok,
        last_self_audit_issue_count: runtime.last_self_audit_issue_count,
        last_self_audit_message: runtime.last_self_audit_message.clone(),
        last_observed_best_height: runtime.last_observed_best_height,
        last_height_change_unix: runtime.last_height_change_unix,
        active_alerts: runtime.active_alerts.clone(),
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
    }))
}

#[derive(Debug, Deserialize)]
pub struct RuntimeEventsQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeEventsData {
    pub count: usize,
    pub events: Vec<pulsedag_storage::RuntimeEvent>,
}

pub async fn get_runtime_events<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<RuntimeEventsQuery>,
) -> Json<ApiResponse<RuntimeEventsData>> {
    let limit = query.limit.unwrap_or(20).min(200);
    match state.storage().list_runtime_events(limit) {
        Ok(events) => Json(ApiResponse::ok(RuntimeEventsData {
            count: events.len(),
            events,
        })),
        Err(e) => Json(ApiResponse::err("RUNTIME_EVENTS_ERROR", &e.to_string())),
    }
}

#[derive(Debug, Serialize)]
pub struct RuntimeEventsSummaryData {
    pub scanned_event_count: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub by_level: BTreeMap<String, usize>,
}

pub async fn get_runtime_events_summary<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<RuntimeEventsQuery>,
) -> Json<ApiResponse<RuntimeEventsSummaryData>> {
    let limit = query.limit.unwrap_or(200).min(2000);
    match state.storage().list_runtime_events(limit) {
        Ok(events) => {
            let mut by_kind = BTreeMap::new();
            let mut by_level = BTreeMap::new();
            for event in &events {
                *by_kind.entry(event.kind.clone()).or_insert(0) += 1;
                *by_level.entry(event.level.clone()).or_insert(0) += 1;
            }
            Json(ApiResponse::ok(RuntimeEventsSummaryData {
                scanned_event_count: events.len(),
                by_kind,
                by_level,
            }))
        }
        Err(e) => Json(ApiResponse::err(
            "RUNTIME_EVENTS_SUMMARY_ERROR",
            &e.to_string(),
        )),
    }
}
