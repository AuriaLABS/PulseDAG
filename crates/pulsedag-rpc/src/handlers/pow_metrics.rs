use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct PowMetricsData {
    pub algorithm: String,
    pub window_size: usize,
    pub best_height: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    pub suggested_difficulty: u64,
    pub target_u64: u64,
    pub target_block_interval_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub notes: Vec<String>,
}

pub async fn get_pow_metrics<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<PowMetricsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let best_height = chain.dag.best_height;
    let window_size = 10usize;

    let mut blocks = chain.dag.blocks.values().collect::<Vec<_>>();
    blocks.sort_by(|a, b| b.header.height.cmp(&a.header.height).then_with(|| b.header.timestamp.cmp(&a.header.timestamp)));
    let window = blocks.into_iter().take(window_size).collect::<Vec<_>>();
    let observed_block_count = window.len();

    let avg_block_interval_secs = if observed_block_count >= 2 {
        let newest = window.first().map(|b| b.header.timestamp).unwrap_or(0);
        let oldest = window.last().map(|b| b.header.timestamp).unwrap_or(0);
        newest.saturating_sub(oldest) / ((observed_block_count - 1) as u64)
    } else {
        0
    };

    let suggested_difficulty = pulsedag_core::dev_recommended_difficulty_for_chain(&chain);
    let target_u64 = pulsedag_core::dev_target_u64(suggested_difficulty);
    let target_block_interval_secs = pulsedag_core::dev_target_block_interval_secs();
    let retarget_multiplier_bps = pulsedag_core::dev_retarget_multiplier_bps(avg_block_interval_secs);

    Json(ApiResponse::ok(PowMetricsData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        window_size,
        best_height,
        observed_block_count,
        avg_block_interval_secs,
        suggested_difficulty,
        target_u64,
        target_block_interval_secs,
        retarget_multiplier_bps,
        notes: vec![
            "This is a development PoW metrics window".to_string(),
            format!("Suggested difficulty targets 1 block every {} seconds", target_block_interval_secs),
            format!("Current retarget multiplier is {} bps", retarget_multiplier_bps),
        ],
    }))
}
