use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use std::{fs, path::PathBuf};

#[derive(Debug, serde::Serialize)]
pub struct PowMetricsCaptureData {
    pub algorithm: String,
    pub best_height: u64,
    pub observed_block_count: usize,
    pub avg_block_interval_secs: u64,
    pub suggested_difficulty: u64,
    pub target_u64: u64,
    pub target_block_interval_secs: u64,
    pub retarget_multiplier_bps: u64,
    pub persisted: bool,
    pub snapshot_path: String,
    pub history_snapshot_path: String,
}

pub async fn post_pow_metrics_capture<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<PowMetricsCaptureData>> {
    let chain = state.chain().read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let best_height = snapshot.best_height;
    let window_size = snapshot.policy.window_size;
    let observed_block_count = snapshot.observed_block_count;
    let avg_block_interval_secs = snapshot.avg_block_interval_secs;
    let suggested_difficulty = snapshot.suggested_difficulty;
    let target_u64 = snapshot.target_u64;
    let target_block_interval_secs = snapshot.policy.target_block_interval_secs;
    let retarget_multiplier_bps = snapshot.retarget_multiplier_bps;

    let snapshot_dir = PathBuf::from("./data/metrics");
    let _ = fs::create_dir_all(&snapshot_dir);
    let snapshot_path = snapshot_dir.join("pow-latest.json");
    let history_snapshot_path = snapshot_dir.join(format!("pow-{}.json", chrono_like_now()));
    let payload = serde_json::json!({
        "algorithm": pulsedag_core::selected_pow_name(),
        "best_height": best_height,
        "window_size": window_size,
        "observed_block_count": observed_block_count,
        "avg_block_interval_secs": avg_block_interval_secs,
        "suggested_difficulty": suggested_difficulty,
        "target_u64": target_u64,
        "target_block_interval_secs": target_block_interval_secs,
        "retarget_multiplier_bps": retarget_multiplier_bps,
    });
    let latest_ok = fs::write(
        &snapshot_path,
        serde_json::to_vec_pretty(&payload).unwrap_or_default(),
    )
    .is_ok();
    let history_ok = fs::write(
        &history_snapshot_path,
        serde_json::to_vec_pretty(&payload).unwrap_or_default(),
    )
    .is_ok();
    let persisted = latest_ok && history_ok;

    Json(ApiResponse::ok(PowMetricsCaptureData {
        algorithm: snapshot.algorithm.to_string(),
        best_height,
        observed_block_count,
        avg_block_interval_secs,
        suggested_difficulty,
        target_u64,
        target_block_interval_secs,
        retarget_multiplier_bps,
        persisted,
        snapshot_path: snapshot_path.to_string_lossy().to_string(),
        history_snapshot_path: history_snapshot_path.to_string_lossy().to_string(),
    }))
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}
