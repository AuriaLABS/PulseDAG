use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct PowPolicyData {
    pub algorithm: String,
    pub best_height: u64,
    pub current_dev_difficulty: u64,
    pub recommended_dev_difficulty: u64,
    pub target_u64: u64,
    pub target_block_interval_secs: u64,
    pub max_future_drift_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub notes: Vec<String>,
}

pub async fn get_pow_policy<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<PowPolicyData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let best_height = chain.dag.best_height;
    let recommended_dev_difficulty = pulsedag_core::dev_recommended_difficulty_for_chain(&chain);
    let target_u64 = pulsedag_core::dev_target_u64(recommended_dev_difficulty);
    let target_block_interval_secs = pulsedag_core::dev_target_block_interval_secs();
    let max_future_drift_secs = pulsedag_core::dev_max_future_drift_secs();
    let window_size = std::env::var("PULSEDAG_DIFFICULTY_WINDOW").ok().and_then(|v| v.parse::<usize>().ok()).filter(|v| *v > 1).unwrap_or(10);
    let avg_block_interval_secs = { let v = pulsedag_core::dev_recent_avg_block_interval_secs(&chain, window_size); if v == 0 { target_block_interval_secs } else { v } };
    let retarget_multiplier_bps = pulsedag_core::dev_retarget_multiplier_bps(avg_block_interval_secs);

    Json(ApiResponse::ok(PowPolicyData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        best_height,
        current_dev_difficulty: recommended_dev_difficulty,
        recommended_dev_difficulty,
        target_u64,
        target_block_interval_secs,
        max_future_drift_secs,
        retarget_multiplier_bps,
        notes: vec![
            "This is a development difficulty policy".to_string(),
            format!("Current target is 1 block every {} seconds", target_block_interval_secs).to_string(),
            format!("Maximum future timestamp drift is {} seconds", max_future_drift_secs),
            format!("Current retarget multiplier is {} bps", retarget_multiplier_bps),
            "Difficulty adjusts proportionally around the recent average interval while a fuller retarget model is prepared".to_string(),
        ],
    }))
}
