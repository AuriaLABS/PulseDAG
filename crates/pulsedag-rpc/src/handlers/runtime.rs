use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    convert::Infallible,
    time::Duration,
};

use async_stream::stream;
use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResponse, RpcStateLike};
use pulsedag_core::{
    combined_pressure_tier, mempool_pressure_bps, pressure_tier_from_bps, SyncPhase,
    SyncProgressCounters,
};
use pulsedag_p2p::mode_connected_peers_are_real_network;

#[derive(Debug, serde::Serialize)]
pub struct RuntimeStatusData {
    pub started_at_unix: u64,
    pub uptime_secs: u64,
    pub burn_in_target_days: u64,
    pub burn_in_elapsed_days: u64,
    pub burn_in_remaining_days: u64,
    pub node_runtime_surface_health: String,
    pub runtime_alert_classes: Vec<String>,
    pub incident_primary_surface: String,
    pub incident_summary: String,
    pub incident_indicators: Vec<String>,
    pub incident_snapshot: RuntimeIncidentSnapshot,
    pub node_health_slo_bps: u64,
    pub sync_health_slo_bps: u64,
    pub p2p_health_slo_bps: u64,
    pub mempool_health_slo_bps: u64,
    pub mining_health_slo_bps: u64,
    pub runtime_health_slo_bps: u64,
    pub accepted_p2p_blocks: u64,
    pub rejected_p2p_blocks: u64,
    pub duplicate_p2p_blocks: u64,
    pub queued_orphan_blocks: u64,
    pub adopted_orphan_blocks: u64,
    pub accepted_p2p_txs: u64,
    pub rejected_p2p_txs: u64,
    pub duplicate_p2p_txs: u64,
    pub dropped_p2p_txs: u64,
    pub dropped_p2p_txs_duplicate_mempool: u64,
    pub dropped_p2p_txs_duplicate_confirmed: u64,
    pub dropped_p2p_txs_accept_failed: u64,
    pub dropped_p2p_txs_persist_failed: u64,
    pub tx_rebroadcast_attempts: u64,
    pub tx_rebroadcast_success: u64,
    pub tx_rebroadcast_failed: u64,
    pub tx_rebroadcast_skipped_no_p2p: u64,
    pub tx_rebroadcast_skipped_no_peers: u64,
    pub last_tx_rebroadcast_unix: Option<u64>,
    pub last_tx_rebroadcast_error: Option<String>,
    pub tx_inbound_counters_coherent: bool,
    pub tx_inbound_counter_delta: i64,
    pub tx_drop_reason_counters_coherent: bool,
    pub tx_drop_reason_counter_delta: i64,
    pub tx_rebroadcast_outcomes_coherent: bool,
    pub tx_rebroadcast_outcome_counter_delta: i64,
    pub tx_propagation_health: String,
    pub tx_inbound_total: u64,
    pub tx_inbound_accepted_total: u64,
    pub tx_inbound_rejected_total: u64,
    pub tx_inbound_dropped_total: u64,
    pub last_tx_accept_unix: Option<u64>,
    pub last_tx_reject_unix: Option<u64>,
    pub last_tx_drop_unix: Option<u64>,
    pub last_tx_drop_reason: Option<String>,
    pub last_tx_drop_txid: Option<String>,
    pub tx_drop_reasons: Vec<String>,
    pub mempool_transactions: usize,
    pub mempool_max_transactions: usize,
    pub mempool_orphan_transactions: usize,
    pub mempool_max_orphans: usize,
    pub mempool_pending_transactions: usize,
    pub mempool_capacity_remaining_transactions: usize,
    pub mempool_pressure_bps: u64,
    pub mempool_orphan_pressure_bps: u64,
    pub mempool_pressure_tier: String,
    pub mempool_orphan_pressure_tier: String,
    pub mempool_backpressure_active: bool,
    pub mempool_backpressure_signal: String,
    pub mempool_surface_health: String,
    pub mempool_admitted_total: u64,
    pub mempool_rejected_total: u64,
    pub mempool_rejected_low_priority_total: u64,
    pub mempool_evicted_total: u64,
    pub mempool_pressure_events_total: u64,
    pub mempool_reconcile_runs_total: u64,
    pub mempool_reconcile_removed_total: u64,
    pub mempool_orphaned_total: u64,
    pub mempool_orphan_promoted_total: u64,
    pub mempool_orphan_dropped_total: u64,
    pub mempool_orphan_pruned_total: u64,
    pub accepted_mined_blocks: u64,
    pub rejected_mined_blocks: u64,
    pub external_mining_templates_emitted: u64,
    pub external_mining_templates_invalidated: u64,
    pub external_mining_stale_work_detected: u64,
    pub external_mining_submit_accepted: u64,
    pub external_mining_submit_rejected: u64,
    pub external_mining_rejected_invalid_pow: u64,
    pub external_mining_rejected_stale_template: u64,
    pub external_mining_rejected_unknown_template: u64,
    pub external_mining_rejected_submit_block_error: u64,
    pub external_mining_rejected_duplicate_block: u64,
    pub external_mining_rejected_invalid_block: u64,
    pub external_mining_rejected_chain_id_mismatch: u64,
    pub external_mining_rejected_internal_error: u64,
    pub external_mining_rejected_storage_error: u64,
    pub external_mining_last_template_id: Option<String>,
    pub external_mining_last_rejection_kind: Option<String>,
    pub external_mining_last_rejection_reason: Option<String>,
    pub external_mining_last_invalid_pow_reason: Option<String>,
    pub external_mining_submit_total: u64,
    pub external_mining_submit_outcome_total: u64,
    pub external_mining_submit_outcome_counters_coherent: bool,
    pub external_mining_submit_outcome_counter_delta: i64,
    pub external_mining_rejection_reason_total: u64,
    pub external_mining_rejection_counters_coherent: bool,
    pub external_mining_rejection_counter_delta: i64,
    pub external_mining_stale_work_submit_rejections: u64,
    pub external_mining_stale_work_template_invalidations: u64,
    pub external_mining_template_health: String,
    pub external_mining_template_stale_submit_ratio_bps: u64,
    pub external_mining_hashrate_hps: u64,
    pub external_mining_worker_efficiency_bps: u64,
    pub external_mining_stale_efficiency_bps: u64,
    pub external_mining_template_usefulness_bps: u64,
    pub external_mining_template_rollup: String,
    pub external_mining_surface_health: String,
    pub startup_snapshot_exists: bool,
    pub startup_persisted_block_count: usize,
    pub startup_persisted_max_height: u64,
    pub startup_consistency_issue_count: usize,
    pub startup_recovery_mode: String,
    pub startup_rebuild_reason: Option<String>,
    pub startup_path: String,
    pub startup_bootstrap_mode: String,
    pub startup_status_summary: String,
    pub startup_fastboot_used: bool,
    pub startup_snapshot_detected: bool,
    pub startup_snapshot_validated: bool,
    pub startup_delta_applied: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
    pub startup_duration_ms: u128,
    pub last_self_audit_unix: Option<u64>,
    pub last_self_audit_ok: bool,
    pub last_self_audit_issue_count: usize,
    pub last_self_audit_message: Option<String>,
    pub recovery_confidence: String,
    pub recovery_confidence_reason: String,
    pub recovery_confidence_non_misleading: bool,
    pub last_observed_best_height: u64,
    pub last_height_change_unix: Option<u64>,
    pub active_alerts: Vec<String>,
    pub snapshot_auto_every_blocks: u64,
    pub auto_prune_enabled: bool,
    pub auto_prune_every_blocks: u64,
    pub prune_keep_recent_blocks: u64,
    pub prune_require_snapshot: bool,
    pub last_snapshot_height: Option<u64>,
    pub last_snapshot_unix: Option<u64>,
    pub last_prune_height: Option<u64>,
    pub last_prune_unix: Option<u64>,
    pub sync_phase: SyncPhase,
    pub sync_surface_health: String,
    pub sync_counters_coherent: bool,
    pub sync_last_transition_unix: Option<u64>,
    pub sync_completed_cycles: u64,
    pub sync_restart_count: u64,
    pub sync_last_error: Option<String>,
    pub sync_selected_peer: Option<String>,
    pub sync_selection_version: u64,
    pub sync_fallback_count: u64,
    pub sync_timeout_fallback_count: u64,
    pub sync_last_fallback_reason: Option<String>,
    pub sync_last_fallback_peer: Option<String>,
    pub sync_counters: SyncProgressCounters,
    pub sync_blocks_request_backlog: u64,
    pub sync_blocks_validation_backlog: u64,
    pub sync_catchup_stage: String,
    pub sync_lag_blocks: u64,
    pub sync_lag_band: String,
    pub sync_catchup_progress_bps: u64,
    pub sync_catchup_summary: String,
    pub sync_recovery_reason: Option<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub retarget_min_bps: u64,
    pub retarget_max_bps: u64,
    pub retarget_was_clamped: bool,
    pub retarget_rationale: String,
    pub retarget_signal_quality: String,
    pub suggested_difficulty: u64,
    pub p2p_peer_reconnect_attempts: u64,
    pub p2p_peer_recovery_success_count: u64,
    pub p2p_last_peer_recovery_unix: Option<u64>,
    pub p2p_peer_cooldown_suppressed_count: u64,
    pub p2p_peer_flap_suppressed_count: u64,
    pub p2p_peers_under_cooldown: usize,
    pub p2p_peers_under_flap_guard: usize,
    pub p2p_peer_lifecycle_watch: usize,
    pub p2p_peer_lifecycle_cooldown: usize,
    pub p2p_degraded_mode: String,
    pub p2p_connection_shaping_active: bool,
    pub p2p_last_peer_seen_unix: Option<u64>,
    pub p2p_peers_with_recent_failures: usize,
    pub p2p_connected_peers_are_real_network: bool,
    pub p2p_peer_health_healthy: usize,
    pub p2p_peer_health_degraded: usize,
    pub p2p_peer_health_recovering: usize,
    pub p2p_peer_health_total: usize,
    pub p2p_peer_health_counters_coherent: bool,
    pub p2p_surface_health: String,
    pub p2p_tx_outbound_duplicates_suppressed: usize,
    pub p2p_tx_outbound_first_seen_relayed: usize,
    pub p2p_tx_outbound_recovery_relayed: usize,
    pub p2p_tx_outbound_priority_relayed: usize,
    pub p2p_tx_outbound_budget_suppressed: usize,
    pub p2p_tx_relay_total_events: usize,
    pub p2p_tx_relay_duplicate_ratio_bps: u64,
    pub p2p_tx_relay_budget_suppression_ratio_bps: u64,
    pub p2p_block_outbound_duplicates_suppressed: usize,
    pub p2p_block_outbound_first_seen_relayed: usize,
    pub p2p_block_outbound_recovery_relayed: usize,
    pub p2p_block_relay_total_events: usize,
    pub p2p_block_relay_duplicate_ratio_bps: u64,
    pub p2p_inbound_duplicates_suppressed: usize,
    pub p2p_queued_block_messages: usize,
    pub p2p_queued_non_block_messages: usize,
    pub p2p_queue_max_depth: usize,
    pub p2p_dequeued_block_messages: usize,
    pub p2p_dequeued_non_block_messages: usize,
    pub p2p_queue_block_priority_picks: usize,
    pub p2p_queue_non_block_fair_picks: usize,
    pub p2p_queue_starvation_relief_picks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeIncidentSnapshot {
    pub primary_surface: String,
    pub status: String,
    pub summary: String,
    pub indicators: Vec<String>,
    pub alert_class_count: usize,
    pub runtime_health_slo_bps: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeTrendWindow {
    pub label: String,
    pub span_secs: u64,
    pub event_count: usize,
    pub warn_or_error_count: usize,
    pub dominant_kind: Option<String>,
    pub incident_snapshot: RuntimeIncidentSnapshot,
}

#[derive(Debug, Clone)]
struct SyncCatchupView {
    stage: String,
    lag_blocks: u64,
    lag_band: String,
    progress_bps: u64,
    summary: String,
    recovery_reason: Option<String>,
}

fn lag_band(lag_blocks: u64) -> &'static str {
    match lag_blocks {
        0 => "aligned",
        1..=2 => "near_tip",
        3..=10 => "catching_up",
        11..=100 => "lagging",
        _ => "severely_lagging",
    }
}

fn sync_catchup_view(runtime: &crate::api::NodeRuntimeStats, now_unix: u64) -> SyncCatchupView {
    let request_backlog = runtime
        .sync_pipeline
        .counters
        .blocks_requested
        .saturating_sub(runtime.sync_pipeline.counters.blocks_acquired);
    let validation_backlog = runtime
        .sync_pipeline
        .counters
        .blocks_acquired
        .saturating_sub(runtime.sync_pipeline.counters.blocks_applied);
    let lag_blocks = request_backlog.max(validation_backlog);
    let lag_band = lag_band(lag_blocks).to_string();
    let progress_bps = if runtime.sync_pipeline.counters.blocks_requested == 0 {
        if lag_blocks == 0 {
            10_000
        } else {
            0
        }
    } else {
        runtime
            .sync_pipeline
            .counters
            .blocks_applied
            .saturating_mul(10_000)
            .saturating_div(runtime.sync_pipeline.counters.blocks_requested)
            .min(10_000)
    };
    let sync_counters_coherent = runtime.sync_pipeline.counters.blocks_applied
        <= runtime.sync_pipeline.counters.blocks_validated
        && runtime.sync_pipeline.counters.blocks_validated
            <= runtime.sync_pipeline.counters.blocks_acquired
        && runtime.sync_pipeline.counters.blocks_acquired
            <= runtime.sync_pipeline.counters.blocks_requested;
    let stage = if runtime.sync_pipeline.last_error.is_some() || !sync_counters_coherent {
        "degraded"
    } else {
        match runtime.sync_pipeline.phase {
            SyncPhase::Idle if lag_blocks == 0 => "steady",
            SyncPhase::Idle => "recovering",
            SyncPhase::PeerSelection => "discovering",
            SyncPhase::HeaderDiscovery => "discovering",
            SyncPhase::BlockAcquisition => "acquiring",
            SyncPhase::ValidationApplication => "validating",
            SyncPhase::CatchUpCompletion => "steady",
        }
    }
    .to_string();
    let stalled = runtime.sync_pipeline.phase != SyncPhase::Idle
        && lag_blocks > 0
        && runtime
            .sync_pipeline
            .last_transition_unix
            .map(|ts| now_unix.saturating_sub(ts) > 120)
            .unwrap_or(false);
    let recovery_reason = if let Some(err) = runtime.sync_pipeline.last_error.clone() {
        Some(format!("sync error: {err}"))
    } else if !sync_counters_coherent {
        Some("sync counter incoherence detected; verify sync pipeline accounting".to_string())
    } else if stalled {
        Some(format!(
            "sync stalled in {:?} with lag_band={lag_band}; last transition over 120s ago",
            runtime.sync_pipeline.phase
        ))
    } else if lag_blocks > 0 {
        Some(format!(
            "catch-up in progress: stage={stage}, lag_band={lag_band}, request_backlog={request_backlog}, validation_backlog={validation_backlog}"
        ))
    } else {
        None
    };
    let summary = format!(
        "stage={stage} lag_blocks={lag_blocks} lag_band={lag_band} progress_bps={progress_bps} request_backlog={request_backlog} validation_backlog={validation_backlog}"
    );
    SyncCatchupView {
        stage,
        lag_blocks,
        lag_band,
        progress_bps,
        summary,
        recovery_reason,
    }
}

#[derive(Default)]
struct RuntimeP2pRecoverySummary {
    reconnect_attempts: u64,
    recovery_success_count: u64,
    last_recovery_unix: Option<u64>,
    cooldown_suppressed_count: u64,
    flap_suppressed_count: u64,
    peers_under_cooldown: usize,
    peers_under_flap_guard: usize,
    peer_lifecycle_watch: usize,
    peer_lifecycle_cooldown: usize,
    degraded_mode: String,
    connection_shaping_active: bool,
    last_peer_seen_unix: Option<u64>,
    peers_with_recent_failures: usize,
    connected_peers_are_real_network: bool,
    peer_health_healthy: usize,
    peer_health_degraded: usize,
    peer_health_recovering: usize,
    tx_outbound_duplicates_suppressed: usize,
    tx_outbound_first_seen_relayed: usize,
    tx_outbound_recovery_relayed: usize,
    tx_outbound_priority_relayed: usize,
    tx_outbound_budget_suppressed: usize,
    block_outbound_duplicates_suppressed: usize,
    block_outbound_first_seen_relayed: usize,
    block_outbound_recovery_relayed: usize,
    inbound_duplicates_suppressed: usize,
    queued_block_messages: usize,
    queued_non_block_messages: usize,
    queue_max_depth: usize,
    dequeued_block_messages: usize,
    dequeued_non_block_messages: usize,
    queue_block_priority_picks: usize,
    queue_non_block_fair_picks: usize,
    queue_starvation_relief_picks: usize,
}

#[derive(Debug, Clone)]
struct StartupStatusView {
    path: String,
    bootstrap_mode: String,
    status_summary: String,
    fastboot_used: bool,
    snapshot_detected: bool,
    snapshot_validated: bool,
    delta_applied: bool,
    replay_required: bool,
    fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSurfaceRollup {
    pub startup_path: String,
    pub startup_bootstrap_mode: String,
    pub startup_status_summary: String,
    pub startup_fastboot_used: bool,
    pub startup_snapshot_detected: bool,
    pub startup_snapshot_validated: bool,
    pub startup_delta_applied: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
    pub sync_surface_health: String,
    pub sync_counters_coherent: bool,
    pub tx_propagation_health: String,
    pub tx_inbound_counters_coherent: bool,
    pub tx_drop_reason_counters_coherent: bool,
    pub tx_rebroadcast_outcomes_coherent: bool,
    pub external_mining_surface_health: String,
    pub external_mining_template_health: String,
    pub external_mining_submit_outcome_counters_coherent: bool,
    pub external_mining_rejection_counters_coherent: bool,
    pub node_runtime_surface_health: String,
    pub runtime_alert_classes: Vec<String>,
    pub incident_primary_surface: String,
    pub incident_summary: String,
    pub incident_indicators: Vec<String>,
    pub node_health_slo_bps: u64,
    pub sync_health_slo_bps: u64,
    pub p2p_health_slo_bps: u64,
    pub mempool_health_slo_bps: u64,
    pub mining_health_slo_bps: u64,
    pub runtime_health_slo_bps: u64,
    pub recovery_confidence: String,
    pub recovery_confidence_reason: String,
    pub recovery_confidence_non_misleading: bool,
}

fn degraded_penalty_bps(health: &str, counter_coherent: bool) -> u64 {
    if !counter_coherent || health == "counter_mismatch" {
        10_000
    } else if matches!(
        health,
        "degraded" | "stale_dominant" | "rebroadcast_stalled" | "saturated"
    ) {
        5_000
    } else if matches!(health, "watch" | "elevated" | "high_pressure") {
        2_000
    } else {
        0
    }
}

fn sync_surface_health(runtime: &crate::api::NodeRuntimeStats) -> (String, bool) {
    let sync_counters_coherent = runtime.sync_pipeline.counters.blocks_applied
        <= runtime.sync_pipeline.counters.blocks_validated
        && runtime.sync_pipeline.counters.blocks_validated
            <= runtime.sync_pipeline.counters.blocks_acquired
        && runtime.sync_pipeline.counters.blocks_acquired
            <= runtime.sync_pipeline.counters.blocks_requested;
    let sync_surface_health =
        if !sync_counters_coherent || runtime.sync_pipeline.last_error.is_some() {
            "degraded"
        } else if runtime.sync_pipeline.phase == SyncPhase::Idle {
            "idle"
        } else {
            "active"
        };
    (sync_surface_health.to_string(), sync_counters_coherent)
}

pub(crate) fn runtime_surface_rollup(
    runtime: &crate::api::NodeRuntimeStats,
) -> RuntimeSurfaceRollup {
    let startup = startup_status_view(runtime);
    let tx_inbound_outcome_total = runtime
        .tx_inbound_accepted_total
        .saturating_add(runtime.tx_inbound_dropped_total);
    let tx_inbound_counter_delta = i64::try_from(runtime.tx_inbound_total).unwrap_or(i64::MAX)
        - i64::try_from(tx_inbound_outcome_total).unwrap_or(i64::MAX);
    let tx_drop_reason_total = runtime
        .dropped_p2p_txs_duplicate_mempool
        .saturating_add(runtime.dropped_p2p_txs_duplicate_confirmed)
        .saturating_add(runtime.dropped_p2p_txs_accept_failed)
        .saturating_add(runtime.dropped_p2p_txs_persist_failed);
    let tx_drop_reason_counter_delta = i64::try_from(runtime.dropped_p2p_txs).unwrap_or(i64::MAX)
        - i64::try_from(tx_drop_reason_total).unwrap_or(i64::MAX);
    let tx_rebroadcast_outcome_total = runtime
        .tx_rebroadcast_success
        .saturating_add(runtime.tx_rebroadcast_failed);
    let tx_rebroadcast_outcome_counter_delta = i64::try_from(runtime.tx_rebroadcast_attempts)
        .unwrap_or(i64::MAX)
        - i64::try_from(tx_rebroadcast_outcome_total).unwrap_or(i64::MAX);
    let tx_propagation_health = if tx_inbound_counter_delta != 0
        || tx_drop_reason_counter_delta != 0
        || tx_rebroadcast_outcome_counter_delta != 0
    {
        "counter_mismatch"
    } else if runtime.tx_rebroadcast_attempts > 0 && runtime.tx_rebroadcast_success == 0 {
        "rebroadcast_stalled"
    } else if runtime.tx_rebroadcast_failed > 0
        || runtime.tx_rebroadcast_skipped_no_p2p > 0
        || runtime.tx_rebroadcast_skipped_no_peers > 0
    {
        "degraded"
    } else {
        "healthy"
    };
    let external_mining_submit_total = runtime
        .external_mining_submit_accepted
        .saturating_add(runtime.external_mining_submit_rejected);
    let external_mining_submit_outcome_total = runtime
        .accepted_mined_blocks
        .saturating_add(runtime.rejected_mined_blocks);
    let external_mining_submit_outcome_counter_delta = i64::try_from(external_mining_submit_total)
        .unwrap_or(i64::MAX)
        - i64::try_from(external_mining_submit_outcome_total).unwrap_or(i64::MAX);
    let external_mining_rejection_reason_total = runtime
        .external_mining_rejected_invalid_pow
        .saturating_add(runtime.external_mining_rejected_stale_template)
        .saturating_add(runtime.external_mining_rejected_unknown_template)
        .saturating_add(runtime.external_mining_rejected_submit_block_error)
        .saturating_add(runtime.external_mining_rejected_duplicate_block)
        .saturating_add(runtime.external_mining_rejected_invalid_block)
        .saturating_add(runtime.external_mining_rejected_chain_id_mismatch)
        .saturating_add(runtime.external_mining_rejected_internal_error)
        .saturating_add(runtime.external_mining_rejected_storage_error);
    let external_mining_rejection_counter_delta =
        i64::try_from(runtime.external_mining_submit_rejected).unwrap_or(i64::MAX)
            - i64::try_from(external_mining_rejection_reason_total).unwrap_or(i64::MAX);
    let external_mining_surface_health = if external_mining_submit_outcome_counter_delta != 0
        || external_mining_rejection_counter_delta != 0
    {
        "counter_mismatch"
    } else if runtime.external_mining_submit_rejected > 0 {
        "degraded"
    } else {
        "healthy"
    };
    let external_mining_template_stale_submit_ratio_bps = if external_mining_submit_total == 0 {
        0
    } else {
        runtime
            .external_mining_rejected_stale_template
            .saturating_mul(10_000)
            .saturating_div(external_mining_submit_total)
            .min(10_000)
    };
    let external_mining_template_health = if external_mining_submit_outcome_counter_delta != 0
        || external_mining_rejection_counter_delta != 0
    {
        "counter_mismatch"
    } else if external_mining_submit_total == 0 {
        "idle"
    } else if external_mining_template_stale_submit_ratio_bps >= 5_000 {
        "stale_dominant"
    } else if runtime.external_mining_rejected_stale_template > 0 {
        "watch"
    } else {
        "healthy"
    };
    let (sync_surface_health, sync_counters_coherent) = sync_surface_health(runtime);
    let mempool_surface_health = if runtime.active_alerts.iter().any(|alert| {
        alert.contains("[mempool_pressure]")
            || alert.contains("high mempool size")
            || alert.contains("high orphan count")
    }) {
        "elevated"
    } else {
        "normal"
    };
    let p2p_surface_health = if runtime
        .active_alerts
        .iter()
        .any(|alert| alert.contains("peer") || alert.contains("p2p"))
    {
        "degraded"
    } else {
        "healthy"
    };
    let node_runtime_surface_health = if !runtime.active_alerts.is_empty()
        || !runtime.last_self_audit_ok
        || runtime.last_self_audit_issue_count > 0
        || runtime.startup_consistency_issue_count > 0
        || sync_surface_health == "degraded"
        || external_mining_surface_health == "counter_mismatch"
        || tx_propagation_health == "counter_mismatch"
    {
        "degraded"
    } else {
        "healthy"
    };
    let mut runtime_alert_classes = BTreeMap::new();
    if !runtime.last_self_audit_ok
        || runtime.last_self_audit_issue_count > 0
        || runtime.startup_consistency_issue_count > 0
    {
        runtime_alert_classes.insert("node_integrity".to_string(), 1usize);
    }
    if sync_surface_health == "degraded" {
        runtime_alert_classes.insert("sync_pipeline".to_string(), 1usize);
    }
    if tx_propagation_health != "healthy" {
        runtime_alert_classes.insert("tx_propagation".to_string(), 1usize);
    }
    if mempool_surface_health != "normal" {
        runtime_alert_classes.insert("mempool_pressure".to_string(), 1usize);
    }
    if p2p_surface_health != "healthy" {
        runtime_alert_classes.insert("p2p_recovery".to_string(), 1usize);
    }
    if external_mining_surface_health != "healthy" || external_mining_template_health != "healthy" {
        runtime_alert_classes.insert("mining_submissions".to_string(), 1usize);
    }
    if runtime
        .active_alerts
        .iter()
        .any(|alert| alert.contains("[tip_stagnation]") || alert.contains("stagnant"))
    {
        runtime_alert_classes.insert("tip_stagnation".to_string(), 1usize);
    }
    let runtime_alert_classes: Vec<String> = runtime_alert_classes.into_keys().collect();
    let node_health_slo_bps = 10_000u64.saturating_sub(degraded_penalty_bps(
        node_runtime_surface_health,
        runtime.last_self_audit_ok && runtime.last_self_audit_issue_count == 0,
    ));
    let sync_health_slo_bps = 10_000u64.saturating_sub(degraded_penalty_bps(
        &sync_surface_health,
        sync_counters_coherent,
    ));
    let p2p_health_slo_bps = 10_000u64.saturating_sub(degraded_penalty_bps(
        p2p_surface_health,
        p2p_surface_health != "counter_mismatch",
    ));
    let mempool_health_slo_bps =
        10_000u64.saturating_sub(degraded_penalty_bps(mempool_surface_health, true));
    let mining_health_slo_bps = 10_000u64.saturating_sub(degraded_penalty_bps(
        external_mining_template_health,
        external_mining_submit_outcome_counter_delta == 0
            && external_mining_rejection_counter_delta == 0,
    ));
    let runtime_health_slo_bps = (node_health_slo_bps
        .saturating_add(sync_health_slo_bps)
        .saturating_add(p2p_health_slo_bps)
        .saturating_add(mempool_health_slo_bps)
        .saturating_add(mining_health_slo_bps))
        / 5;
    let incident_primary_surface = runtime_alert_classes
        .first()
        .cloned()
        .unwrap_or_else(|| "none".to_string());
    let mut incident_indicators = Vec::new();
    if !runtime.active_alerts.is_empty() {
        incident_indicators.push(format!(
            "active_alerts={}",
            runtime.active_alerts.join(" | ")
        ));
    }
    if sync_surface_health == "degraded" {
        incident_indicators.push(format!(
            "sync_degraded last_error={}",
            runtime
                .sync_pipeline
                .last_error
                .clone()
                .unwrap_or_else(|| "none".to_string())
        ));
    }
    if tx_propagation_health != "healthy" {
        incident_indicators.push(format!("tx_propagation={tx_propagation_health}"));
    }
    if external_mining_surface_health != "healthy" {
        incident_indicators.push(format!(
            "mining_surface={} template={}",
            external_mining_surface_health, external_mining_template_health
        ));
    }
    if incident_indicators.is_empty() {
        incident_indicators.push("no incident indicators".to_string());
    }
    let incident_summary = format!(
        "primary_surface={} classes={} runtime_health_slo_bps={}",
        incident_primary_surface,
        runtime_alert_classes.len(),
        runtime_health_slo_bps
    );
    let recovery_confidence_non_misleading = !runtime.startup_replay_required
        || runtime.startup_fallback_reason.is_some()
        || runtime.startup_consistency_issue_count == 0;
    let (recovery_confidence, recovery_confidence_reason) = if !runtime.last_self_audit_ok
        || runtime.last_self_audit_issue_count > 0
        || runtime.startup_consistency_issue_count > 0
    {
        (
            "low".to_string(),
            "self-audit or startup consistency checks report unresolved issues".to_string(),
        )
    } else if runtime.startup_fastboot_used
        && runtime.startup_snapshot_detected
        && runtime.startup_snapshot_validated
    {
        (
            "high".to_string(),
            "startup snapshot validation and self-audit signals are both healthy".to_string(),
        )
    } else {
        (
            "medium".to_string(),
            "runtime is healthy but snapshot-assisted startup evidence is incomplete".to_string(),
        )
    };

    RuntimeSurfaceRollup {
        startup_path: startup.path,
        startup_bootstrap_mode: startup.bootstrap_mode,
        startup_status_summary: startup.status_summary,
        startup_fastboot_used: startup.fastboot_used,
        startup_snapshot_detected: startup.snapshot_detected,
        startup_snapshot_validated: startup.snapshot_validated,
        startup_delta_applied: startup.delta_applied,
        startup_replay_required: startup.replay_required,
        startup_fallback_reason: startup.fallback_reason,
        sync_surface_health,
        sync_counters_coherent,
        tx_propagation_health: tx_propagation_health.to_string(),
        tx_inbound_counters_coherent: tx_inbound_counter_delta == 0,
        tx_drop_reason_counters_coherent: tx_drop_reason_counter_delta == 0,
        tx_rebroadcast_outcomes_coherent: tx_rebroadcast_outcome_counter_delta == 0,
        external_mining_surface_health: external_mining_surface_health.to_string(),
        external_mining_template_health: external_mining_template_health.to_string(),
        external_mining_submit_outcome_counters_coherent:
            external_mining_submit_outcome_counter_delta == 0,
        external_mining_rejection_counters_coherent: external_mining_rejection_counter_delta == 0,
        node_runtime_surface_health: node_runtime_surface_health.to_string(),
        runtime_alert_classes,
        incident_primary_surface,
        incident_summary,
        incident_indicators,
        node_health_slo_bps,
        sync_health_slo_bps,
        p2p_health_slo_bps,
        mempool_health_slo_bps,
        mining_health_slo_bps,
        runtime_health_slo_bps,
        recovery_confidence,
        recovery_confidence_reason,
        recovery_confidence_non_misleading,
    }
}

pub(crate) fn runtime_incident_snapshot(
    rollup: &RuntimeSurfaceRollup,
    warn_or_error_count: usize,
    event_count: usize,
) -> RuntimeIncidentSnapshot {
    let status = if rollup.node_runtime_surface_health == "degraded" || warn_or_error_count > 0 {
        "active"
    } else if event_count > 0 {
        "monitoring"
    } else {
        "quiet"
    };
    let mut indicators = rollup.incident_indicators.clone();
    indicators.truncate(5);
    RuntimeIncidentSnapshot {
        primary_surface: rollup.incident_primary_surface.clone(),
        status: status.to_string(),
        summary: format!(
            "{} status={} events={} warn_or_error={} runtime_health_slo_bps={}",
            rollup.incident_summary,
            status,
            event_count,
            warn_or_error_count,
            rollup.runtime_health_slo_bps
        ),
        indicators,
        alert_class_count: rollup.runtime_alert_classes.len(),
        runtime_health_slo_bps: rollup.runtime_health_slo_bps,
    }
}

pub(crate) fn build_runtime_trend_windows(
    events: &[pulsedag_storage::RuntimeEvent],
    rollup: &RuntimeSurfaceRollup,
    now_unix: u64,
) -> Vec<RuntimeTrendWindow> {
    let anchor_unix = events
        .iter()
        .map(|event| event.timestamp_unix)
        .max()
        .unwrap_or(now_unix)
        .max(now_unix);
    [
        ("last_5m", 300u64),
        ("last_30m", 1_800u64),
        ("last_2h", 7_200u64),
    ]
    .into_iter()
    .map(|(label, span_secs)| {
        let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
        let mut warn_or_error_count = 0usize;
        let mut event_count = 0usize;
        for event in events
            .iter()
            .filter(|event| anchor_unix.saturating_sub(event.timestamp_unix) <= span_secs)
        {
            event_count += 1;
            if matches!(event.level.as_str(), "warn" | "error") {
                warn_or_error_count += 1;
            }
            *by_kind.entry(event.kind.clone()).or_insert(0) += 1;
        }
        let dominant_kind = by_kind
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(kind, _)| kind);
        RuntimeTrendWindow {
            label: label.to_string(),
            span_secs,
            event_count,
            warn_or_error_count,
            dominant_kind,
            incident_snapshot: runtime_incident_snapshot(rollup, warn_or_error_count, event_count),
        }
    })
    .collect()
}

fn startup_status_view(runtime: &crate::api::NodeRuntimeStats) -> StartupStatusView {
    let path = runtime.startup_path.clone();
    let bootstrap_mode = match path.as_str() {
        "fast_boot" => "snapshot_assisted".to_string(),
        "fallback_full_replay" => "recovery_fallback".to_string(),
        "full_replay" => "replay".to_string(),
        _ => "normal".to_string(),
    };
    let fastboot_used = path == "fast_boot";
    let snapshot_detected = runtime.startup_snapshot_detected || runtime.startup_snapshot_exists;
    let snapshot_validated = fastboot_used;
    let delta_applied = fastboot_used;
    let replay_required = !fastboot_used;
    let fallback_reason = if path == "fallback_full_replay" {
        runtime.startup_fallback_reason.clone().or_else(|| {
            Some(
                "fallback replay reported without explicit reason; inspect startup logs"
                    .to_string(),
            )
        })
    } else {
        None
    };
    let status_summary = if let Some(reason) = fallback_reason.as_ref() {
        format!(
            "{} startup via {}; fallback_reason={}",
            bootstrap_mode, path, reason
        )
    } else {
        format!("{} startup via {}", bootstrap_mode, path)
    };
    StartupStatusView {
        path,
        bootstrap_mode,
        status_summary,
        fastboot_used,
        snapshot_detected,
        snapshot_validated,
        delta_applied,
        replay_required,
        fallback_reason,
    }
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
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let rollup = runtime_surface_rollup(&runtime);
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let mempool_transactions = chain.mempool.transactions.len();
    let mempool_max_transactions = chain.mempool.max_transactions;
    let mempool_orphan_transactions = chain.mempool.orphan_transactions.len();
    let mempool_max_orphans = chain.mempool.max_orphans;
    let mempool_pending_transactions =
        mempool_transactions.saturating_add(mempool_orphan_transactions);
    let mempool_capacity_remaining_transactions =
        mempool_max_transactions.saturating_sub(mempool_transactions);
    let mempool_pressure_bps_value =
        mempool_pressure_bps(mempool_transactions, mempool_max_transactions);
    let mempool_orphan_pressure_bps_value =
        mempool_pressure_bps(mempool_orphan_transactions, mempool_max_orphans);
    let mempool_pressure_tier = pressure_tier_from_bps(mempool_pressure_bps_value);
    let mempool_orphan_pressure_tier = pressure_tier_from_bps(mempool_orphan_pressure_bps_value);
    let mempool_combined_pressure_tier = combined_pressure_tier(
        mempool_pressure_bps_value,
        mempool_orphan_pressure_bps_value,
    );
    let mempool_backpressure_active = mempool_pressure_bps_value >= 8_000
        || mempool_orphan_pressure_bps_value >= 8_000
        || mempool_capacity_remaining_transactions == 0;
    let mempool_backpressure_signal = if mempool_capacity_remaining_transactions == 0 {
        "at_capacity"
    } else if mempool_orphan_pressure_bps_value >= 9_500 {
        "orphan_saturated"
    } else if mempool_pressure_bps_value >= 9_500 {
        "mempool_saturated"
    } else if mempool_orphan_pressure_bps_value >= 8_000 {
        "orphan_high_pressure"
    } else if mempool_pressure_bps_value >= 8_000 {
        "mempool_high_pressure"
    } else {
        "none"
    };
    let p2p_recovery = state
        .p2p()
        .and_then(|p2p| p2p.status().ok())
        .map(|status| {
            let p2p_last_peer_seen_unix = status
                .peer_recovery
                .iter()
                .filter_map(|peer| peer.last_seen_unix)
                .max();
            let p2p_peers_with_recent_failures = status
                .peer_recovery
                .iter()
                .filter(|peer| !peer.recent_failures_unix.is_empty())
                .count();
            RuntimeP2pRecoverySummary {
                reconnect_attempts: status.peer_reconnect_attempts,
                recovery_success_count: status.peer_recovery_success_count,
                last_recovery_unix: status.last_peer_recovery_unix,
                cooldown_suppressed_count: status.peer_cooldown_suppressed_count,
                flap_suppressed_count: status.peer_flap_suppressed_count,
                peers_under_cooldown: status.peers_under_cooldown,
                peers_under_flap_guard: status.peers_under_flap_guard,
                last_peer_seen_unix: p2p_last_peer_seen_unix,
                peers_with_recent_failures: p2p_peers_with_recent_failures,
                connected_peers_are_real_network: mode_connected_peers_are_real_network(
                    &status.mode,
                ),
                peer_health_healthy: status.peer_lifecycle_healthy,
                peer_health_degraded: status.peer_lifecycle_degraded,
                peer_health_recovering: status.peer_lifecycle_recovering,
                peer_lifecycle_watch: status.peer_lifecycle_watch,
                peer_lifecycle_cooldown: status.peer_lifecycle_cooldown,
                degraded_mode: status.degraded_mode,
                connection_shaping_active: status.connection_shaping_active,
                tx_outbound_duplicates_suppressed: status.tx_outbound_duplicates_suppressed,
                tx_outbound_first_seen_relayed: status.tx_outbound_first_seen_relayed,
                tx_outbound_recovery_relayed: status.tx_outbound_recovery_relayed,
                tx_outbound_priority_relayed: status.tx_outbound_priority_relayed,
                tx_outbound_budget_suppressed: status.tx_outbound_budget_suppressed,
                block_outbound_duplicates_suppressed: status.block_outbound_duplicates_suppressed,
                block_outbound_first_seen_relayed: status.block_outbound_first_seen_relayed,
                block_outbound_recovery_relayed: status.block_outbound_recovery_relayed,
                inbound_duplicates_suppressed: status.inbound_duplicates_suppressed,
                queued_block_messages: status.queued_block_messages,
                queued_non_block_messages: status.queued_non_block_messages,
                queue_max_depth: status.queue_max_depth,
                dequeued_block_messages: status.dequeued_block_messages,
                dequeued_non_block_messages: status.dequeued_non_block_messages,
                queue_block_priority_picks: status.queue_block_priority_picks,
                queue_non_block_fair_picks: status.queue_non_block_fair_picks,
                queue_starvation_relief_picks: status.queue_starvation_relief_picks,
            }
        })
        .unwrap_or_default();
    let external_mining_submit_total = runtime
        .external_mining_submit_accepted
        .saturating_add(runtime.external_mining_submit_rejected);
    let external_mining_submit_outcome_total = runtime
        .accepted_mined_blocks
        .saturating_add(runtime.rejected_mined_blocks);
    let external_mining_submit_outcome_counter_delta = i64::try_from(external_mining_submit_total)
        .unwrap_or(i64::MAX)
        - i64::try_from(external_mining_submit_outcome_total).unwrap_or(i64::MAX);
    let external_mining_rejection_reason_total = runtime
        .external_mining_rejected_invalid_pow
        .saturating_add(runtime.external_mining_rejected_stale_template)
        .saturating_add(runtime.external_mining_rejected_unknown_template)
        .saturating_add(runtime.external_mining_rejected_submit_block_error)
        .saturating_add(runtime.external_mining_rejected_duplicate_block)
        .saturating_add(runtime.external_mining_rejected_invalid_block)
        .saturating_add(runtime.external_mining_rejected_chain_id_mismatch)
        .saturating_add(runtime.external_mining_rejected_internal_error)
        .saturating_add(runtime.external_mining_rejected_storage_error);
    let external_mining_rejection_counter_delta =
        i64::try_from(runtime.external_mining_submit_rejected).unwrap_or(i64::MAX)
            - i64::try_from(external_mining_rejection_reason_total).unwrap_or(i64::MAX);
    let external_mining_stale_work_submit_rejections =
        runtime.external_mining_rejected_stale_template;
    let external_mining_stale_work_template_invalidations = runtime
        .external_mining_stale_work_detected
        .saturating_sub(external_mining_stale_work_submit_rejections);
    let external_mining_template_stale_submit_ratio_bps = if external_mining_submit_total == 0 {
        0
    } else {
        external_mining_stale_work_submit_rejections
            .saturating_mul(10_000)
            .saturating_div(external_mining_submit_total)
            .min(10_000)
    };
    let external_mining_template_health = if external_mining_submit_outcome_counter_delta != 0
        || external_mining_rejection_counter_delta != 0
    {
        "counter_mismatch"
    } else if external_mining_submit_total == 0 {
        "idle"
    } else if external_mining_template_stale_submit_ratio_bps >= 5_000 {
        "stale_dominant"
    } else if external_mining_stale_work_submit_rejections > 0 {
        "watch"
    } else {
        "healthy"
    };
    let external_mining_hashrate_hps = if uptime_secs == 0 {
        0
    } else {
        runtime
            .accepted_mined_blocks
            .saturating_div(uptime_secs)
    };
    let external_mining_worker_efficiency_bps = if external_mining_submit_total == 0 {
        0
    } else {
        runtime
            .external_mining_submit_accepted
            .saturating_mul(10_000)
            .saturating_div(external_mining_submit_total)
            .min(10_000)
    };
    let external_mining_stale_efficiency_bps =
        if runtime.external_mining_stale_work_detected == 0 {
            10_000
        } else {
            external_mining_stale_work_template_invalidations
                .saturating_mul(10_000)
                .saturating_div(runtime.external_mining_stale_work_detected)
                .min(10_000)
        };
    let external_mining_template_usefulness_bps = if runtime.external_mining_templates_emitted == 0 {
        0
    } else {
        runtime
            .external_mining_submit_accepted
            .saturating_mul(10_000)
            .saturating_div(runtime.external_mining_templates_emitted)
            .min(10_000)
    };
    let external_mining_template_rollup = format!(
        "template_health={} stale_submit_rejections={} stale_template_invalidations={} stale_submit_ratio_bps={} worker_efficiency_bps={} stale_efficiency_bps={} template_usefulness_bps={} hashrate_hps={} submit_total={}",
        external_mining_template_health,
        external_mining_stale_work_submit_rejections,
        external_mining_stale_work_template_invalidations,
        external_mining_template_stale_submit_ratio_bps,
        external_mining_worker_efficiency_bps,
        external_mining_stale_efficiency_bps,
        external_mining_template_usefulness_bps,
        external_mining_hashrate_hps,
        external_mining_submit_total
    );
    let external_mining_surface_health = if external_mining_submit_outcome_counter_delta != 0
        || external_mining_rejection_counter_delta != 0
    {
        "counter_mismatch"
    } else if runtime.external_mining_submit_rejected > 0 {
        "degraded"
    } else {
        "healthy"
    };

    let p2p_tx_relay_total_events = p2p_recovery
        .tx_outbound_duplicates_suppressed
        .saturating_add(p2p_recovery.tx_outbound_first_seen_relayed)
        .saturating_add(p2p_recovery.tx_outbound_recovery_relayed)
        .saturating_add(p2p_recovery.tx_outbound_priority_relayed);
    let p2p_tx_relay_duplicate_ratio_bps = if p2p_tx_relay_total_events == 0 {
        0
    } else {
        (p2p_recovery.tx_outbound_duplicates_suppressed as u64)
            .saturating_mul(10_000)
            .saturating_div(p2p_tx_relay_total_events as u64)
            .min(10_000)
    };
    let p2p_tx_relay_budget_suppression_ratio_bps = if p2p_tx_relay_total_events == 0 {
        0
    } else {
        (p2p_recovery.tx_outbound_budget_suppressed as u64)
            .saturating_mul(10_000)
            .saturating_div(p2p_tx_relay_total_events as u64)
            .min(10_000)
    };
    let p2p_block_relay_total_events = p2p_recovery
        .block_outbound_duplicates_suppressed
        .saturating_add(p2p_recovery.block_outbound_first_seen_relayed)
        .saturating_add(p2p_recovery.block_outbound_recovery_relayed);
    let p2p_block_relay_duplicate_ratio_bps = if p2p_block_relay_total_events == 0 {
        0
    } else {
        (p2p_recovery.block_outbound_duplicates_suppressed as u64)
            .saturating_mul(10_000)
            .saturating_div(p2p_block_relay_total_events as u64)
            .min(10_000)
    };
    let p2p_peer_health_total = p2p_recovery
        .peer_health_healthy
        .saturating_add(p2p_recovery.peer_health_degraded)
        .saturating_add(p2p_recovery.peer_health_recovering);
    let p2p_peer_health_counters_coherent = p2p_peer_health_total
        >= p2p_recovery.peers_under_cooldown
        && p2p_peer_health_total >= p2p_recovery.peers_under_flap_guard;
    let tx_inbound_outcome_total = runtime
        .tx_inbound_accepted_total
        .saturating_add(runtime.tx_inbound_dropped_total);
    let tx_inbound_counter_delta = i64::try_from(runtime.tx_inbound_total).unwrap_or(i64::MAX)
        - i64::try_from(tx_inbound_outcome_total).unwrap_or(i64::MAX);
    let tx_drop_reason_total = runtime
        .dropped_p2p_txs_duplicate_mempool
        .saturating_add(runtime.dropped_p2p_txs_duplicate_confirmed)
        .saturating_add(runtime.dropped_p2p_txs_accept_failed)
        .saturating_add(runtime.dropped_p2p_txs_persist_failed);
    let tx_drop_reason_counter_delta = i64::try_from(runtime.dropped_p2p_txs).unwrap_or(i64::MAX)
        - i64::try_from(tx_drop_reason_total).unwrap_or(i64::MAX);
    let tx_rebroadcast_outcome_total = runtime
        .tx_rebroadcast_success
        .saturating_add(runtime.tx_rebroadcast_failed);
    let tx_rebroadcast_outcome_counter_delta = i64::try_from(runtime.tx_rebroadcast_attempts)
        .unwrap_or(i64::MAX)
        - i64::try_from(tx_rebroadcast_outcome_total).unwrap_or(i64::MAX);
    let sync_blocks_request_backlog = runtime
        .sync_pipeline
        .counters
        .blocks_requested
        .saturating_sub(runtime.sync_pipeline.counters.blocks_acquired);
    let sync_blocks_validation_backlog = runtime
        .sync_pipeline
        .counters
        .blocks_acquired
        .saturating_sub(runtime.sync_pipeline.counters.blocks_applied);
    let sync_catchup = sync_catchup_view(&runtime, now);
    let sync_counters_coherent = rollup.sync_counters_coherent;
    let mempool_surface_health = mempool_combined_pressure_tier.as_str();
    let p2p_surface_health = if !p2p_peer_health_counters_coherent {
        "counter_mismatch"
    } else if p2p_recovery.peers_with_recent_failures > 0 || p2p_recovery.peer_health_recovering > 0
    {
        "degraded"
    } else {
        "healthy"
    };
    Json(ApiResponse::ok(RuntimeStatusData {
        started_at_unix: runtime.started_at_unix,
        uptime_secs,
        burn_in_target_days,
        burn_in_elapsed_days,
        burn_in_remaining_days,
        node_runtime_surface_health: rollup.node_runtime_surface_health.clone(),
        runtime_alert_classes: rollup.runtime_alert_classes.clone(),
        incident_primary_surface: rollup.incident_primary_surface.clone(),
        incident_summary: rollup.incident_summary.clone(),
        incident_indicators: rollup.incident_indicators.clone(),
        incident_snapshot: runtime_incident_snapshot(&rollup, 0, 0),
        node_health_slo_bps: rollup.node_health_slo_bps,
        sync_health_slo_bps: rollup.sync_health_slo_bps,
        p2p_health_slo_bps: rollup.p2p_health_slo_bps,
        mempool_health_slo_bps: rollup.mempool_health_slo_bps,
        mining_health_slo_bps: rollup.mining_health_slo_bps,
        runtime_health_slo_bps: rollup.runtime_health_slo_bps,
        accepted_p2p_blocks: runtime.accepted_p2p_blocks,
        rejected_p2p_blocks: runtime.rejected_p2p_blocks,
        duplicate_p2p_blocks: runtime.duplicate_p2p_blocks,
        queued_orphan_blocks: runtime.queued_orphan_blocks,
        adopted_orphan_blocks: runtime.adopted_orphan_blocks,
        accepted_p2p_txs: runtime.accepted_p2p_txs,
        rejected_p2p_txs: runtime.rejected_p2p_txs,
        duplicate_p2p_txs: runtime.duplicate_p2p_txs,
        dropped_p2p_txs: runtime.dropped_p2p_txs,
        dropped_p2p_txs_duplicate_mempool: runtime.dropped_p2p_txs_duplicate_mempool,
        dropped_p2p_txs_duplicate_confirmed: runtime.dropped_p2p_txs_duplicate_confirmed,
        dropped_p2p_txs_accept_failed: runtime.dropped_p2p_txs_accept_failed,
        dropped_p2p_txs_persist_failed: runtime.dropped_p2p_txs_persist_failed,
        tx_rebroadcast_attempts: runtime.tx_rebroadcast_attempts,
        tx_rebroadcast_success: runtime.tx_rebroadcast_success,
        tx_rebroadcast_failed: runtime.tx_rebroadcast_failed,
        tx_rebroadcast_skipped_no_p2p: runtime.tx_rebroadcast_skipped_no_p2p,
        tx_rebroadcast_skipped_no_peers: runtime.tx_rebroadcast_skipped_no_peers,
        last_tx_rebroadcast_unix: runtime.last_tx_rebroadcast_unix,
        last_tx_rebroadcast_error: runtime.last_tx_rebroadcast_error.clone(),
        tx_inbound_counters_coherent: tx_inbound_counter_delta == 0,
        tx_inbound_counter_delta,
        tx_drop_reason_counters_coherent: tx_drop_reason_counter_delta == 0,
        tx_drop_reason_counter_delta,
        tx_rebroadcast_outcomes_coherent: tx_rebroadcast_outcome_counter_delta == 0,
        tx_rebroadcast_outcome_counter_delta,
        tx_propagation_health: rollup.tx_propagation_health.clone(),
        tx_inbound_total: runtime.tx_inbound_total,
        tx_inbound_accepted_total: runtime.tx_inbound_accepted_total,
        tx_inbound_rejected_total: runtime.tx_inbound_rejected_total,
        tx_inbound_dropped_total: runtime.tx_inbound_dropped_total,
        last_tx_accept_unix: runtime.last_tx_accept_unix,
        last_tx_reject_unix: runtime.last_tx_reject_unix,
        last_tx_drop_unix: runtime.last_tx_drop_unix,
        last_tx_drop_reason: runtime.last_tx_drop_reason.clone(),
        last_tx_drop_txid: runtime.last_tx_drop_txid.clone(),
        tx_drop_reasons: runtime.tx_drop_reasons.clone(),
        mempool_transactions,
        mempool_max_transactions,
        mempool_orphan_transactions,
        mempool_max_orphans,
        mempool_pending_transactions,
        mempool_capacity_remaining_transactions,
        mempool_pressure_bps: mempool_pressure_bps_value,
        mempool_orphan_pressure_bps: mempool_orphan_pressure_bps_value,
        mempool_pressure_tier: mempool_pressure_tier.as_str().to_string(),
        mempool_orphan_pressure_tier: mempool_orphan_pressure_tier.as_str().to_string(),
        mempool_backpressure_active,
        mempool_backpressure_signal: mempool_backpressure_signal.to_string(),
        mempool_surface_health: mempool_surface_health.to_string(),
        mempool_admitted_total: chain.mempool.counters.accepted_total,
        mempool_rejected_total: chain.mempool.counters.rejected_total,
        mempool_rejected_low_priority_total: chain.mempool.counters.rejected_low_priority_total,
        mempool_evicted_total: chain.mempool.counters.evicted_total,
        mempool_pressure_events_total: chain.mempool.counters.pressure_events_total,
        mempool_reconcile_runs_total: chain.mempool.counters.reconcile_runs_total,
        mempool_reconcile_removed_total: chain.mempool.counters.reconcile_removed_total,
        mempool_orphaned_total: chain.mempool.counters.orphaned_total,
        mempool_orphan_promoted_total: chain.mempool.counters.orphan_promoted_total,
        mempool_orphan_dropped_total: chain.mempool.counters.orphan_dropped_total,
        mempool_orphan_pruned_total: chain.mempool.counters.orphan_pruned_total,
        accepted_mined_blocks: runtime.accepted_mined_blocks,
        rejected_mined_blocks: runtime.rejected_mined_blocks,
        external_mining_templates_emitted: runtime.external_mining_templates_emitted,
        external_mining_templates_invalidated: runtime.external_mining_templates_invalidated,
        external_mining_stale_work_detected: runtime.external_mining_stale_work_detected,
        external_mining_submit_accepted: runtime.external_mining_submit_accepted,
        external_mining_submit_rejected: runtime.external_mining_submit_rejected,
        external_mining_rejected_invalid_pow: runtime.external_mining_rejected_invalid_pow,
        external_mining_rejected_stale_template: runtime.external_mining_rejected_stale_template,
        external_mining_rejected_unknown_template: runtime
            .external_mining_rejected_unknown_template,
        external_mining_rejected_submit_block_error: runtime
            .external_mining_rejected_submit_block_error,
        external_mining_rejected_duplicate_block: runtime.external_mining_rejected_duplicate_block,
        external_mining_rejected_invalid_block: runtime.external_mining_rejected_invalid_block,
        external_mining_rejected_chain_id_mismatch: runtime
            .external_mining_rejected_chain_id_mismatch,
        external_mining_rejected_internal_error: runtime.external_mining_rejected_internal_error,
        external_mining_rejected_storage_error: runtime.external_mining_rejected_storage_error,
        external_mining_last_template_id: runtime.external_mining_last_template_id.clone(),
        external_mining_last_rejection_kind: runtime.external_mining_last_rejection_kind.clone(),
        external_mining_last_rejection_reason: runtime
            .external_mining_last_rejection_reason
            .clone(),
        external_mining_last_invalid_pow_reason: runtime
            .external_mining_last_invalid_pow_reason
            .clone(),
        external_mining_submit_total,
        external_mining_submit_outcome_total,
        external_mining_submit_outcome_counters_coherent:
            external_mining_submit_outcome_counter_delta == 0,
        external_mining_submit_outcome_counter_delta,
        external_mining_rejection_reason_total,
        external_mining_rejection_counters_coherent: external_mining_rejection_counter_delta == 0,
        external_mining_rejection_counter_delta,
        external_mining_stale_work_submit_rejections,
        external_mining_stale_work_template_invalidations,
        external_mining_template_health: external_mining_template_health.to_string(),
        external_mining_template_stale_submit_ratio_bps,
        external_mining_hashrate_hps,
        external_mining_worker_efficiency_bps,
        external_mining_stale_efficiency_bps,
        external_mining_template_usefulness_bps,
        external_mining_template_rollup,
        external_mining_surface_health: external_mining_surface_health.to_string(),
        startup_snapshot_exists: runtime.startup_snapshot_exists,
        startup_persisted_block_count: runtime.startup_persisted_block_count,
        startup_persisted_max_height: runtime.startup_persisted_max_height,
        startup_consistency_issue_count: runtime.startup_consistency_issue_count,
        startup_recovery_mode: runtime.startup_recovery_mode.clone(),
        startup_rebuild_reason: runtime.startup_rebuild_reason.clone(),
        startup_path: rollup.startup_path.clone(),
        startup_bootstrap_mode: rollup.startup_bootstrap_mode.clone(),
        startup_status_summary: rollup.startup_status_summary.clone(),
        startup_fastboot_used: rollup.startup_fastboot_used,
        startup_snapshot_detected: rollup.startup_snapshot_detected,
        startup_snapshot_validated: rollup.startup_snapshot_validated,
        startup_delta_applied: rollup.startup_delta_applied,
        startup_replay_required: rollup.startup_replay_required,
        startup_fallback_reason: rollup.startup_fallback_reason.clone(),
        startup_duration_ms: runtime.startup_duration_ms,
        last_self_audit_unix: runtime.last_self_audit_unix,
        last_self_audit_ok: runtime.last_self_audit_ok,
        last_self_audit_issue_count: runtime.last_self_audit_issue_count,
        last_self_audit_message: runtime.last_self_audit_message.clone(),
        recovery_confidence: rollup.recovery_confidence.clone(),
        recovery_confidence_reason: rollup.recovery_confidence_reason.clone(),
        recovery_confidence_non_misleading: rollup.recovery_confidence_non_misleading,
        last_observed_best_height: runtime.last_observed_best_height,
        last_height_change_unix: runtime.last_height_change_unix,
        active_alerts: runtime.active_alerts.clone(),
        snapshot_auto_every_blocks: runtime.snapshot_auto_every_blocks,
        auto_prune_enabled: runtime.auto_prune_enabled,
        auto_prune_every_blocks: runtime.auto_prune_every_blocks,
        prune_keep_recent_blocks: runtime.prune_keep_recent_blocks,
        prune_require_snapshot: runtime.prune_require_snapshot,
        last_snapshot_height: runtime.last_snapshot_height,
        last_snapshot_unix: runtime.last_snapshot_unix,
        last_prune_height: runtime.last_prune_height,
        last_prune_unix: runtime.last_prune_unix,
        sync_phase: runtime.sync_pipeline.phase,
        sync_surface_health: rollup.sync_surface_health.clone(),
        sync_counters_coherent,
        sync_last_transition_unix: runtime.sync_pipeline.last_transition_unix,
        sync_completed_cycles: runtime.sync_pipeline.completed_cycles,
        sync_restart_count: runtime.sync_pipeline.restart_count,
        sync_last_error: runtime.sync_pipeline.last_error.clone(),
        sync_selected_peer: runtime.sync_pipeline.selected_peer.clone(),
        sync_selection_version: runtime.sync_pipeline.selection_version,
        sync_fallback_count: runtime.sync_pipeline.fallback_count,
        sync_timeout_fallback_count: runtime.sync_pipeline.timeout_fallback_count,
        sync_last_fallback_reason: runtime.sync_pipeline.last_fallback_reason.clone(),
        sync_last_fallback_peer: runtime.sync_pipeline.last_fallback_peer.clone(),
        sync_counters: runtime.sync_pipeline.counters.clone(),
        sync_blocks_request_backlog,
        sync_blocks_validation_backlog,
        sync_catchup_stage: sync_catchup.stage,
        sync_lag_blocks: sync_catchup.lag_blocks,
        sync_lag_band: sync_catchup.lag_band,
        sync_catchup_progress_bps: sync_catchup.progress_bps,
        sync_catchup_summary: sync_catchup.summary,
        sync_recovery_reason: sync_catchup.recovery_reason,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        retarget_min_bps: snapshot.retarget_min_bps,
        retarget_max_bps: snapshot.retarget_max_bps,
        retarget_was_clamped: snapshot.retarget_was_clamped,
        retarget_rationale: snapshot.retarget_rationale,
        retarget_signal_quality: snapshot.retarget_signal_quality,
        suggested_difficulty: snapshot.suggested_difficulty,
        p2p_peer_reconnect_attempts: p2p_recovery.reconnect_attempts,
        p2p_peer_recovery_success_count: p2p_recovery.recovery_success_count,
        p2p_last_peer_recovery_unix: p2p_recovery.last_recovery_unix,
        p2p_peer_cooldown_suppressed_count: p2p_recovery.cooldown_suppressed_count,
        p2p_peer_flap_suppressed_count: p2p_recovery.flap_suppressed_count,
        p2p_peers_under_cooldown: p2p_recovery.peers_under_cooldown,
        p2p_peers_under_flap_guard: p2p_recovery.peers_under_flap_guard,
        p2p_peer_lifecycle_watch: p2p_recovery.peer_lifecycle_watch,
        p2p_peer_lifecycle_cooldown: p2p_recovery.peer_lifecycle_cooldown,
        p2p_degraded_mode: p2p_recovery.degraded_mode.clone(),
        p2p_connection_shaping_active: p2p_recovery.connection_shaping_active,
        p2p_last_peer_seen_unix: p2p_recovery.last_peer_seen_unix,
        p2p_peers_with_recent_failures: p2p_recovery.peers_with_recent_failures,
        p2p_connected_peers_are_real_network: p2p_recovery.connected_peers_are_real_network,
        p2p_peer_health_healthy: p2p_recovery.peer_health_healthy,
        p2p_peer_health_degraded: p2p_recovery.peer_health_degraded,
        p2p_peer_health_recovering: p2p_recovery.peer_health_recovering,
        p2p_peer_health_total,
        p2p_peer_health_counters_coherent,
        p2p_surface_health: p2p_surface_health.to_string(),
        p2p_tx_outbound_duplicates_suppressed: p2p_recovery.tx_outbound_duplicates_suppressed,
        p2p_tx_outbound_first_seen_relayed: p2p_recovery.tx_outbound_first_seen_relayed,
        p2p_tx_outbound_recovery_relayed: p2p_recovery.tx_outbound_recovery_relayed,
        p2p_tx_outbound_priority_relayed: p2p_recovery.tx_outbound_priority_relayed,
        p2p_tx_outbound_budget_suppressed: p2p_recovery.tx_outbound_budget_suppressed,
        p2p_tx_relay_total_events,
        p2p_tx_relay_duplicate_ratio_bps,
        p2p_tx_relay_budget_suppression_ratio_bps,
        p2p_block_outbound_duplicates_suppressed: p2p_recovery.block_outbound_duplicates_suppressed,
        p2p_block_outbound_first_seen_relayed: p2p_recovery.block_outbound_first_seen_relayed,
        p2p_block_outbound_recovery_relayed: p2p_recovery.block_outbound_recovery_relayed,
        p2p_block_relay_total_events,
        p2p_block_relay_duplicate_ratio_bps,
        p2p_inbound_duplicates_suppressed: p2p_recovery.inbound_duplicates_suppressed,
        p2p_queued_block_messages: p2p_recovery.queued_block_messages,
        p2p_queued_non_block_messages: p2p_recovery.queued_non_block_messages,
        p2p_queue_max_depth: p2p_recovery.queue_max_depth,
        p2p_dequeued_block_messages: p2p_recovery.dequeued_block_messages,
        p2p_dequeued_non_block_messages: p2p_recovery.dequeued_non_block_messages,
        p2p_queue_block_priority_picks: p2p_recovery.queue_block_priority_picks,
        p2p_queue_non_block_fair_picks: p2p_recovery.queue_non_block_fair_picks,
        p2p_queue_starvation_relief_picks: p2p_recovery.queue_starvation_relief_picks,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        extract::{Query, State},
        Json,
    };
    use pulsedag_core::types::Transaction;
    use pulsedag_core::{ChainState, SyncPhase};
    use pulsedag_p2p::{P2pHandle, P2pStatus, PeerRecoveryStatus, P2P_MODE_LIBP2P_REAL};
    use pulsedag_storage::Storage;
    use tokio::sync::RwLock;

    use crate::api::{NodeRuntimeStats, RpcStateLike};

    use super::{get_runtime_events_summary, get_runtime_status, RuntimeEventsQuery};

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

        fn p2p(&self) -> Option<Arc<dyn pulsedag_p2p::P2pHandle>> {
            self.p2p.clone()
        }

        fn storage(&self) -> Arc<Storage> {
            self.storage.clone()
        }

        fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>> {
            self.runtime.clone()
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
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

    #[tokio::test]
    async fn runtime_status_surfaces_sync_phase_coherently() {
        let path = temp_db_path("runtime-sync-phase");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.sync_pipeline.begin_cycle(100);
        runtime.sync_pipeline.observe_headers(5, 101);
        runtime.sync_pipeline.request_blocks(5, 102);
        runtime.sync_pipeline.acquire_blocks(5);
        runtime.sync_pipeline.validate_and_apply_blocks(2, 103);

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        assert!(resp.ok);
        let data = resp.data.expect("runtime status payload");
        assert_eq!(data.sync_phase, SyncPhase::ValidationApplication);
        assert_eq!(data.sync_surface_health, "active");
        assert!(data.sync_counters_coherent);
        assert_eq!(data.sync_counters.headers_discovered, 5);
        assert_eq!(data.sync_counters.blocks_requested, 5);
        assert_eq!(data.sync_counters.blocks_applied, 2);
        assert_eq!(data.sync_blocks_request_backlog, 0);
        assert_eq!(data.sync_blocks_validation_backlog, 3);
        assert_eq!(data.sync_catchup_stage, "validating");
        assert_eq!(data.sync_lag_blocks, 3);
        assert_eq!(data.sync_lag_band, "catching_up");
        assert_eq!(data.sync_catchup_progress_bps, 4_000);
        assert!(data.sync_catchup_summary.contains("stage=validating"));
        assert!(data.sync_recovery_reason.is_some());
    }

    #[tokio::test]
    async fn runtime_status_bounds_sync_lag_progress_and_stage_deterministically() {
        let path = temp_db_path("runtime-sync-bounds");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.sync_pipeline.phase = SyncPhase::ValidationApplication;
        runtime.sync_pipeline.counters.blocks_requested = 1;
        runtime.sync_pipeline.counters.blocks_acquired = 1;
        runtime.sync_pipeline.counters.blocks_validated = 1;
        runtime.sync_pipeline.counters.blocks_applied = 99;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };
        let Json(resp_a) = get_runtime_status(State(state.clone())).await;
        let Json(resp_b) = get_runtime_status(State(state)).await;
        let data_a = resp_a.data.expect("runtime status payload A");
        let data_b = resp_b.data.expect("runtime status payload B");
        assert_eq!(data_a.sync_catchup_progress_bps, 10_000);
        assert_eq!(data_a.sync_lag_blocks, 0);
        assert_eq!(data_a.sync_lag_band, "aligned");
        assert_eq!(data_a.sync_catchup_summary, data_b.sync_catchup_summary);
    }

    #[tokio::test]
    async fn runtime_status_exposes_explicit_recovery_reason_when_sync_is_degraded_or_stalled() {
        let path = temp_db_path("runtime-sync-recovery-reason");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.sync_pipeline.phase = SyncPhase::BlockAcquisition;
        runtime.sync_pipeline.counters.blocks_requested = 10;
        runtime.sync_pipeline.counters.blocks_acquired = 2;
        runtime.sync_pipeline.counters.blocks_validated = 2;
        runtime.sync_pipeline.counters.blocks_applied = 1;
        runtime.sync_pipeline.last_transition_unix = Some(now.saturating_sub(300));
        runtime.sync_pipeline.last_error = Some("peer timeout".to_string());

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };
        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status payload");
        assert_eq!(data.sync_catchup_stage, "degraded");
        assert_eq!(data.sync_surface_health, "degraded");
        assert_eq!(
            data.sync_recovery_reason.as_deref(),
            Some("sync error: peer timeout")
        );
    }

    #[tokio::test]
    async fn runtime_status_surfaces_p2p_mode_and_peer_health_summary() {
        let path = temp_db_path("runtime-p2p-summary");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let p2p_status = P2pStatus {
            mode: P2P_MODE_LIBP2P_REAL.to_string(),
            peer_id: "self".into(),
            listening: vec![],
            connected_peers: vec!["peer-a".into()],
            topics: vec![],
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
            queue_priority_tx_lane_picks: 0,
            queue_standard_tx_lane_picks: 0,
            queue_non_block_fair_picks: 0,
            queue_starvation_relief_picks: 0,
            inbound_messages: 0,
            runtime_started: true,
            runtime_mode_detail: "swarm-poll-loop-real".into(),
            swarm_events_seen: 0,
            subscriptions_active: 0,
            last_message_kind: None,
            last_swarm_event: None,
            per_topic_publishes: std::collections::HashMap::new(),
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
            peer_reconnect_attempts: 5,
            peer_recovery_success_count: 1,
            last_peer_recovery_unix: Some(now.saturating_sub(3)),
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
            connection_shaping_active: true,
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
                    reconnect_attempts: 0,
                    recovery_success_count: 0,
                    last_recovery_unix: Some(now.saturating_sub(3)),
                    recent_failures_unix: vec![],
                    cooldown_suppressed_count: 0,
                    flap_suppressed_count: 0,
                    flap_events: 0,
                    suppression_until_unix: None,
                },
                PeerRecoveryStatus {
                    peer_id: "recovering".into(),
                    score: 60,
                    fail_streak: 1,
                    lifecycle_tier: "recovering".into(),
                    recovery_tier: "assisted".into(),
                    connected: false,
                    last_seen_unix: Some(now.saturating_sub(100)),
                    last_successful_connect_unix: Some(now.saturating_sub(200)),
                    next_retry_unix: now.saturating_add(50),
                    reconnect_attempts: 4,
                    recovery_success_count: 1,
                    last_recovery_unix: Some(now.saturating_sub(40)),
                    recent_failures_unix: vec![now.saturating_sub(20)],
                    cooldown_suppressed_count: 2,
                    flap_suppressed_count: 1,
                    flap_events: 2,
                    suppression_until_unix: Some(now.saturating_add(10)),
                },
            ],
            sync_candidates: vec![],
            selected_sync_peer: Some("peer-a".into()),
            connection_slot_budget: 8,
            connected_slots_in_use: 1,
            available_connection_slots: 7,
            sync_selection_sticky_until_unix: Some(now.saturating_add(30)),
            topology_bucket_count: 8,
            topology_distinct_buckets: 1,
            topology_dominant_bucket_share_bps: 10_000,
            topology_diversity_score_bps: 625,
        };
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: Some(Arc::new(TestP2pHandle { status: p2p_status })),
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert!(data.p2p_connected_peers_are_real_network);
        assert_eq!(data.p2p_peer_health_healthy, 1);
        assert_eq!(data.p2p_peer_health_degraded, 0);
        assert_eq!(data.p2p_peer_health_recovering, 1);
        assert_eq!(data.p2p_peer_health_total, 2);
        assert!(data.p2p_peer_health_counters_coherent);
        assert_eq!(data.p2p_surface_health, "degraded");
    }

