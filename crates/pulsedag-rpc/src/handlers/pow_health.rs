use crate::api::ApiResponse;
use axum::Json;
use std::fs;

#[derive(Debug, serde::Serialize)]
pub struct PowHealthData {
    pub status: String,
    pub snapshot_count: usize,
    pub latest_suggested_difficulty: u64,
    pub latest_avg_block_interval_secs: u64,
    pub alerts: Vec<String>,
}

pub async fn get_pow_health() -> Json<ApiResponse<PowHealthData>> {
    let mut snapshot_count = 0usize;
    let mut latest_suggested_difficulty = 0u64;
    let mut latest_avg_block_interval_secs = 0u64;
    let mut alerts = Vec::new();

    if let Ok(bytes) = fs::read("./data/metrics/pow-latest.json") {
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
        alerts.push("missing latest PoW metrics snapshot".to_string());
    }

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

    if snapshot_count == 0 {
        alerts.push("no PoW snapshots captured yet".to_string());
    }
    if latest_avg_block_interval_secs > 90 {
        alerts.push("block production looks slow in recent window".to_string());
    }
    if latest_avg_block_interval_secs > 0 && latest_avg_block_interval_secs < 30 {
        alerts.push("block production looks too fast in recent window".to_string());
    }
    if latest_suggested_difficulty == 0 {
        alerts.push("difficulty suggestion unavailable".to_string());
    }

    let status = if alerts.is_empty() {
        "ok"
    } else if snapshot_count == 0 {
        "degraded"
    } else {
        "warn"
    }
    .to_string();

    Json(ApiResponse::ok(PowHealthData {
        status,
        snapshot_count,
        latest_suggested_difficulty,
        latest_avg_block_interval_secs,
        alerts,
    }))
}
