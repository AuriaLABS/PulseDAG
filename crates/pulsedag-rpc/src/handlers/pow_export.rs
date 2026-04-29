use crate::api::ApiResponse;
use axum::Json;
use std::fs;

#[derive(Debug, serde::Serialize)]
pub struct PowExportData {
    pub latest_snapshot_path: String,
    pub history_count: usize,
    pub health_status: String,
    pub latest_suggested_difficulty: u64,
    pub latest_avg_block_interval_secs: u64,
}

pub async fn get_pow_export() -> Json<ApiResponse<PowExportData>> {
    let latest_snapshot_path = "./data/metrics/pow-latest.json".to_string();
    let mut history_count = 0usize;
    let mut latest_suggested_difficulty = 0u64;
    let mut latest_avg_block_interval_secs = 0u64;
    let mut health_status = "ok".to_string();

    if let Ok(bytes) = fs::read(&latest_snapshot_path) {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            latest_suggested_difficulty = value
                .get("suggested_difficulty")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            latest_avg_block_interval_secs = value
                .get("avg_block_interval_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        }
    } else {
        health_status = "degraded".to_string();
    }

    if let Ok(entries) = fs::read_dir("./data/metrics") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("pow-") && name.ends_with(".json") && name != "pow-latest.json"
                {
                    history_count += 1;
                }
            }
        }
    }

    if latest_avg_block_interval_secs > 90
        || (latest_avg_block_interval_secs > 0 && latest_avg_block_interval_secs < 30)
    {
        health_status = "warn".to_string();
    }

    Json(ApiResponse::ok(PowExportData {
        latest_snapshot_path,
        history_count,
        health_status,
        latest_suggested_difficulty,
        latest_avg_block_interval_secs,
    }))
}
