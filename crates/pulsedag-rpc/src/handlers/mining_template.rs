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
    build_candidate_block, build_coinbase_transaction, dev_difficulty_snapshot, pow_preimage_bytes,
    preferred_tip_hash, state::ChainState,
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
    #[serde(default)]
    pub mempool_fingerprint: String,
    #[serde(default)]
    pub mempool_tx_count: usize,
    #[serde(default)]
    pub expires_at_unix: u64,
    #[serde(default)]
    pub template_txids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct MiningTemplateData {
    pub mode: String,
    pub algorithm: String,
    pub miner_address: String,
    pub template_id: String,
    pub selected_tip: Option<String>,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
    pub block: pulsedag_core::types::Block,
    pub target_u64: u64,
    pub mempool_tx_count: usize,
    pub metrics_hint: PowMetricsData,
    pub pow_preimage_hex: String,
    pub pow_preimage_nonce_offset: usize,
    pub pow_header_preimage_version: u8,
    pub mutable_header_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateLifecycleState {
    pub height: u64,
    pub parent_hashes: Vec<String>,
    pub selected_tip: Option<String>,
    pub difficulty: u32,
    pub target_u64: u64,
    pub mempool_fingerprint: String,
    pub mempool_tx_count: usize,
}

pub(crate) const TEMPLATE_TTL_SECS: u64 = 30;
const POW_NONCE_OFFSET: usize = 1 + 4;

pub(crate) fn current_template_state(chain: &ChainState) -> TemplateLifecycleState {
    let height = chain.dag.best_height + 1;
    let mut parent_hashes = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    parent_hashes.sort();
    let selected_tip = preferred_tip_hash(chain);
    let snapshot = dev_difficulty_snapshot(chain);
    let difficulty = u32::try_from(snapshot.suggested_difficulty).unwrap_or(u32::MAX);
    let target_u64 = snapshot.target_u64;
    let mut tx_ids = chain
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    tx_ids.sort();
    let mempool_tx_count = tx_ids.len();
    let mempool_fingerprint = format!("{mempool_tx_count}:{}", tx_ids.join(","));

    TemplateLifecycleState {
        height,
        parent_hashes,
        selected_tip,
        difficulty,
        target_u64,
        mempool_fingerprint,
        mempool_tx_count,
    }
}

pub(crate) fn template_id_for_state(state: &TemplateLifecycleState) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        state.height,
        state.parent_hashes.join(","),
        state
            .selected_tip
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        state.difficulty,
        state.target_u64,
        state.mempool_fingerprint
    )
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
    let snapshot = dev_difficulty_snapshot(&chain);
    let lifecycle = current_template_state(&chain);
    let height = lifecycle.height;
    let parents = lifecycle.parent_hashes.clone();
    let reward = 50;
    let template_id = template_id_for_state(&lifecycle);
    let selected_tip = lifecycle.selected_tip.clone();
    let created_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let expires_at_unix = created_at_unix + TEMPLATE_TTL_SECS;

    let mut txs = vec![build_coinbase_transaction(
        &req.miner_address,
        reward,
        height,
    )];
    let mut mempool_txs = chain
        .mempool
        .transactions
        .values()
        .cloned()
        .collect::<Vec<_>>();
    mempool_txs.sort_by(|a, b| a.txid.cmp(&b.txid));
    txs.extend(mempool_txs);
    let header_difficulty = lifecycle.difficulty;
    let block = build_candidate_block(parents.clone(), height, header_difficulty, txs);
    let target_u64 = lifecycle.target_u64;
    let template_txids = block
        .transactions
        .iter()
        .map(|tx| tx.txid.clone())
        .collect::<Vec<_>>();
    let pow_preimage_hex = hex::encode(pow_preimage_bytes(&block.header));

    store_template(&StoredMiningTemplate {
        template_id: template_id.clone(),
        miner_address: req.miner_address.clone(),
        selected_tip: selected_tip.clone(),
        parent_hashes: parents,
        height,
        difficulty: header_difficulty,
        created_at_unix,
        target_u64,
        mempool_fingerprint: lifecycle.mempool_fingerprint.clone(),
        mempool_tx_count: lifecycle.mempool_tx_count,
        expires_at_unix,
        template_txids: template_txids.clone(),
    });
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.external_mining_templates_emitted =
            runtime.external_mining_templates_emitted.saturating_add(1);
        if runtime
            .external_mining_last_template_id
            .as_ref()
            .is_some_and(|last| last != &template_id)
        {
            runtime.external_mining_templates_invalidated = runtime
                .external_mining_templates_invalidated
                .saturating_add(1);
            runtime.external_mining_stale_work_detected = runtime
                .external_mining_stale_work_detected
                .saturating_add(1);
            let _ = state.storage().append_runtime_event(
                "warn",
                "external_mining_template_invalidated",
                &format!(
                    "previous={} current={}",
                    runtime
                        .external_mining_last_template_id
                        .clone()
                        .unwrap_or_default(),
                    template_id
                ),
            );
        }
        runtime.external_mining_last_template_id = Some(template_id.clone());
    }
    let _ = state.storage().append_runtime_event(
        "info",
        "external_mining_template_emitted",
        &format!(
            "template_id={} height={} expires_at_unix={} miner={}",
            template_id, height, expires_at_unix, req.miner_address
        ),
    );

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
        expires_at_unix,
        block,
        target_u64,
        mempool_tx_count: lifecycle.mempool_tx_count,
        metrics_hint,
        pow_preimage_hex,
        pow_preimage_nonce_offset: POW_NONCE_OFFSET,
        pow_header_preimage_version: pulsedag_core::POW_HEADER_PREIMAGE_VERSION,
        mutable_header_fields: vec!["nonce".to_string()],
    }))
}

#[cfg(test)]
mod tests {
    use super::{current_template_state, template_id_for_state};
    use pulsedag_core::genesis::init_chain_state;

    #[test]
    fn template_id_changes_when_mempool_changes() {
        let mut chain = init_chain_state("testnet-dev".to_string());
        let state_before = current_template_state(&chain);
        let before = template_id_for_state(&state_before);

        let tx = pulsedag_core::build_coinbase_transaction("kaspa:qptest", 1, 1);
        chain.mempool.transactions.insert(tx.txid.clone(), tx);
        let state_after = current_template_state(&chain);
        let after = template_id_for_state(&state_after);

        assert_ne!(before, after);
        assert_eq!(state_after.mempool_tx_count, 1);
    }
}
