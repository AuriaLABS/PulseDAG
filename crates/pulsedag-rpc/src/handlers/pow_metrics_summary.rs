use std::fs;
use axum::Json;
use crate::api::ApiResponse;

#[derive(Debug, serde::Serialize)]
pub struct PowMetricsSummaryData {
    pub snapshot_count: usize,
    pub avg_suggested_difficulty: f64,
    pub min_suggested_difficulty: u64,
    pub max_suggested_difficulty: u64,
    pub avg_block_interval_secs: f64,
}

pub async fn get_pow_metrics_summary() -> Json<ApiResponse<PowMetricsSummaryData>> {
    let mut difficulties = Vec::new();
    let mut intervals = Vec::new();

    if let Ok(entries) = fs::read_dir("./data/metrics") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("pow-") && name.ends_with(".json") && name != "pow-latest.json" {
                    if let Ok(bytes) = fs::read(&path) {
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Some(d) = value.get("suggested_difficulty").and_then(|v| v.as_u64()) {
                                difficulties.push(d);
                            }
                            if let Some(i) = value.get("avg_block_interval_secs").and_then(|v| v.as_u64()) {
                                intervals.push(i);
                            }
                        }
                    }
                }
            }
        }
    }

    let snapshot_count = difficulties.len().max(intervals.len());
    let avg_suggested_difficulty = if difficulties.is_empty() { 0.0 } else { difficulties.iter().copied().sum::<u64>() as f64 / difficulties.len() as f64 };
    let min_suggested_difficulty = difficulties.iter().copied().min().unwrap_or(0);
    let max_suggested_difficulty = difficulties.iter().copied().max().unwrap_or(0);
    let avg_block_interval_secs = if intervals.is_empty() { 0.0 } else { intervals.iter().copied().sum::<u64>() as f64 / intervals.len() as f64 };

    Json(ApiResponse::ok(PowMetricsSummaryData {
        snapshot_count,
        avg_suggested_difficulty,
        min_suggested_difficulty,
        max_suggested_difficulty,
        avg_block_interval_secs,
    }))
}
