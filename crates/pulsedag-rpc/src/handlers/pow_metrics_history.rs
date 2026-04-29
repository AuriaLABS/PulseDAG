use crate::api::ApiResponse;
use axum::Json;
use std::fs;

#[derive(Debug, serde::Serialize)]
pub struct PowMetricsHistoryItem {
    pub path: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PowMetricsHistoryData {
    pub count: usize,
    pub items: Vec<PowMetricsHistoryItem>,
}

pub async fn get_pow_metrics_history() -> Json<ApiResponse<PowMetricsHistoryData>> {
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir("./data/metrics") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("pow-") && name.ends_with(".json") && name != "pow-latest.json"
                {
                    items.push(PowMetricsHistoryItem {
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));
    let count = items.len();
    Json(ApiResponse::ok(PowMetricsHistoryData { count, items }))
}
