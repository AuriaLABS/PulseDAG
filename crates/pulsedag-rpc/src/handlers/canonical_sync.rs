use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::api::NodeRuntimeStats;
use pulsedag_core::{state::ChainState, SyncPhase};

#[derive(Debug, Clone, serde::Serialize)]
pub struct CanonicalSyncState {
    pub sync_state: String,
    pub catchup_stage: String,
    pub lag_blocks: u64,
    pub lag_band: String,
    pub catchup_progress_bps: u64,
    pub catchup_summary: String,
    pub recovery_reason: Option<String>,
    pub selected_sync_peer: Option<String>,
    pub canonical_sync_state_generation: u64,
    pub synced_with_recovering_stage_total: u64,
    pub aligned_with_active_recovery_reason_total: u64,
    pub catchup_recovery_started_total: u64,
    pub catchup_recovery_completed_total: u64,
    pub catchup_recovery_last_reason: Option<String>,
}

pub fn lag_band(lag_blocks: u64) -> &'static str {
    match lag_blocks {
        0 => "aligned",
        1..=2 => "near_tip",
        3..=10 => "catching_up",
        11..=100 => "lagging",
        _ => "severely_lagging",
    }
}

fn counters_coherent(runtime: &NodeRuntimeStats) -> bool {
    runtime.sync_pipeline.counters.blocks_applied <= runtime.sync_pipeline.counters.blocks_validated
        && runtime.sync_pipeline.counters.blocks_validated
            <= runtime.sync_pipeline.counters.blocks_acquired
        && runtime.sync_pipeline.counters.blocks_acquired
            <= runtime.sync_pipeline.counters.blocks_requested
}

pub fn build_canonical_sync_state(
    chain: &ChainState,
    runtime: &NodeRuntimeStats,
    persisted_block_count: usize,
    now_unix: u64,
    fallback_selected_sync_peer: Option<String>,
) -> CanonicalSyncState {
    let lag_blocks = (persisted_block_count as u64).saturating_sub(chain.dag.blocks.len() as u64);
    let lag_band = lag_band(lag_blocks).to_string();
    let catchup_progress_bps = if persisted_block_count == 0 {
        10_000
    } else {
        (chain.dag.blocks.len() as u64)
            .saturating_mul(10_000)
            .saturating_div(persisted_block_count as u64)
            .min(10_000)
    };
    let pending_missing_parents = pulsedag_core::pending_missing_parent_count(chain);
    let active_terminal_missing_parents =
        pulsedag_core::terminal_missing_parent_active_blocking_count(chain);
    let has_orphan_work = !chain.orphan_blocks.is_empty() || active_terminal_missing_parents > 0;
    let no_blockers = lag_blocks == 0
        && has_orphan_work == false
        && pending_missing_parents == 0
        && runtime.pending_block_requests == 0
        && runtime.inflight_block_requests == 0;
    let coherent = counters_coherent(runtime);
    let stalled = runtime.sync_pipeline.phase != SyncPhase::Idle
        && lag_band != "aligned"
        && (lag_blocks > 0
            || runtime.sync_pipeline.counters.blocks_requested
                > runtime.sync_pipeline.counters.blocks_applied)
        && runtime
            .sync_pipeline
            .last_transition_unix
            .map(|ts| now_unix.saturating_sub(ts) > 120)
            .unwrap_or(false);

    let (sync_state, catchup_stage, recovery_reason) = if no_blockers
        && coherent
        && runtime.sync_pipeline.last_error.is_none()
    {
        ("synced".to_string(), "steady".to_string(), None)
    } else if runtime.sync_pipeline.last_error.is_some() || !coherent {
        (
            runtime.sync_state.clone(),
            "degraded".to_string(),
            Some(
                if let Some(err) = runtime.sync_pipeline.last_error.clone() {
                    format!("sync error: {err}")
                } else {
                    "sync counter incoherence detected; verify sync pipeline accounting".to_string()
                },
            ),
        )
    } else if stalled {
        (
            runtime.sync_state.clone(),
            "recovering".to_string(),
            Some(format!(
                "no-progress escalation: sync stalled in {:?} with lag_band={lag_band}; bounded remediation active (fallbacks={}, timeouts={}, restarts={})",
                runtime.sync_pipeline.phase,
                runtime.sync_pipeline.fallback_count,
                runtime.sync_pipeline.timeout_fallback_count,
                runtime.sync_pipeline.restart_count
            )),
        )
    } else if pending_missing_parents > 0 {
        (
            runtime.sync_state.clone(),
            "recovering".to_string(),
            Some(format!(
                "orphan recovery pending: {} missing parent(s) still queued for reprocess",
                pending_missing_parents
            )),
        )
    } else if has_orphan_work {
        (
            runtime.sync_state.clone(),
            "recovering".to_string(),
            Some(
                "orphan recovery active: queued orphan or terminal missing-parent blocker remains"
                    .to_string(),
            ),
        )
    } else {
        let stage = match runtime.sync_pipeline.phase {
            SyncPhase::Idle if lag_blocks == 0 => "steady",
            SyncPhase::Idle => "recovering",
            SyncPhase::PeerSelection | SyncPhase::HeaderDiscovery => "discovering",
            SyncPhase::BlockAcquisition => "recovering",
            SyncPhase::ValidationApplication => "validating",
            SyncPhase::CatchUpCompletion => "steady",
        };
        let reason = if stage != "steady" || lag_blocks > 0 {
            Some(format!(
                "catch-up in progress: stage={stage}, lag_band={lag_band}, replay_gap={}",
                persisted_block_count as i64 - chain.dag.blocks.len() as i64
            ))
        } else {
            None
        };
        (runtime.sync_state.clone(), stage.to_string(), reason)
    };

    let selected_sync_peer = if sync_state == "synced" && catchup_stage == "steady" {
        None
    } else {
        runtime
            .sync_pipeline
            .selected_peer
            .clone()
            .or(fallback_selected_sync_peer)
    };
    let catchup_summary = format!(
        "stage={catchup_stage} lag_blocks={lag_blocks} lag_band={lag_band} progress_bps={catchup_progress_bps}"
    );
    let synced_with_recovering_stage_total =
        u64::from(sync_state == "synced" && catchup_stage == "recovering");
    let aligned_with_active_recovery_reason_total =
        u64::from(lag_band == "aligned" && recovery_reason.is_some() && catchup_stage != "steady");
    let catchup_recovery_started_total = runtime.sync_pipeline.restart_count
        + runtime.sync_pipeline.fallback_count
        + runtime.sync_pipeline.timeout_fallback_count;
    let catchup_recovery_completed_total =
        u64::from(sync_state == "synced" && catchup_stage == "steady");
    let catchup_recovery_last_reason = recovery_reason.clone();
    let mut hasher = DefaultHasher::new();
    sync_state.hash(&mut hasher);
    catchup_stage.hash(&mut hasher);
    lag_blocks.hash(&mut hasher);
    lag_band.hash(&mut hasher);
    recovery_reason.hash(&mut hasher);
    selected_sync_peer.hash(&mut hasher);
    let canonical_sync_state_generation = hasher.finish();

    CanonicalSyncState {
        sync_state,
        catchup_stage,
        lag_blocks,
        lag_band,
        catchup_progress_bps,
        catchup_summary,
        recovery_reason,
        selected_sync_peer,
        canonical_sync_state_generation,
        synced_with_recovering_stage_total,
        aligned_with_active_recovery_reason_total,
        catchup_recovery_started_total,
        catchup_recovery_completed_total,
        catchup_recovery_last_reason,
    }
}
