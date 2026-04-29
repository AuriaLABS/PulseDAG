use crate::{
    api::{ApiResponse, ConfigureMiningWorkerRequest, SubmitMiningShareRequest},
    handlers::{mining_accounting::credit_share, mining_jobs::load_job},
};
use axum::Json;
use pulsedag_core::{dev_hash_score_u64, dev_target_u64};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MiningWorkerConfigRecord {
    pub worker_id: String,
    pub share_difficulty: u64,
    pub updated_at_unix: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MiningShareSubmitData {
    pub accepted: bool,
    pub worker_id: String,
    pub job_id: String,
    pub share_difficulty: u64,
    pub share_target_u64: u64,
    pub hash_score_u64: u64,
}

pub async fn post_configure_mining_worker(
    Json(req): Json<ConfigureMiningWorkerRequest>,
) -> Json<ApiResponse<MiningWorkerConfigRecord>> {
    let record = MiningWorkerConfigRecord {
        worker_id: req.worker_id,
        share_difficulty: req.share_difficulty.max(1),
        updated_at_unix: unix_now(),
    };
    let _ = persist_worker_config(&record);
    Json(ApiResponse::ok(record))
}

pub async fn post_submit_mining_share(
    Json(req): Json<SubmitMiningShareRequest>,
) -> Json<ApiResponse<MiningShareSubmitData>> {
    let cfg = load_worker_config(&req.worker_id).unwrap_or(MiningWorkerConfigRecord {
        worker_id: req.worker_id.clone(),
        share_difficulty: 1,
        updated_at_unix: unix_now(),
    });
    let hash_score_u64 = dev_hash_score_u64(&req.header);
    let share_target_u64 = dev_target_u64(cfg.share_difficulty);
    let accepted = hash_score_u64 <= share_target_u64;
    if accepted {
        let _ = persist_share(
            &req.worker_id,
            &req.job_id,
            hash_score_u64,
            cfg.share_difficulty,
        );
        if let Some(job) = load_job(&req.job_id) {
            let _ = credit_share(&req.worker_id, &job.miner_address);
        }
    }
    Json(ApiResponse::ok(MiningShareSubmitData {
        accepted,
        worker_id: req.worker_id,
        job_id: req.job_id,
        share_difficulty: cfg.share_difficulty,
        share_target_u64,
        hash_score_u64,
    }))
}

pub fn load_worker_config(worker_id: &str) -> Option<MiningWorkerConfigRecord> {
    let path = worker_config_dir().join(format!("{}.json", sanitize(worker_id)));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn persist_worker_config(record: &MiningWorkerConfigRecord) -> std::io::Result<()> {
    let dir = worker_config_dir();
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{}.json", sanitize(&record.worker_id))),
        serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
}

fn persist_share(
    worker_id: &str,
    job_id: &str,
    hash_score_u64: u64,
    share_difficulty: u64,
) -> std::io::Result<()> {
    let dir = PathBuf::from("./data/mining-shares");
    fs::create_dir_all(&dir)?;
    let ts = unix_now();
    let record = serde_json::json!({
        "worker_id": worker_id,
        "job_id": job_id,
        "hash_score_u64": hash_score_u64,
        "share_difficulty": share_difficulty,
        "created_at_unix": ts,
    });
    fs::write(
        dir.join(format!(
            "share-{}-{}-{}.json",
            sanitize(worker_id),
            sanitize(job_id),
            ts
        )),
        serde_json::to_vec_pretty(&record).unwrap_or_default(),
    )
}

fn worker_config_dir() -> PathBuf {
    PathBuf::from("./data/mining-worker-config")
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

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
