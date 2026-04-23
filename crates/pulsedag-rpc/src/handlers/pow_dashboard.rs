use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use std::fs;

#[derive(Debug, serde::Serialize)]
pub struct PowDashboardData {
    pub algorithm: String,
    pub best_height: u64,
    pub suggested_difficulty: u64,
    pub target_u64: u64,
    pub target_block_interval_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub avg_block_interval_secs: u64,
    pub snapshot_count: usize,
    pub health_status: String,
}

pub async fn get_pow_dashboard<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<PowDashboardData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let best_height = snapshot.best_height;
    let suggested_difficulty = snapshot.suggested_difficulty;
    let target_u64 = snapshot.target_u64;
    let target_block_interval_secs = snapshot.policy.target_block_interval_secs;

    let mut avg_block_interval_secs = snapshot.avg_block_interval_secs;
    let mut health_status = "ok".to_string();
    if let Ok(bytes) = fs::read("./data/metrics/pow-latest.json") {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            avg_block_interval_secs = value
                .get("avg_block_interval_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if avg_block_interval_secs > 90
                || (avg_block_interval_secs > 0 && avg_block_interval_secs < 30)
            {
                health_status = "warn".to_string();
            }
        }
    } else {
        health_status = "degraded".to_string();
    }

    let mut snapshot_count = 0usize;
    if let Ok(entries) = fs::read_dir("./data/metrics") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("pow-") && name.ends_with(".json") {
                    snapshot_count += 1;
                }
            }
        }
    }

    let retarget_multiplier_bps =
        pulsedag_core::dev_retarget_multiplier_bps(avg_block_interval_secs);

    Json(ApiResponse::ok(PowDashboardData {
        algorithm: snapshot.algorithm.to_string(),
        best_height,
        suggested_difficulty,
        target_u64,
        target_block_interval_secs,
        retarget_multiplier_bps,
        avg_block_interval_secs,
        snapshot_count,
        health_status,
    }))
}
