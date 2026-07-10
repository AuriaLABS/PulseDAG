use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{
    extract::{Path, State},
    Json,
};

#[derive(Debug, serde::Serialize)]
pub struct HealthData {
    pub service: String,
    pub status: String,
    pub process_alive: bool,
    pub listener_alive: bool,
    pub chain_id: String,
    pub height: u64,
    pub selected_tip: Option<String>,
    pub snapshot_age_ms: u64,
    pub peer_count: usize,
    pub storage: String,
    pub startup_recovery_mode: String,
    pub last_consistency_audit_ok: bool,
    pub last_consistency_audit_issue_count: usize,
    pub last_consistency_audit_unix: Option<u64>,
    pub active_alert_count: usize,
    pub degraded_reason: Option<String>,
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
    let started = std::time::Instant::now();
    let snapshot = state.rpc_snapshot().load();
    let snapshot_age_ms = crate::api::unix_now_ms().saturating_sub(snapshot.last_updated_ms);
    if snapshot.stale {
        crate::api::record_health_snapshot_stale();
    }
    let status =
        if !snapshot.last_consistency_audit_ok && snapshot.last_consistency_audit_issue_count > 0 {
            "failed"
        } else if snapshot.stale || snapshot.degraded {
            "degraded"
        } else {
            "ok"
        };
    let response = Json(ApiResponse::ok(HealthData {
        service: "pulsedagd".into(),
        status: status.into(),
        process_alive: true,
        listener_alive: true,
        chain_id: snapshot.chain_id,
        height: snapshot.height,
        selected_tip: snapshot.tip,
        snapshot_age_ms,
        peer_count: snapshot.peer_count,
        storage: snapshot.storage_mode,
        startup_recovery_mode: snapshot.startup_mode,
        last_consistency_audit_ok: snapshot.last_consistency_audit_ok,
        last_consistency_audit_issue_count: snapshot.last_consistency_audit_issue_count,
        last_consistency_audit_unix: snapshot.last_consistency_audit_unix,
        active_alert_count: snapshot.active_alert_count,
        degraded_reason: snapshot.degraded_reason,
    }));
    crate::api::record_health_handler_duration_ms(started.elapsed().as_millis() as u64);
    response
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
