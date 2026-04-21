use std::fs;
use axum::Json;
use crate::api::ApiResponse;

#[derive(Debug, serde::Deserialize)]
pub struct PowMetricsPruneRequest {
    pub keep_last: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct PowMetricsPruneData {
    pub kept: usize,
    pub removed: usize,
    pub keep_last: usize,
}

pub async fn post_pow_metrics_prune(Json(req): Json<PowMetricsPruneRequest>) -> Json<ApiResponse<PowMetricsPruneData>> {
    let keep_last = req.keep_last.unwrap_or(20).max(1);
    let mut items = Vec::new();

    if let Ok(entries) = fs::read_dir("./data/metrics") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("pow-") && name.ends_with(".json") && name != "pow-latest.json" {
                    items.push(path);
                }
            }
        }
    }

    items.sort();
    let total = items.len();
    let to_remove = total.saturating_sub(keep_last);
    for path in items.iter().take(to_remove) {
        let _ = fs::remove_file(path);
    }

    Json(ApiResponse::ok(PowMetricsPruneData {
        kept: total.saturating_sub(to_remove),
        removed: to_remove,
        keep_last,
    }))
}
