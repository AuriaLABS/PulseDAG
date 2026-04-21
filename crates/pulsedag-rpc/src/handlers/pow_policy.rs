use axum::{extract::State, Json};

use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct PowPolicyData {
    pub algorithm: String,
    pub best_height: u64,
    pub current_dev_difficulty: u64,
    pub recommended_dev_difficulty: u64,
    pub suggested_difficulty: u64,
    pub target_u64: u64,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub max_future_drift_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub notes: Vec<String>,
}

pub async fn get_pow_policy<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<PowPolicyData>> {
    let chain = state.chain().read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);

    Json(ApiResponse::ok(PowPolicyData {
        algorithm: snapshot.algorithm.to_string(),
        best_height: snapshot.best_height,
        current_dev_difficulty: snapshot.current_difficulty,
        recommended_dev_difficulty: snapshot.suggested_difficulty,
        suggested_difficulty: snapshot.suggested_difficulty,
        target_u64: snapshot.target_u64,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        max_future_drift_secs: snapshot.policy.max_future_drift_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        notes: vec![
            "This is a development difficulty policy".to_string(),
            format!(
                "Current target is 1 block every {} seconds",
                snapshot.policy.target_block_interval_secs
            ),
            format!(
                "Maximum future timestamp drift is {} seconds",
                snapshot.policy.max_future_drift_secs
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
