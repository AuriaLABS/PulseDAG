use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike},
    handlers::pow_metrics::PowMetricsData,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    build_candidate_block, build_coinbase_transaction, dev_difficulty_snapshot, preferred_tip_hash,
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct StoredMiningTemplate {
    pub template_id: String,
    pub miner_address: String,
    pub selected_tip: Option<String>,
    pub parent_hashes: Vec<String>,
    pub height: u64,
    pub difficulty: u32,
    pub created_at_unix: u64,
    pub target_u64: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MiningTemplateData {
    pub mode: String,
    pub algorithm: String,
    pub miner_address: String,
    pub template_id: String,
    pub selected_tip: Option<String>,
    pub created_at_unix: u64,
    pub block: pulsedag_core::types::Block,
    pub target_u64: u64,
    pub metrics_hint: PowMetricsData,
}

pub(crate) fn store_template(record: &StoredMiningTemplate) {
    let dir = PathBuf::from("./data/mining_templates");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.json", sanitize(&record.template_id)));
    let _ = fs::write(path, serde_json::to_vec_pretty(record).unwrap_or_default());
}

pub(crate) fn load_template(template_id: &str) -> Option<StoredMiningTemplate> {
    let path =
        PathBuf::from("./data/mining_templates").join(format!("{}.json", sanitize(template_id)));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<StoredMiningTemplate>(&bytes).ok()
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<MiningTemplateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let height = chain.dag.best_height + 1;
    let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    parents.sort();
    let snapshot = dev_difficulty_snapshot(&chain);
    let difficulty = snapshot.suggested_difficulty;
    let reward = 50;
    let template_id = format!("{}:{}", height, parents.join(","));
    let selected_tip = preferred_tip_hash(&chain);
    let created_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut txs = vec![build_coinbase_transaction(
        &req.miner_address,
        reward,
        height,
    )];
    txs.extend(chain.mempool.transactions.values().cloned());
    let header_difficulty = u32::try_from(difficulty).unwrap_or(u32::MAX);
    let block = build_candidate_block(parents.clone(), height, header_difficulty, txs);
    let target_u64 = snapshot.target_u64;

    store_template(&StoredMiningTemplate {
        template_id: template_id.clone(),
        miner_address: req.miner_address.clone(),
        selected_tip: selected_tip.clone(),
        parent_hashes: parents,
        height,
        difficulty: header_difficulty,
        created_at_unix,
        target_u64,
    });
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.mining_templates_issued += 1;
        runtime.mining_template_refresh_events += 1;
        if runtime
            .mining_last_template_id
            .as_ref()
            .is_some_and(|previous| previous != &template_id)
        {
            runtime.mining_template_invalidations += 1;
            runtime.mining_stale_work_indicated += 1;
        }
        runtime.mining_last_template_id = Some(template_id.clone());
        runtime.mining_last_template_created_unix = Some(created_at_unix);
    }

    let metrics_hint = PowMetricsData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        best_height: chain.dag.best_height,
        window_size: snapshot.policy.window_size,
        observed_block_count: snapshot.observed_block_count,
        avg_block_interval_secs: snapshot.avg_block_interval_secs,
        suggested_difficulty: snapshot.suggested_difficulty,
        target_u64,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        notes: vec!["Mining template uses centralized runtime retarget policy".to_string()],
    };

    Json(ApiResponse::ok(MiningTemplateData {
        mode: "external-miner-template".to_string(),
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        miner_address: req.miner_address,
        template_id,
        selected_tip,
        created_at_unix,
        block,
        target_u64,
        metrics_hint,
    }))
}

#[cfg(test)]
mod tests {
    use super::post_mining_template;
    use crate::api::{GetBlockTemplateRequest, NodeRuntimeStats, RpcStateLike};
    use axum::{extract::State, Json};
    use pulsedag_core::state::ChainState;
    use pulsedag_p2p::P2pHandle;
    use pulsedag_storage::Storage;
    use std::{
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
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }
        fn p2p(&self) -> Option<Arc<dyn P2pHandle>> {
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

    fn build_state() -> TestState {
        let path = temp_db_path("mining-template-tests");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        }
    }

    #[tokio::test]
    async fn template_counters_capture_refresh_and_invalidation() {
        let state = build_state();
        let req = GetBlockTemplateRequest {
            miner_address: "kaspa:qptestminer".to_string(),
        };

        let Json(first) = post_mining_template(State(state.clone()), Json(req.clone())).await;
        assert!(first.ok);
        let first_template_id = first.data.unwrap().template_id;

        let Json(second) = post_mining_template(State(state.clone()), Json(req)).await;
        assert!(second.ok);
        let second_template_id = second.data.unwrap().template_id;
        assert_eq!(first_template_id, second_template_id);

        let mut runtime = state.runtime.write().await;
        runtime.mining_last_template_id = Some("forced-old-template".to_string());
        drop(runtime);

        let Json(third) = post_mining_template(
            State(state.clone()),
            Json(GetBlockTemplateRequest {
                miner_address: "kaspa:qptestminer".to_string(),
            }),
        )
        .await;
        assert!(third.ok);

        let runtime = state.runtime.read().await;
        assert_eq!(runtime.mining_templates_issued, 3);
        assert_eq!(runtime.mining_template_refresh_events, 3);
        assert_eq!(runtime.mining_template_invalidations, 1);
    }
}