    #[tokio::test]
    async fn runtime_status_surfaces_mempool_pressure_and_relay_visibility_metrics() {
        let path = temp_db_path("runtime-mempool-relay-metrics");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let mut chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        chain.mempool.max_transactions = 4;
        chain.mempool.max_orphans = 3;
        chain.mempool.transactions.insert(
            "tx-1".into(),
            Transaction {
                txid: "tx-1".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 1,
                nonce: 1,
            },
        );
        chain.mempool.transactions.insert(
            "tx-2".into(),
            Transaction {
                txid: "tx-2".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 2,
                nonce: 2,
            },
        );
        chain.mempool.orphan_transactions.insert(
            "orphan-1".into(),
            Transaction {
                txid: "orphan-1".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 0,
                nonce: 3,
            },
        );
        chain.mempool.counters.accepted_total = 7;
        chain.mempool.counters.rejected_total = 3;
        chain.mempool.counters.rejected_low_priority_total = 2;
        chain.mempool.counters.evicted_total = 1;
        chain.mempool.counters.pressure_events_total = 4;
        chain.mempool.counters.reconcile_runs_total = 5;
        chain.mempool.counters.reconcile_removed_total = 2;
        chain.mempool.counters.orphaned_total = 6;
        chain.mempool.counters.orphan_promoted_total = 4;
        chain.mempool.counters.orphan_dropped_total = 1;
        chain.mempool.counters.orphan_pruned_total = 1;

        let p2p_status = P2pStatus {
            mode: P2P_MODE_LIBP2P_REAL.to_string(),
            peer_id: "self".into(),
            listening: vec![],
            connected_peers: vec!["peer-a".into()],
            topics: vec![],
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
            queue_priority_tx_lane_picks: 0,
            queue_standard_tx_lane_picks: 0,
            queue_non_block_fair_picks: 0,
            queue_starvation_relief_picks: 0,
            inbound_messages: 0,
            runtime_started: true,
            runtime_mode_detail: "swarm-poll-loop-real".into(),
            swarm_events_seen: 0,
            subscriptions_active: 0,
            last_message_kind: None,
            last_swarm_event: None,
            per_topic_publishes: std::collections::HashMap::new(),
            inbound_decode_failed: 0,
            inbound_chain_mismatch_dropped: 0,
            inbound_duplicates_suppressed: 2,
            tx_outbound_duplicates_suppressed: 3,
            tx_outbound_first_seen_relayed: 9,
            tx_outbound_recovery_relayed: 2,
            tx_outbound_priority_relayed: 1,
            tx_outbound_budget_suppressed: 3,
            tx_outbound_recovery_budget_suppressed: 0,
            block_outbound_duplicates_suppressed: 2,
            block_outbound_first_seen_relayed: 5,
            block_outbound_recovery_relayed: 1,
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
            connection_shaping_active: true,
            peer_recovery: vec![],
            sync_candidates: vec![],
            selected_sync_peer: None,
            connection_slot_budget: 8,
            connected_slots_in_use: 0,
            available_connection_slots: 8,
            sync_selection_sticky_until_unix: None,
            topology_bucket_count: 8,
            topology_distinct_buckets: 1,
            topology_dominant_bucket_share_bps: 10_000,
            topology_diversity_score_bps: 625,
        };
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: Some(Arc::new(TestP2pHandle { status: p2p_status })),
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.mempool_transactions, 2);
        assert_eq!(data.mempool_orphan_transactions, 1);
        assert_eq!(data.mempool_pending_transactions, 3);
        assert_eq!(data.mempool_capacity_remaining_transactions, 2);
        assert_eq!(data.mempool_pressure_bps, 5_000);
        assert_eq!(data.mempool_orphan_pressure_bps, 3_333);
        assert_eq!(data.mempool_pressure_tier, "normal");
        assert_eq!(data.mempool_orphan_pressure_tier, "normal");
        assert!(!data.mempool_backpressure_active);
        assert_eq!(data.mempool_backpressure_signal, "none");
        assert_eq!(data.mempool_surface_health, "normal");
        assert_eq!(data.mempool_admitted_total, 7);
        assert_eq!(data.mempool_rejected_total, 3);
        assert_eq!(data.mempool_rejected_low_priority_total, 2);
        assert_eq!(data.mempool_evicted_total, 1);
        assert_eq!(data.mempool_pressure_events_total, 4);
        assert_eq!(data.mempool_reconcile_runs_total, 5);
        assert_eq!(data.mempool_reconcile_removed_total, 2);
        assert_eq!(data.mempool_orphaned_total, 6);
        assert_eq!(data.mempool_orphan_promoted_total, 4);
        assert_eq!(data.mempool_orphan_dropped_total, 1);
        assert_eq!(data.mempool_orphan_pruned_total, 1);
        assert_eq!(data.p2p_tx_relay_total_events, 15);
        assert_eq!(data.p2p_tx_relay_duplicate_ratio_bps, 2_000);
        assert_eq!(data.p2p_tx_relay_budget_suppression_ratio_bps, 2_000);
        assert_eq!(data.p2p_block_relay_total_events, 8);
        assert_eq!(data.p2p_block_relay_duplicate_ratio_bps, 2_500);
    }

    #[tokio::test]
    async fn runtime_status_mempool_backpressure_signals_are_explicit_and_stable() {
        let path = temp_db_path("runtime-mempool-backpressure-signals");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let mut chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        chain.mempool.max_transactions = 4;
        chain.mempool.max_orphans = 3;
        for i in 0..4 {
            let txid = format!("tx-{i}");
            chain.mempool.transactions.insert(
                txid.clone(),
                Transaction {
                    txid,
                    version: 1,
                    inputs: vec![],
                    outputs: vec![],
                    fee: i as u64,
                    nonce: i as u64,
                },
            );
        }
        chain.mempool.orphan_transactions.insert(
            "orphan-1".into(),
            Transaction {
                txid: "orphan-1".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 0,
                nonce: 9,
            },
        );
        chain.mempool.orphan_transactions.insert(
            "orphan-2".into(),
            Transaction {
                txid: "orphan-2".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 0,
                nonce: 10,
            },
        );
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: None,
        };

        let Json(resp_a) = get_runtime_status(State(state.clone())).await;
        let a = resp_a.data.expect("runtime status data");
        let Json(resp_b) = get_runtime_status(State(state)).await;
        let b = resp_b.data.expect("runtime status data");

        assert_eq!(a.mempool_pressure_tier, "saturated");
        assert_eq!(a.mempool_orphan_pressure_tier, "elevated");
        assert_eq!(a.mempool_surface_health, "saturated");
        assert!(a.mempool_backpressure_active);
        assert_eq!(a.mempool_backpressure_signal, "at_capacity");
        assert_eq!(a.mempool_backpressure_signal, b.mempool_backpressure_signal);
        assert_eq!(a.mempool_backpressure_active, b.mempool_backpressure_active);
    }

    #[tokio::test]
    async fn runtime_status_tx_propagation_coherence_and_reasons_are_explicit() {
        let path = temp_db_path("runtime-tx-propagation-coherence");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.tx_inbound_total = 8;
        runtime.tx_inbound_accepted_total = 3;
        runtime.tx_inbound_dropped_total = 5;
        runtime.tx_inbound_rejected_total = 2;
        runtime.dropped_p2p_txs = 5;
        runtime.dropped_p2p_txs_duplicate_mempool = 2;
        runtime.dropped_p2p_txs_duplicate_confirmed = 1;
        runtime.dropped_p2p_txs_accept_failed = 1;
        runtime.dropped_p2p_txs_persist_failed = 1;
        runtime.tx_rebroadcast_attempts = 2;
        runtime.tx_rebroadcast_success = 1;
        runtime.tx_rebroadcast_failed = 1;
        runtime.tx_drop_reasons = vec![
            "txid=tx-a reason=duplicate_mempool".to_string(),
            "txid=tx-b reason=accept_failed error=fee too low".to_string(),
            "txid=tx-c reason=persist_failed error=io unavailable".to_string(),
        ];
        runtime.last_tx_drop_reason = Some("persist_failed".to_string());
        runtime.last_tx_drop_txid = Some("tx-c".to_string());
        runtime.last_tx_rebroadcast_error = Some("publish queue backpressure".to_string());

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert!(data.tx_inbound_counters_coherent);
        assert_eq!(data.tx_inbound_counter_delta, 0);
        assert!(data.tx_drop_reason_counters_coherent);
        assert_eq!(data.tx_drop_reason_counter_delta, 0);
        assert!(data.tx_rebroadcast_outcomes_coherent);
        assert_eq!(data.tx_rebroadcast_outcome_counter_delta, 0);
        assert_eq!(data.tx_propagation_health, "degraded");
        assert_eq!(data.node_runtime_surface_health, "healthy");
        assert_eq!(data.last_tx_drop_reason.as_deref(), Some("persist_failed"));
        assert_eq!(data.last_tx_drop_txid.as_deref(), Some("tx-c"));
        assert_eq!(
            data.last_tx_rebroadcast_error.as_deref(),
            Some("publish queue backpressure")
        );
        assert!(data
            .tx_drop_reasons
            .iter()
            .any(|entry| entry.contains("reason=duplicate_mempool")));
        assert!(data
            .tx_drop_reasons
            .iter()
            .any(|entry| entry.contains("reason=accept_failed")));
        assert!(data
            .tx_drop_reasons
            .iter()
            .any(|entry| entry.contains("reason=persist_failed")));
    }

    #[tokio::test]
    async fn runtime_status_surfaces_external_mining_diagnostics_without_regression() {
        let path = temp_db_path("runtime-mining-diagnostics");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.accepted_mined_blocks = 4;
        runtime.rejected_mined_blocks = 3;
        runtime.external_mining_templates_emitted = 7;
        runtime.external_mining_templates_invalidated = 2;
        runtime.external_mining_stale_work_detected = 5;
        runtime.external_mining_submit_accepted = 4;
        runtime.external_mining_submit_rejected = 3;
        runtime.external_mining_rejected_invalid_pow = 2;
        runtime.external_mining_rejected_stale_template = 1;
        runtime.external_mining_rejected_unknown_template = 0;
        runtime.external_mining_rejected_submit_block_error = 0;
        runtime.external_mining_rejected_duplicate_block = 0;
        runtime.external_mining_rejected_invalid_block = 0;
        runtime.external_mining_rejected_chain_id_mismatch = 0;
        runtime.external_mining_rejected_internal_error = 0;
        runtime.external_mining_rejected_storage_error = 0;
        runtime.external_mining_last_template_id = Some("tpl-007".to_string());
        runtime.external_mining_last_rejection_kind = Some("invalid_pow".to_string());
        runtime.external_mining_last_rejection_reason =
            Some("submitted block does not satisfy randomx policy".to_string());
        runtime.external_mining_last_invalid_pow_reason = Some("score=9999 target=100".to_string());

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        assert!(resp.ok);
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.accepted_mined_blocks, 4);
        assert_eq!(data.rejected_mined_blocks, 3);
        assert_eq!(data.external_mining_templates_emitted, 7);
        assert_eq!(data.external_mining_templates_invalidated, 2);
        assert_eq!(data.external_mining_stale_work_detected, 5);
        assert_eq!(data.external_mining_submit_accepted, 4);
        assert_eq!(data.external_mining_submit_rejected, 3);
        assert_eq!(data.external_mining_submit_total, 7);
        assert_eq!(data.external_mining_submit_outcome_total, 7);
        assert!(data.external_mining_submit_outcome_counters_coherent);
        assert_eq!(data.external_mining_submit_outcome_counter_delta, 0);
        assert_eq!(data.external_mining_rejected_invalid_pow, 2);
        assert_eq!(data.external_mining_rejected_stale_template, 1);
        assert_eq!(data.external_mining_rejected_duplicate_block, 0);
        assert_eq!(data.external_mining_rejected_invalid_block, 0);
        assert_eq!(data.external_mining_rejected_chain_id_mismatch, 0);
        assert_eq!(data.external_mining_rejected_internal_error, 0);
        assert_eq!(data.external_mining_rejection_reason_total, 3);
        assert!(data.external_mining_rejection_counters_coherent);
        assert_eq!(data.external_mining_rejection_counter_delta, 0);
        assert_eq!(data.external_mining_stale_work_submit_rejections, 1);
        assert_eq!(data.external_mining_stale_work_template_invalidations, 4);
        assert_eq!(data.external_mining_template_health, "watch");
        assert_eq!(data.external_mining_template_stale_submit_ratio_bps, 1428);
        assert_eq!(data.external_mining_worker_efficiency_bps, 5714);
        assert_eq!(data.external_mining_stale_efficiency_bps, 8000);
        assert_eq!(data.external_mining_template_usefulness_bps, 5714);
        assert!(data
            .external_mining_template_rollup
            .contains("template_health=watch"));
        assert!(data
            .external_mining_template_rollup
            .contains("worker_efficiency_bps=5714"));
        assert_eq!(data.external_mining_surface_health, "degraded");
        assert_eq!(
            data.external_mining_last_template_id.as_deref(),
            Some("tpl-007")
        );
        assert_eq!(
            data.external_mining_last_rejection_kind.as_deref(),
            Some("invalid_pow")
        );
        assert!(data
            .external_mining_last_rejection_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("randomx")));
        assert!(data
            .external_mining_last_invalid_pow_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("score=9999")));
    }

    #[tokio::test]
    async fn runtime_status_flags_external_mining_counter_incoherence() {
        let path = temp_db_path("runtime-mining-counter-incoherence");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");

        let mut runtime = NodeRuntimeStats::default();
        runtime.accepted_mined_blocks = 5;
        runtime.rejected_mined_blocks = 2;
        runtime.external_mining_submit_accepted = 4;
        runtime.external_mining_submit_rejected = 1;
        runtime.external_mining_rejected_invalid_pow = 1;
        runtime.external_mining_rejected_stale_template = 0;
        runtime.external_mining_rejected_unknown_template = 0;
        runtime.external_mining_rejected_submit_block_error = 0;
        runtime.external_mining_rejected_duplicate_block = 0;
        runtime.external_mining_rejected_invalid_block = 0;
        runtime.external_mining_rejected_chain_id_mismatch = 0;
        runtime.external_mining_rejected_internal_error = 0;
        runtime.external_mining_rejected_storage_error = 0;
        runtime.external_mining_stale_work_detected = 0;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.external_mining_submit_total, 5);
        assert_eq!(data.external_mining_submit_outcome_total, 7);
        assert!(!data.external_mining_submit_outcome_counters_coherent);
        assert_eq!(data.external_mining_submit_outcome_counter_delta, -2);
        assert_eq!(data.external_mining_rejection_reason_total, 1);
        assert!(data.external_mining_rejection_counters_coherent);
        assert_eq!(data.external_mining_surface_health, "counter_mismatch");
        assert_eq!(data.external_mining_template_health, "counter_mismatch");
    }

    #[tokio::test]
    async fn runtime_status_external_mining_efficiency_rollups_stay_bounded_and_coherent() {
        let path = temp_db_path("runtime-mining-efficiency-bounds");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");

        let mut runtime = NodeRuntimeStats::default();
        runtime.external_mining_stale_work_detected = 2;
        runtime.external_mining_rejected_stale_template = 5;
        runtime.external_mining_submit_accepted = 0;
        runtime.external_mining_submit_rejected = 5;
        runtime.accepted_mined_blocks = 0;
        runtime.rejected_mined_blocks = 5;
        runtime.external_mining_templates_emitted = 0;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status payload");
        assert_eq!(data.external_mining_worker_efficiency_bps, 0);
        assert_eq!(data.external_mining_stale_efficiency_bps, 0);
        assert_eq!(data.external_mining_template_usefulness_bps, 0);
        assert!(data.external_mining_stale_efficiency_bps <= 10_000);
        assert!(data.external_mining_template_usefulness_bps <= 10_000);
        assert!(data
            .external_mining_template_rollup
            .contains("stale_efficiency_bps=0"));
    }

    #[tokio::test]
    async fn runtime_status_normalizes_contradictory_sync_and_node_signals() {
        let path = temp_db_path("runtime-sync-node-normalization");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::ValidationApplication;
        runtime.sync_pipeline.counters.blocks_requested = 3;
        runtime.sync_pipeline.counters.blocks_acquired = 1;
        runtime.sync_pipeline.counters.blocks_validated = 4;
        runtime.sync_pipeline.counters.blocks_applied = 4;
        runtime.sync_pipeline.last_error = Some("validation mismatch".to_string());
        runtime.active_alerts = vec!["sync stalled".to_string()];
        runtime.last_self_audit_ok = false;
        runtime.last_self_audit_issue_count = 1;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert!(!data.sync_counters_coherent);
        assert_eq!(data.sync_surface_health, "degraded");
        assert_eq!(data.sync_blocks_request_backlog, 2);
        assert_eq!(data.sync_blocks_validation_backlog, 0);
        assert_eq!(data.node_runtime_surface_health, "degraded");
        assert_eq!(data.recovery_confidence, "low");
        assert!(data.recovery_confidence_non_misleading);
    }

    #[tokio::test]
    async fn runtime_status_surfaces_recovery_confidence_coherently() {
        let path = temp_db_path("runtime-recovery-confidence");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.startup_fastboot_used = true;
        runtime.startup_snapshot_detected = true;
        runtime.startup_snapshot_validated = true;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.recovery_confidence, "high");
        assert!(data
            .recovery_confidence_reason
            .contains("snapshot validation"));
        assert!(data.recovery_confidence_non_misleading);
    }

    #[tokio::test]
    async fn runtime_status_alert_classes_remain_coherent_under_mixed_degraded_states() {
        let path = temp_db_path("runtime-alert-classes-mixed-degraded");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = false;
        runtime.last_self_audit_issue_count = 2;
        runtime.sync_pipeline.last_error = Some("peer timeout".to_string());
        runtime.tx_rebroadcast_attempts = 1;
        runtime.tx_rebroadcast_success = 0;
        runtime.external_mining_submit_accepted = 1;
        runtime.external_mining_submit_rejected = 1;
        runtime.external_mining_rejected_stale_template = 1;
        runtime.active_alerts = vec![
            "high mempool size: 999".to_string(),
            "height stagnant for 900 seconds".to_string(),
        ];

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.node_runtime_surface_health, "degraded");
        assert!(data
            .runtime_alert_classes
            .contains(&"node_integrity".to_string()));
        assert!(data
            .runtime_alert_classes
            .contains(&"sync_pipeline".to_string()));
        assert!(data
            .runtime_alert_classes
            .contains(&"mempool_pressure".to_string()));
        assert!(data
            .runtime_alert_classes
            .contains(&"mining_submissions".to_string()));
        assert!(data
            .runtime_alert_classes
            .contains(&"tip_stagnation".to_string()));
        assert_ne!(data.incident_primary_surface, "none");
        assert!(data.incident_summary.contains("runtime_health_slo_bps="));
    }

    #[tokio::test]
    async fn runtime_status_slo_style_rollups_align_with_counter_health() {
        let path = temp_db_path("runtime-slo-rollup-alignment");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.sync_pipeline.phase = SyncPhase::HeaderDiscovery;
        runtime.sync_pipeline.counters.blocks_requested = 2;
        runtime.sync_pipeline.counters.blocks_acquired = 2;
        runtime.sync_pipeline.counters.blocks_validated = 2;
        runtime.sync_pipeline.counters.blocks_applied = 2;
        runtime.tx_inbound_total = 4;
        runtime.tx_inbound_accepted_total = 3;
        runtime.tx_inbound_dropped_total = 1;
        runtime.dropped_p2p_txs = 1;
        runtime.dropped_p2p_txs_duplicate_mempool = 1;
        runtime.tx_rebroadcast_attempts = 1;
        runtime.tx_rebroadcast_success = 1;
        runtime.external_mining_submit_accepted = 2;
        runtime.external_mining_submit_rejected = 0;
        runtime.accepted_mined_blocks = 2;
        runtime.rejected_mined_blocks = 0;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.tx_inbound_counter_delta, 0);
        assert!(data.tx_inbound_counters_coherent);
        assert_eq!(data.sync_surface_health, "active");
        assert_eq!(data.external_mining_surface_health, "healthy");
        assert_eq!(data.sync_health_slo_bps, 10_000);
        assert_eq!(data.mining_health_slo_bps, 10_000);
        assert!(data.runtime_health_slo_bps >= 9_000);
    }

    #[tokio::test]
    async fn runtime_status_keeps_existing_runtime_fields_stable() {
        let path = temp_db_path("runtime-existing-fields-stable");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.accepted_p2p_blocks = 9;
        runtime.rejected_p2p_blocks = 2;
        runtime.duplicate_p2p_blocks = 1;
        runtime.accepted_mined_blocks = 4;
        runtime.rejected_mined_blocks = 1;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) = get_runtime_status(State(state)).await;
        let data = resp.data.expect("runtime status data");
        assert_eq!(data.accepted_p2p_blocks, 9);
        assert_eq!(data.rejected_p2p_blocks, 2);
        assert_eq!(data.duplicate_p2p_blocks, 1);
        assert_eq!(data.accepted_mined_blocks, 4);
        assert_eq!(data.rejected_mined_blocks, 1);
    }

    #[tokio::test]
    async fn runtime_events_summary_includes_runtime_rollup_without_regressing_counts() {
        let path = temp_db_path("runtime-event-summary-rollup");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        storage
            .append_runtime_event("info", "sync_phase_change", "headers discovered")
            .expect("append event");
        storage
            .append_runtime_event("warn", "mining_reject", "invalid_pow")
            .expect("append event");
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.sync_pipeline.last_error = Some("peer timeout".to_string());
        runtime.tx_rebroadcast_attempts = 1;
        runtime.tx_rebroadcast_success = 0;
        runtime.external_mining_submit_accepted = 1;
        runtime.external_mining_submit_rejected = 1;
        runtime.external_mining_rejected_invalid_pow = 1;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) =
            get_runtime_events_summary(State(state), Query(RuntimeEventsQuery { limit: Some(50) }))
                .await;
        let data = resp.data.expect("runtime events summary");
        assert_eq!(data.scanned_event_count, 2);
        assert_eq!(data.by_kind.get("sync_phase_change").copied(), Some(1));
        assert_eq!(data.by_level.get("warn").copied(), Some(1));
        assert_eq!(data.runtime_surface_rollup.sync_surface_health, "degraded");
        assert_eq!(
            data.runtime_surface_rollup.tx_propagation_health,
            "counter_mismatch"
        );
        assert_eq!(
            data.runtime_surface_rollup.external_mining_surface_health,
            "counter_mismatch"
        );
        assert_eq!(
            data.runtime_surface_rollup.external_mining_template_health,
            "counter_mismatch"
        );
        assert_eq!(
            data.incident_snapshot.primary_surface,
            data.runtime_surface_rollup.incident_primary_surface
        );
        assert_eq!(data.trend_windows.len(), 3);
        assert!(data
            .trend_windows
            .iter()
            .all(|window| window.event_count <= data.scanned_event_count));
    }

    #[tokio::test]
    async fn runtime_events_trend_windows_and_incident_snapshots_stay_coherent_and_bounded() {
        let path = temp_db_path("runtime-event-summary-trend-windows");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).expect("storage"));
        storage
            .append_runtime_event("warn", "sync_phase_change", "sync slowed")
            .expect("append event");
        storage
            .append_runtime_event("error", "mempool_pressure", "mempool saturated")
            .expect("append event");
        storage
            .append_runtime_event("info", "snapshot", "snapshot captured")
            .expect("append event");
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .expect("genesis");
        let mut runtime = NodeRuntimeStats::default();
        runtime.last_self_audit_ok = true;
        runtime.sync_pipeline.last_error = Some("peer timeout".to_string());
        runtime.active_alerts = vec!["sync stalled".to_string()];

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: None,
        };

        let Json(resp) =
            get_runtime_events_summary(State(state), Query(RuntimeEventsQuery { limit: Some(50) }))
                .await;
        let data = resp.data.expect("runtime events summary");
        assert_eq!(data.scanned_event_count, 3);
        assert_eq!(data.trend_windows.len(), 3);
        assert!(data
            .trend_windows
            .windows(2)
            .all(|pair| pair[0].event_count <= pair[1].event_count));
        assert!(data
            .trend_windows
            .iter()
            .all(|window| window.event_count <= 3));
        assert!(data
            .trend_windows
            .iter()
            .all(|window| window.warn_or_error_count <= window.event_count));
        assert!(data
            .trend_windows
            .iter()
            .all(|window| window.incident_snapshot.summary.len() < 220));
        assert!(data
            .trend_windows
            .iter()
            .all(|window| window.incident_snapshot.indicators.len() <= 5));
        assert_eq!(
            data.incident_snapshot.primary_surface,
            data.runtime_surface_rollup.incident_primary_surface
        );
        assert_eq!(
            data.incident_snapshot.runtime_health_slo_bps,
            data.runtime_surface_rollup.runtime_health_slo_bps
        );
    }
}

