use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

use axum::{extract::State, Json};
use crate::{api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike}, handlers::pow_metrics::PowMetricsData};
use pulsedag_core::{build_candidate_block, build_coinbase_transaction, dev_recommended_difficulty_for_chain, dev_target_u64, preferred_tip_hash};

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
    let path = PathBuf::from("./data/mining_templates").join(format!("{}.json", sanitize(template_id)));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<StoredMiningTemplate>(&bytes).ok()
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

pub async fn post_mining_template<S: RpcStateLike>(State(state): State<S>, Json(req): Json<GetBlockTemplateRequest>) -> Json<ApiResponse<MiningTemplateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let height = chain.dag.best_height + 1;
    let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    parents.sort();
    let difficulty = dev_recommended_difficulty_for_chain(&chain);
    let reward = 50;
    let template_id = format!("{}:{}", height, parents.join(","));
    let selected_tip = preferred_tip_hash(&chain);
    let created_at_unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

    let mut txs = vec![build_coinbase_transaction(&req.miner_address, reward, height)];
    txs.extend(chain.mempool.transactions.values().cloned());
    let block = build_candidate_block(parents.clone(), height, difficulty as u32, txs);
    let target_u64 = dev_target_u64(difficulty as u64);

    store_template(&StoredMiningTemplate {
        template_id: template_id.clone(),
        miner_address: req.miner_address.clone(),
        selected_tip: selected_tip.clone(),
        parent_hashes: parents,
        height,
        difficulty: difficulty as u32,
        created_at_unix,
        target_u64,
    });

    let metrics_hint = PowMetricsData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        best_height: chain.dag.best_height,
        window_size: 10,
        observed_block_count: chain.dag.blocks.len().min(10),
        avg_block_interval_secs: 0,
        suggested_difficulty: difficulty as u64,
        target_u64,
        target_block_interval_secs: pulsedag_core::dev_target_block_interval_secs(),
        retarget_multiplier_bps: 10_000,
        notes: vec!["Mining template uses the live 60 second block target policy".to_string()],
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
