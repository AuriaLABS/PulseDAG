use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{
    extract::{Path, State},
    Json,
};

#[derive(Debug, serde::Serialize)]
pub struct HealthData {
    pub service: String,
    pub status: String,
    pub chain_id: String,
    pub height: u64,
    pub selected_tip: Option<String>,
    pub consistency_ok: bool,
    pub consistency_issue_count: usize,
    pub mempool_size: usize,
    pub orphan_count: usize,
    pub p2p_enabled: bool,
    pub peer_count: usize,
    pub storage: String,
    pub uptime_secs: u64,
    pub burn_in_remaining_days: u64,
    pub startup_recovery_mode: String,
    pub last_self_audit_ok: bool,
    pub last_self_audit_issue_count: usize,
    pub active_alert_count: usize,
}
#[derive(Debug, serde::Serialize)]
pub struct GenesisData {
    pub genesis_block_hash: String,
    pub treasury_address: String,
    pub initial_supply: u64,
}
#[derive(Debug, serde::Serialize)]
pub struct DagBlockSummary {
    pub hash: String,
    pub height: u64,
    pub blue_score: u64,
    pub parents: Vec<String>,
}
#[derive(Debug, serde::Serialize)]
pub struct DagData {
    pub block_count: usize,
    pub selected_tip: Option<String>,
    pub tips: Vec<String>,
    pub blocks: Vec<DagBlockSummary>,
}
#[derive(Debug, serde::Serialize)]
pub struct TipsData {
    pub selected_tip: Option<String>,
    pub tips: Vec<String>,
}
#[derive(Debug, serde::Serialize)]
pub struct DagConsistencyData {
    pub ok: bool,
    pub selected_tip: Option<String>,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub issues: Vec<String>,
}
#[derive(Debug, serde::Serialize)]
pub struct BlockTxSummary {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
}
#[derive(Debug, serde::Serialize)]
pub struct BlockData {
    pub hash: String,
    pub height: u64,
    pub blue_score: u64,
    pub parents: Vec<String>,
    pub timestamp: u64,
    pub difficulty: u32,
    pub nonce: u64,
    pub tx_count: usize,
    pub previous_block_hash: Option<String>,
    pub next_block_hash: Option<String>,
    pub transactions: Vec<BlockTxSummary>,
}

pub async fn get_health<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<HealthData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let peer_count = state
        .p2p()
        .and_then(|p| p.status().ok())
        .map(|s| s.connected_peers.len())
        .unwrap_or(0);
    let selected_tip = pulsedag_core::preferred_tip_hash(&chain);
    let consistency_issues = pulsedag_core::dag_consistency_issues(&chain);
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(runtime.started_at_unix);
    let uptime_secs = now.saturating_sub(runtime.started_at_unix);
    let burn_in_remaining_days = 30u64.saturating_sub(uptime_secs / 86_400);
    Json(ApiResponse::ok(HealthData {
        service: "pulsedagd".into(),
        status: "ok".into(),
        chain_id: chain.chain_id.clone(),
        height: chain.dag.best_height,
        selected_tip,
        consistency_ok: consistency_issues.is_empty(),
        consistency_issue_count: consistency_issues.len(),
        mempool_size: chain.mempool.transactions.len(),
        orphan_count: chain.orphan_blocks.len(),
        p2p_enabled: state.p2p().is_some(),
        peer_count,
        storage: "rocksdb".into(),
        uptime_secs,
        burn_in_remaining_days,
        startup_recovery_mode: runtime.startup_recovery_mode.clone(),
        last_self_audit_ok: runtime.last_self_audit_ok,
        last_self_audit_issue_count: runtime.last_self_audit_issue_count,
        active_alert_count: runtime.active_alerts.len(),
    }))
}

pub async fn get_genesis<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<GenesisData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    Json(ApiResponse::ok(GenesisData {
        genesis_block_hash: chain.dag.genesis_hash.clone(),
        treasury_address: "genesis-treasury".into(),
        initial_supply: 1_000_000_000,
    }))
}

pub async fn get_dag<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<DagData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| DagBlockSummary {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            parents: b.header.parents.clone(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.blue_score.cmp(&a.blue_score))
            .then_with(|| a.hash.cmp(&b.hash))
    });
    let selected_tip = pulsedag_core::preferred_tip_hash(&chain);
    let tips = pulsedag_core::sorted_tip_hashes(&chain);
    Json(ApiResponse::ok(DagData {
        block_count: chain.dag.blocks.len(),
        selected_tip,
        tips,
        blocks,
    }))
}

pub async fn get_tips<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<TipsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let selected_tip = pulsedag_core::preferred_tip_hash(&chain);
    let tips = pulsedag_core::sorted_tip_hashes(&chain);
    Json(ApiResponse::ok(TipsData { selected_tip, tips }))
}

pub async fn get_block<S: RpcStateLike>(
    State(state): State<S>,
    Path(hash): Path<String>,
) -> Json<ApiResponse<BlockData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match chain.dag.blocks.get(&hash) {
        Some(block) => {
            let previous_block_hash = chain
                .dag
                .blocks
                .values()
                .find(|b| b.header.height + 1 == block.header.height)
                .map(|b| b.hash.clone());
            let next_block_hash = chain
                .dag
                .blocks
                .values()
                .find(|b| b.header.height == block.header.height + 1)
                .map(|b| b.hash.clone());

            Json(ApiResponse::ok(BlockData {
                hash: block.hash.clone(),
                height: block.header.height,
                blue_score: block.header.blue_score,
                parents: block.header.parents.clone(),
                timestamp: block.header.timestamp,
                difficulty: block.header.difficulty,
                nonce: block.header.nonce,
                tx_count: block.transactions.len(),
                previous_block_hash,
                next_block_hash,
                transactions: block
                    .transactions
                    .iter()
                    .map(|tx| BlockTxSummary {
                        txid: tx.txid.clone(),
                        fee: tx.fee,
                        inputs: tx.inputs.len(),
                        outputs: tx.outputs.len(),
                    })
                    .collect(),
            }))
        }
        None => Json(ApiResponse::err("NOT_FOUND", "block not found")),
    }
}

pub async fn get_dag_consistency<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<DagConsistencyData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let selected_tip = pulsedag_core::preferred_tip_hash(&chain);
    let issues = pulsedag_core::dag_consistency_issues(&chain);
    Json(ApiResponse::ok(DagConsistencyData {
        ok: issues.is_empty(),
        selected_tip,
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        issues,
    }))
}