#[derive(Debug, Deserialize)]
pub struct RuntimeEventsQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeEventStreamQuery {
    pub poll_interval_ms: Option<u64>,
    pub scan_limit: Option<usize>,
    pub emit_limit: Option<usize>,
    pub heartbeat_secs: Option<u64>,
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
    pub runtime_surface_rollup: RuntimeSurfaceRollup,
    pub incident_snapshot: RuntimeIncidentSnapshot,
    pub trend_windows: Vec<RuntimeTrendWindow>,
}

pub async fn get_runtime_events_summary<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<RuntimeEventsQuery>,
) -> Json<ApiResponse<RuntimeEventsSummaryData>> {
    let limit = query.limit.unwrap_or(200).min(2000);
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let rollup = runtime_surface_rollup(&runtime);
    match state.storage().list_runtime_events(limit) {
        Ok(events) => {
            let mut by_kind = BTreeMap::new();
            let mut by_level = BTreeMap::new();
            for event in &events {
                *by_kind.entry(event.kind.clone()).or_insert(0) += 1;
                *by_level.entry(event.level.clone()).or_insert(0) += 1;
            }
            let warn_or_error_count = by_level.get("warn").copied().unwrap_or(0)
                + by_level.get("error").copied().unwrap_or(0);
            let now_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let trend_windows = build_runtime_trend_windows(&events, &rollup, now_unix);
            let incident_snapshot =
                runtime_incident_snapshot(&rollup, warn_or_error_count, events.len());
            Json(ApiResponse::ok(RuntimeEventsSummaryData {
                scanned_event_count: events.len(),
                by_kind,
                by_level,
                runtime_surface_rollup: rollup,
                incident_snapshot,
                trend_windows,
            }))
        }
        Err(e) => Json(ApiResponse::err(
            "RUNTIME_EVENTS_SUMMARY_ERROR",
            &e.to_string(),
        )),
    }
}

