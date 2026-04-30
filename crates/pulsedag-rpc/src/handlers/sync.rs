use axum::{extract::State, Json};

use crate::api::{ApiResponse, RpcStateLike, SyncRebuildRequest};
use pulsedag_core::reconcile_mempool;

#[derive(Debug, serde::Serialize)]
pub struct SyncStatusData {
    pub chain_id: String,
    pub snapshot_exists: bool,
    pub persisted_block_count: usize,
    pub in_memory_block_count: usize,
    pub best_height: u64,
    pub selected_tip: Option<String>,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub orphan_count: usize,
    pub can_replay_from_blocks: bool,
    pub replay_gap: i64,
    pub rebuild_recommended: bool,
    pub consistency_ok: bool,
    pub consistency_issue_count: usize,
    pub catchup_stage: String,
    pub lag_blocks: u64,
    pub lag_band: String,
    pub catchup_progress_bps: u64,
    pub catchup_summary: String,
    pub recovery_reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncReconcileMempoolData {
    pub removed_count: usize,
    pub kept_count: usize,
    pub removed_txids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncRebuildData {
    pub rebuilt: bool,
    pub block_count: usize,
    pub best_height: u64,
    pub selected_tip: Option<String>,
    pub consistency_ok: bool,
    pub consistency_issue_count: usize,
    pub mempool_reconciled: bool,
    pub snapshot_persisted: bool,
    pub partial_replay_used: bool,
    pub accepted_blocks: usize,
    pub skipped_blocks: usize,
    pub skipped_hashes: Vec<String>,
}

pub async fn get_sync_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SyncStatusData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let consistency_issues = pulsedag_core::dag_consistency_issues(&chain);
    let lag_blocks = (persisted_blocks.len() as u64).saturating_sub(chain.dag.blocks.len() as u64);
    let lag_band = match lag_blocks {
        0 => "aligned",
        1..=2 => "near_tip",
        3..=10 => "catching_up",
        11..=100 => "lagging",
        _ => "severely_lagging",
    }
    .to_string();
    let catchup_progress_bps = if persisted_blocks.is_empty() {
        10_000
    } else {
        (chain.dag.blocks.len() as u64)
            .saturating_mul(10_000)
            .saturating_div(persisted_blocks.len() as u64)
            .min(10_000)
    };
    let counters_coherent = runtime.sync_pipeline.counters.blocks_applied
        <= runtime.sync_pipeline.counters.blocks_validated
        && runtime.sync_pipeline.counters.blocks_validated
            <= runtime.sync_pipeline.counters.blocks_acquired
        && runtime.sync_pipeline.counters.blocks_acquired
            <= runtime.sync_pipeline.counters.blocks_requested;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stalled = runtime.sync_pipeline.phase != pulsedag_core::SyncPhase::Idle
        && (lag_blocks > 0
            || runtime.sync_pipeline.counters.blocks_requested
                > runtime.sync_pipeline.counters.blocks_applied)
        && runtime
            .sync_pipeline
            .last_transition_unix
            .map(|ts| now_unix.saturating_sub(ts) > 120)
            .unwrap_or(false);
    let catchup_stage = if runtime.sync_pipeline.last_error.is_some() || !counters_coherent {
        "degraded"
    } else if stalled {
        "stalled"
    } else {
        match runtime.sync_pipeline.phase {
            pulsedag_core::SyncPhase::Idle if lag_blocks == 0 => "steady",
            pulsedag_core::SyncPhase::Idle => "recovering",
            pulsedag_core::SyncPhase::PeerSelection => "discovering",
            pulsedag_core::SyncPhase::HeaderDiscovery => "discovering",
            pulsedag_core::SyncPhase::BlockAcquisition => "recovering",
            pulsedag_core::SyncPhase::ValidationApplication => "validating",
            pulsedag_core::SyncPhase::CatchUpCompletion => "steady",
        }
    }
    .to_string();
    let recovery_reason = if let Some(err) = runtime.sync_pipeline.last_error.clone() {
        Some(format!("sync error: {err}"))
    } else if !counters_coherent {
        Some("sync counter incoherence detected; verify sync pipeline accounting".to_string())
    } else if stalled {
        Some(format!(
            "no-progress escalation: sync stalled in {:?} with lag_band={lag_band}; bounded remediation active (fallbacks={}, timeouts={}, restarts={})",
            runtime.sync_pipeline.phase,
            runtime.sync_pipeline.fallback_count,
            runtime.sync_pipeline.timeout_fallback_count,
            runtime.sync_pipeline.restart_count
        ))
    } else if catchup_stage != "steady" {
        Some(format!(
            "catch-up in progress: stage={catchup_stage}, lag_band={lag_band}, replay_gap={}",
            persisted_blocks.len() as i64 - chain.dag.blocks.len() as i64
        ))
    } else {
        None
    };
    let catchup_summary = format!(
        "stage={catchup_stage} lag_blocks={lag_blocks} lag_band={lag_band} progress_bps={catchup_progress_bps}"
    );
    Json(ApiResponse::ok(SyncStatusData {
        chain_id: chain.chain_id.clone(),
        snapshot_exists,
        persisted_block_count: persisted_blocks.len(),
        in_memory_block_count: chain.dag.blocks.len(),
        best_height: chain.dag.best_height,
        selected_tip: pulsedag_core::preferred_tip_hash(&chain),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        orphan_count: chain.orphan_blocks.len(),
        can_replay_from_blocks: !persisted_blocks.is_empty(),
        replay_gap: persisted_blocks.len() as i64 - chain.dag.blocks.len() as i64,
        rebuild_recommended: !snapshot_exists || persisted_blocks.len() > chain.dag.blocks.len(),
        consistency_ok: consistency_issues.is_empty(),
        consistency_issue_count: consistency_issues.len(),
        catchup_stage,
        lag_blocks,
        lag_band,
        catchup_progress_bps,
        catchup_summary,
        recovery_reason,
    }))
}

pub async fn post_sync_rebuild<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SyncRebuildRequest>,
) -> Json<ApiResponse<SyncRebuildData>> {
    let current_chain_id = {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        chain.chain_id.clone()
    };

    if !req.force {
        return Json(ApiResponse::err(
            "REBUILD_REQUIRES_FORCE",
            "set force=true to rebuild state from persisted blocks",
        ));
    }

    let allow_partial_replay = req.allow_partial_replay.unwrap_or(false);
    let persist_after_rebuild = req.persist_after_rebuild.unwrap_or(true);
    let reconcile_mempool_after = req.reconcile_mempool.unwrap_or(true);

    let (mut rebuilt, partial_replay_used, accepted_blocks, skipped_blocks, skipped_hashes) =
        if allow_partial_replay {
            let persisted_blocks = match state.storage().list_blocks() {
                Ok(v) => v,
                Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
            };
            let report = pulsedag_core::rebuild_state_from_blocks_defensive(
                current_chain_id,
                persisted_blocks,
            );
            (
                report.state,
                true,
                report.accepted_blocks,
                report.skipped_blocks,
                report.skipped_hashes,
            )
        } else {
            let rebuilt = match state.storage().replay_blocks_or_init(current_chain_id) {
                Ok(v) => v,
                Err(e) => return Json(ApiResponse::err("REBUILD_ERROR", e.to_string())),
            };
            let accepted_blocks = rebuilt.dag.blocks.len().saturating_sub(1);
            (rebuilt, false, accepted_blocks, 0, Vec::new())
        };

    let mempool_reconciled = if reconcile_mempool_after {
        let _ = reconcile_mempool(&mut rebuilt);
        true
    } else {
        false
    };

    let block_count = rebuilt.dag.blocks.len();
    let best_height = rebuilt.dag.best_height;
    let selected_tip = pulsedag_core::preferred_tip_hash(&rebuilt);
    let consistency_issues = pulsedag_core::dag_consistency_issues(&rebuilt);

    {
        let chain_handle = state.chain();
        let mut chain = chain_handle.write().await;
        *chain = rebuilt.clone();
    }

    let snapshot_persisted = if persist_after_rebuild {
        if let Err(e) = state.storage().persist_chain_state(&rebuilt) {
            return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
        }
        true
    } else {
        false
    };

    Json(ApiResponse::ok(SyncRebuildData {
        rebuilt: true,
        block_count,
        best_height,
        selected_tip,
        consistency_ok: consistency_issues.is_empty(),
        consistency_issue_count: consistency_issues.len(),
        mempool_reconciled,
        snapshot_persisted,
        partial_replay_used,
        accepted_blocks,
        skipped_blocks,
        skipped_hashes,
    }))
}

