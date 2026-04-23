use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{
    extract::{Query, State},
    Json,
};
use pulsedag_storage::StorageAuditReport;
use std::collections::BTreeSet;

#[derive(Debug, Default, serde::Deserialize)]
pub struct MaintenanceReportQuery {
    pub deep: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct MaintenanceReportData {
    pub snapshot_exists: bool,
    pub snapshot_height: Option<u64>,
    pub captured_at_unix: Option<u64>,
    pub best_height: u64,
    pub in_memory_block_count: usize,
    pub persisted_block_count: usize,
    pub recommended_keep_from_height: u64,
    pub consistent: bool,
    pub recommended_action: String,
    pub state_audit: StorageAuditReport,
}

pub async fn get_maintenance_report<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<MaintenanceReportQuery>,
) -> Json<ApiResponse<MaintenanceReportData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_hashes = persisted_blocks
        .into_iter()
        .map(|b| b.hash)
        .collect::<BTreeSet<_>>();

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let keep_recent = runtime.prune_keep_recent_blocks.max(1);
    let recommended_keep_from_height = chain
        .dag
        .best_height
        .saturating_sub(keep_recent.saturating_sub(1));
    let captured_at_unix = match state.storage().snapshot_captured_at_unix() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let deep_check = query.deep.unwrap_or(false);
    let audit = match state
        .storage()
        .audit_state_integrity(Some(&chain.chain_id), deep_check)
    {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let memory_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();

    let consistent = memory_hashes == persisted_hashes;
    let recommended_action = if !audit.ok {
        "run storage self-check with deep=true and review audit issues".to_string()
    } else if !snapshot_exists {
        "create or refresh snapshot soon".to_string()
    } else if !consistent {
        "run sync verify and consider rebuild with force=true".to_string()
    } else if chain.mempool.transactions.len() > 1000 {
        "inspect mempool pressure".to_string()
    } else {
        "node state looks healthy".to_string()
    };

    Json(ApiResponse::ok(MaintenanceReportData {
        snapshot_exists,
        snapshot_height: if snapshot_exists {
            Some(chain.dag.best_height)
        } else {
            None
        },
        captured_at_unix,
        best_height: chain.dag.best_height,
        in_memory_block_count: memory_hashes.len(),
        persisted_block_count: persisted_hashes.len(),
        recommended_keep_from_height,
        consistent,
        recommended_action,
        state_audit: audit,
    }))
}

#[cfg(test)]
mod tests {
    use super::{get_maintenance_report, MaintenanceReportQuery};
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::extract::{Query, State};
    use pulsedag_core::genesis::init_chain_state;
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<pulsedag_core::ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<pulsedag_core::ChainState>> {
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
        std::env::temp_dir().join(format!("pulsedag-maintenance-{name}-{unique}"))
    }

    #[tokio::test]
    async fn maintenance_report_surfaces_audit_failures_for_operators() {
        let path = temp_db_path("audit-fail");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8")).expect("open"));
        let chain = init_chain_state("testnet".to_string());
        let genesis = chain
            .dag
            .blocks
            .get(&chain.dag.genesis_hash)
            .expect("genesis")
            .clone();
        storage.persist_block(&genesis).expect("persist block");
        let meta_cf = storage.db.cf_handle("meta").expect("meta cf");
        storage
            .db
            .put_cf(&meta_cf, b"chain_state", b"corrupt-snapshot")
            .expect("corrupt snapshot");

        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        };
        let response = get_maintenance_report(
            State(state),
            Query(MaintenanceReportQuery { deep: Some(true) }),
        )
        .await;
        let data = response.0.data.expect("data");
        assert!(!data.state_audit.ok);
        assert!(!data.recommended_action.is_empty());
    }
}
