use crate::{
    api::ApiResponse,
    api::RpcStateLike,
    handlers::release::{operator_stage, repo_version},
};
use axum::{extract::State, Json};
use pulsedag_storage::StorageAuditReport;

#[derive(Debug, serde::Serialize)]
pub struct DiagnosticsData {
    pub version: String,
    pub stage: String,
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub snapshot_exists: bool,
    pub p2p_enabled: bool,
    pub peer_count: usize,
    pub storage_audit_ok: bool,
    pub storage_audit_issue_count: usize,
    pub storage_audit_summary: StorageAuditReport,
}

pub async fn get_diagnostics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<DiagnosticsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let storage_audit = state
        .storage()
        .audit_state_integrity(Some(&chain.chain_id), false)
        .unwrap_or(StorageAuditReport {
            ok: false,
            read_only: true,
            deep_check_performed: false,
            snapshot_exists: false,
            snapshot_best_height: None,
            persisted_block_count: 0,
            persisted_best_height: None,
            issue_count: 1,
            issues: vec![pulsedag_storage::StorageAuditIssue {
                code: "AUDIT_UNAVAILABLE".into(),
                message: "storage audit could not be completed".into(),
            }],
        });
    let snapshot_exists = state.storage().snapshot_exists().unwrap_or(false);
    let (p2p_enabled, peer_count) = match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => (true, status.connected_peers.len()),
            Err(_) => (true, 0),
        },
        None => (false, 0),
    };

    Json(ApiResponse::ok(DiagnosticsData {
        version: repo_version(),
        stage: operator_stage().to_string(),
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        snapshot_exists,
        p2p_enabled,
        peer_count,
        storage_audit_ok: storage_audit.ok,
        storage_audit_issue_count: storage_audit.issue_count,
        storage_audit_summary: storage_audit,
    }))
}

#[cfg(test)]
mod tests {
    use super::get_diagnostics;
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::extract::State;
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
        std::env::temp_dir().join(format!("pulsedag-diagnostics-{name}-{unique}"))
    }

    #[tokio::test]
    async fn diagnostics_reports_storage_audit_pass_fail_status() {
        let path = temp_db_path("audit-status");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8")).expect("open"));
        let chain = init_chain_state("testnet".to_string());
        storage
            .persist_chain_state(&chain)
            .expect("persist healthy snapshot");
        let genesis = chain
            .dag
            .blocks
            .get(&chain.dag.best_hash)
            .expect("genesis")
            .clone();
        storage.persist_block(&genesis).expect("persist genesis");
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        };

        let response = get_diagnostics(State(state)).await;
        let data = response.0.data.expect("data");
        assert!(data.storage_audit_ok);
        assert_eq!(data.storage_audit_issue_count, 0);
    }
}