pub async fn post_sync_reconcile_mempool<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SyncReconcileMempoolData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let result = reconcile_mempool(&mut chain);
    let snapshot = chain.clone();
    drop(chain);

    if let Err(e) = state.storage().persist_chain_state(&snapshot) {
        return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
    }

    Json(ApiResponse::ok(SyncReconcileMempoolData {
        removed_count: result.removed_txids.len(),
        kept_count: result.kept_txids.len(),
        removed_txids: result.removed_txids,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{extract::State, Json};
    use pulsedag_core::{ChainState, SyncPhase};
    use pulsedag_storage::Storage;
    use tokio::sync::RwLock;

    use crate::api::{NodeRuntimeStats, RpcStateLike};

    use super::get_sync_status;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }

        fn p2p(&self) -> Option<Arc<dyn pulsedag_p2p::P2pHandle>> {
            None
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

    #[tokio::test]
    async fn sync_status_derives_catchup_stage_and_recovery_reason_coherently() {
        let path = temp_db_path("sync-status-stage");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::BlockAcquisition;
        runtime.sync_pipeline.counters.blocks_requested = 20;
        runtime.sync_pipeline.counters.blocks_acquired = 10;
        runtime.sync_pipeline.counters.blocks_validated = 8;
        runtime.sync_pipeline.counters.blocks_applied = 6;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp) = get_sync_status(State(state)).await;
        assert!(resp.ok);
        let data = resp.data.expect("sync status payload");
        assert_eq!(data.catchup_stage, "recovering");
        assert!(data.recovery_reason.is_some());
        assert_eq!(data.lag_band, "aligned");
        assert_eq!(data.catchup_progress_bps, 10_000);
    }

    #[tokio::test]
    async fn sync_status_lag_band_and_progress_are_bounded_and_deterministic() {
        let path = temp_db_path("sync-status-bounds");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::ValidationApplication;
        runtime.sync_pipeline.counters.blocks_requested = 1;
        runtime.sync_pipeline.counters.blocks_acquired = 1;
        runtime.sync_pipeline.counters.blocks_validated = 1;
        runtime.sync_pipeline.counters.blocks_applied = 5;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp1) = get_sync_status(State(state.clone())).await;
        let Json(resp2) = get_sync_status(State(state)).await;
        let data1 = resp1.data.expect("sync status payload #1");
        let data2 = resp2.data.expect("sync status payload #2");
        assert_eq!(data1.lag_blocks, 0);
        assert_eq!(data1.lag_band, "aligned");
        assert_eq!(data1.catchup_progress_bps, 10_000);
        assert_eq!(data1.catchup_summary, data2.catchup_summary);
    }

    #[tokio::test]
    async fn sync_status_no_progress_escalation_is_explicit_and_bounded() {
        let path = temp_db_path("sync-status-no-progress");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut runtime = NodeRuntimeStats::default();
        runtime.sync_pipeline.phase = SyncPhase::ValidationApplication;
        runtime.sync_pipeline.counters.blocks_requested = 30;
        runtime.sync_pipeline.counters.blocks_acquired = 20;
        runtime.sync_pipeline.counters.blocks_validated = 20;
        runtime.sync_pipeline.counters.blocks_applied = 10;
        runtime.sync_pipeline.last_transition_unix = Some(now.saturating_sub(240));
        runtime.sync_pipeline.fallback_count = 2;
        runtime.sync_pipeline.timeout_fallback_count = 1;
        runtime.sync_pipeline.restart_count = 1;

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
        };

        let Json(resp) = get_sync_status(State(state)).await;
        let data = resp.data.expect("sync status payload");
        assert_eq!(data.catchup_stage, "validating");
        let reason = data.recovery_reason.expect("recovery reason");
        assert!(reason.contains("no-progress escalation"));
        assert!(reason.contains("bounded remediation active"));
        assert!(reason.contains("fallbacks=2"));
        assert!(reason.contains("timeouts=1"));
        assert!(reason.contains("restarts=1"));
    }
}
