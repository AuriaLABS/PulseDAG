use crate::api::{ApiResponse, MiningWorkerHeartbeatRequest};
use axum::Json;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MiningWorkerRecord {
    pub worker_id: String,
    pub miner_address: String,
    pub templates_requested: u64,
    pub blocks_submitted: u64,
    pub accepted_blocks: u64,
    pub stale_rejections: u64,
    pub invalid_pow_rejections: u64,
    pub accepted_shares: u64,
    pub last_seen_unix: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MiningWorkersStatsData {
    pub worker_count: usize,
    pub workers: Vec<MiningWorkerRecord>,
}

pub async fn post_mining_worker_heartbeat(
    Json(req): Json<MiningWorkerHeartbeatRequest>,
) -> Json<ApiResponse<MiningWorkerRecord>> {
    let dir = PathBuf::from("./data/miners");
    let _ = fs::create_dir_all(&dir);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let record = MiningWorkerRecord {
        worker_id: req.worker_id.clone(),
        miner_address: req.miner_address,
        templates_requested: req.templates_requested,
        blocks_submitted: req.blocks_submitted,
        accepted_blocks: req.accepted_blocks,
        stale_rejections: req.stale_rejections,
        invalid_pow_rejections: req.invalid_pow_rejections,
        accepted_shares: req.accepted_shares,
        last_seen_unix: now,
    };
    let path = dir.join(format!("{}.json", sanitize(&req.worker_id)));
    let _ = fs::write(path, serde_json::to_vec_pretty(&record).unwrap_or_default());
    Json(ApiResponse::ok(record))
}

pub async fn get_mining_workers_stats() -> Json<ApiResponse<MiningWorkersStatsData>> {
    let dir = PathBuf::from("./data/miners");
    let mut workers = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(bytes) = fs::read(entry.path()) {
                if let Ok(record) = serde_json::from_slice::<MiningWorkerRecord>(&bytes) {
                    workers.push(record);
                }
            }
        }
    }
    workers.sort_by(|a, b| b.last_seen_unix.cmp(&a.last_seen_unix));
    let worker_count = workers.len();
    Json(ApiResponse::ok(MiningWorkersStatsData {
        worker_count,
        workers,
    }))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