#[derive(Debug, Serialize)]
struct RuntimeEventStreamEnvelope {
    sequence: u64,
    dropped_count: usize,
    event: pulsedag_storage::RuntimeEvent,
}

#[derive(Debug)]
struct StreamDeduper {
    seen: VecDeque<String>,
    seen_lookup: HashSet<String>,
    cap: usize,
}

impl StreamDeduper {
    fn new(cap: usize) -> Self {
        Self {
            seen: VecDeque::with_capacity(cap),
            seen_lookup: HashSet::with_capacity(cap),
            cap,
        }
    }

    fn unseen<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a pulsedag_storage::RuntimeEvent>,
    ) -> Vec<pulsedag_storage::RuntimeEvent> {
        let mut unseen = Vec::new();
        for event in events {
            let key = format!(
                "{}|{}|{}|{}",
                event.timestamp_unix, event.level, event.kind, event.message
            );
            if self.seen_lookup.contains(&key) {
                continue;
            }
            self.remember(key);
            unseen.push(event.clone());
        }
        unseen
    }

    fn remember(&mut self, key: String) {
        self.seen_lookup.insert(key.clone());
        self.seen.push_back(key);
        if self.seen.len() > self.cap {
            if let Some(old) = self.seen.pop_front() {
                self.seen_lookup.remove(&old);
            }
        }
    }
}

