use axum::{extract::State, Json};

use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct MetricsData {
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub circulating_supply: u64,
    pub last_block_hash: Option<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
    pub blocks_accepted_total: u64,
    pub blocks_rejected_total: u64,
    pub invalid_pow_total: u64,
    pub mining_templates_total: u64,
    pub mining_submits_total: u64,
    pub p2p_blocks_received_total: u64,
    pub tx_inbound_received: u64,
    pub tx_inbound_accepted: u64,
    pub tx_inbound_duplicate: u64,
    pub tx_inbound_invalid: u64,
    pub tx_relayed: u64,
    pub tx_relay_suppressed_budget: u64,
    pub tx_relay_suppressed_duplicate: u64,
    pub sync_missing_parents_total: u64,
    pub orphan_current_count: usize,
    pub peer_count: usize,
    pub limitations: Vec<String>,
}

pub async fn get_metrics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MetricsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let runtime = state.runtime();
    let runtime = runtime.read().await;
    let p2p_status = state.p2p().and_then(|p| p.status().ok());
    let peer_count = p2p_status
        .as_ref()
        .map(|s| s.connected_peers.len())
        .unwrap_or(0);
    let circulating_supply = chain.utxo.utxos.values().map(|u| u.amount).sum();
    let last_block_hash = chain
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| b.hash.clone());

    Json(ApiResponse::ok(MetricsData {
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        address_count: chain.utxo.address_index.len(),
        circulating_supply,
        last_block_hash,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
        blocks_accepted_total: runtime.pulsedag_blocks_accepted_total,
        blocks_rejected_total: runtime.pulsedag_blocks_rejected_total,
        invalid_pow_total: runtime.pulsedag_invalid_pow_total,
        mining_templates_total: runtime.pulsedag_mining_templates_total,
        mining_submits_total: runtime.pulsedag_mining_submits_total,
        p2p_blocks_received_total: runtime.pulsedag_p2p_blocks_received_total,
        tx_inbound_received: runtime.tx_inbound_received,
        tx_inbound_accepted: runtime.tx_inbound_accepted,
        tx_inbound_duplicate: runtime.tx_inbound_duplicate,
        tx_inbound_invalid: runtime.tx_inbound_invalid,
        tx_relayed: runtime.tx_relayed.saturating_add(
            p2p_status
                .as_ref()
                .map(|s| s.tx_relayed as u64)
                .unwrap_or(0),
        ),
        tx_relay_suppressed_budget: runtime.tx_relay_suppressed_budget.saturating_add(
            p2p_status
                .as_ref()
                .map(|s| s.tx_relay_suppressed_budget as u64)
                .unwrap_or(0),
        ),
        tx_relay_suppressed_duplicate: runtime.tx_relay_suppressed_duplicate.saturating_add(
            p2p_status
                .as_ref()
                .map(|s| s.tx_relay_suppressed_duplicate as u64)
                .unwrap_or(0),
        ),
        sync_missing_parents_total: runtime.pulsedag_sync_missing_parents_total,
        orphan_current_count: chain.orphan_blocks.len(),
        peer_count,
        limitations: vec![
            "Counters reset on node restart.".to_string(),
            "Peer and orphan counts are point-in-time snapshots.".to_string(),
        ],
    }))
}
