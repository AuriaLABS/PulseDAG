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

pub async fn get_pow_metrics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<PowMetricsData>> {
    let chain = state.chain().read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);

    Json(ApiResponse::ok(PowMetricsData {
        algorithm: snapshot.algorithm.to_string(),
        window_size: snapshot.policy.window_size,
        best_height: snapshot.best_height,
        observed_block_count: snapshot.observed_block_count,
        avg_block_interval_secs: snapshot.avg_block_interval_secs,
        suggested_difficulty: snapshot.suggested_difficulty,
        target_u64: snapshot.target_u64,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        notes: vec![
            "This is a development PoW metrics window".to_string(),
            format!(
                "Suggested difficulty targets 1 block every {} seconds",
                snapshot.policy.target_block_interval_secs
            ),
            format!(
                "Current retarget multiplier is {} bps",
                snapshot.retarget_multiplier_bps
            ),
            format!(
                "Timestamp smoothing uses median: {}",
                snapshot.policy.use_median
            ),
        ],
    }))
}