pub async fn get_runtime_events_stream<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<RuntimeEventStreamQuery>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let poll_interval_ms = query.poll_interval_ms.unwrap_or(500).clamp(100, 5_000);
    let scan_limit = query.scan_limit.unwrap_or(200).clamp(20, 1_000);
    let emit_limit = query.emit_limit.unwrap_or(32).clamp(1, 200);
    let heartbeat_secs = query.heartbeat_secs.unwrap_or(15).clamp(5, 60);
    let storage = state.storage();
    let event_stream =
        build_runtime_events_stream(storage, poll_interval_ms, scan_limit, emit_limit);
    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(heartbeat_secs))
            .text("keep-alive"),
    )
}

fn build_runtime_events_stream(
    storage: std::sync::Arc<pulsedag_storage::Storage>,
    poll_interval_ms: u64,
    scan_limit: usize,
    emit_limit: usize,
) -> impl futures_core::Stream<Item = Result<Event, Infallible>> {
    let mut ticker = tokio::time::interval(Duration::from_millis(poll_interval_ms));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    stream! {
        let mut sequence = 0u64;
        let mut deduper = StreamDeduper::new(scan_limit.saturating_mul(4));
        loop {
            ticker.tick().await;
            let events = match storage.list_runtime_events(scan_limit) {
                Ok(events) => events,
                Err(err) => {
                    let payload: ApiResponse<()> = ApiResponse::err(
                        "RUNTIME_EVENTS_STREAM_ERROR",
                        format!("event stream poll failed: {err}"),
                    );
                    if let Ok(data) = serde_json::to_string(&payload) {
                        yield Ok(Event::default().event("error").data(data));
                    }
                    continue;
                }
            };
            let unseen = deduper.unseen(events.iter());
            if unseen.is_empty() {
                continue;
            }
            let dropped_count = unseen.len().saturating_sub(emit_limit);
            for event in unseen.into_iter().skip(dropped_count) {
                sequence = sequence.saturating_add(1);
                let envelope = RuntimeEventStreamEnvelope {
                    sequence,
                    dropped_count,
                    event,
                };
                if let Ok(data) = serde_json::to_string(&envelope) {
                    yield Ok(Event::default().event("runtime_event").data(data));
                }
            }
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use std::{sync::Arc, time::Duration};

    use super::{build_runtime_events_stream, RuntimeEventStreamEnvelope, StreamDeduper};
    use pulsedag_storage::RuntimeEvent;
    use pulsedag_storage::Storage;

    fn runtime_event(ts: u64, kind: &str, message: &str) -> RuntimeEvent {
        RuntimeEvent {
            timestamp_unix: ts,
            level: "info".into(),
            kind: kind.into(),
            message: message.into(),
        }
    }

    #[test]
    fn stream_deduper_emits_first_seen_events() {
        let events = vec![
            runtime_event(10, "sync_phase_change", "headers"),
            runtime_event(11, "p2p_reconnect", "peer-a"),
        ];
        let mut deduper = StreamDeduper::new(32);
        let unseen = deduper.unseen(events.iter());
        assert_eq!(unseen.len(), 2);
        assert_eq!(unseen[0].kind, "sync_phase_change");
    }

    #[test]
    fn stream_deduper_suppresses_replays_between_polls() {
        let events = vec![runtime_event(10, "sync_phase_change", "headers")];
        let mut deduper = StreamDeduper::new(32);
        assert_eq!(deduper.unseen(events.iter()).len(), 1);
        assert!(deduper.unseen(events.iter()).is_empty());
    }

    #[test]
    fn stream_backpressure_policy_drops_oldest_in_batch() {
        let events = vec![
            runtime_event(1, "a", "one"),
            runtime_event(2, "b", "two"),
            runtime_event(3, "c", "three"),
        ];
        let mut deduper = StreamDeduper::new(32);
        let unseen = deduper.unseen(events.iter());
        let emit_limit = 2usize;
        let dropped_count = unseen.len().saturating_sub(emit_limit);
        let emitted: Vec<RuntimeEventStreamEnvelope> = unseen
            .into_iter()
            .skip(dropped_count)
            .enumerate()
            .map(|(i, event)| RuntimeEventStreamEnvelope {
                sequence: (i + 1) as u64,
                dropped_count,
                event,
            })
            .collect();
        assert_eq!(dropped_count, 1);
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].event.kind, "b");
        assert_eq!(emitted[1].event.kind, "c");
    }

    #[test]
    fn stream_ordering_is_monotonic_for_emitted_events() {
        let events = vec![
            runtime_event(2, "sync_phase_change", "header-sync"),
            runtime_event(3, "snapshot", "rebuild-start"),
            runtime_event(4, "snapshot", "rebuild-done"),
        ];
        let mut deduper = StreamDeduper::new(32);
        let unseen = deduper.unseen(events.iter());
        let emitted_kinds: Vec<String> = unseen.into_iter().map(|event| event.kind).collect();
        assert_eq!(
            emitted_kinds,
            vec![
                "sync_phase_change".to_string(),
                "snapshot".to_string(),
                "snapshot".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn stream_client_disconnect_drop_is_safe() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("pulsedag-runtime-stream-drop-{unique}"));
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        storage
            .append_runtime_event("info", "startup_completed", "startup completed")
            .expect("append runtime event");
        let mut stream = Box::pin(build_runtime_events_stream(storage, 100, 20, 10));
        let event = tokio::time::timeout(
            Duration::from_secs(2),
            std::future::poll_fn(|cx| futures_core::Stream::poll_next(stream.as_mut(), cx)),
        )
        .await
        .expect("stream poll timeout")
        .expect("stream ended unexpectedly")
        .expect("stream event result");
        let _ = event;
        drop(stream);
    }
}

#[cfg(test)]
mod startup_tests {
    use super::startup_status_view;
    use crate::api::NodeRuntimeStats;

    #[test]
    fn startup_view_reports_replay_path_coherently() {
        let runtime = NodeRuntimeStats {
            startup_path: "full_replay".to_string(),
            startup_snapshot_exists: false,
            ..NodeRuntimeStats::default()
        };
        let view = startup_status_view(&runtime);
        assert_eq!(view.bootstrap_mode, "replay");
        assert!(view.replay_required);
        assert!(!view.fastboot_used);
        assert!(view.fallback_reason.is_none());
    }

    #[test]
    fn startup_view_reports_recovery_fallback_coherently() {
        let runtime = NodeRuntimeStats {
            startup_path: "fallback_full_replay".to_string(),
            startup_fallback_reason: Some("snapshot validation failed".to_string()),
            ..NodeRuntimeStats::default()
        };
        let view = startup_status_view(&runtime);
        assert_eq!(view.bootstrap_mode, "recovery_fallback");
        assert!(view.replay_required);
        assert!(view.fallback_reason.is_some());
    }

    #[test]
    fn startup_view_prevents_contradictory_fastboot_flags() {
        let runtime = NodeRuntimeStats {
            startup_path: "full_replay".to_string(),
            startup_fastboot_used: true,
            startup_snapshot_validated: true,
            startup_delta_applied: true,
            startup_replay_required: false,
            ..NodeRuntimeStats::default()
        };
        let view = startup_status_view(&runtime);
        assert!(!view.fastboot_used);
        assert!(!view.snapshot_validated);
        assert!(!view.delta_applied);
        assert!(view.replay_required);
    }
}
