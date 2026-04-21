use crate::{
    api::{ApiResponse, ClaimMiningJobRequest, RpcStateLike, SubmitMiningJobRequest},
    handlers::mining_accounting::credit_block,
    handlers::mining_pool::load_worker_config,
    handlers::pow_metrics::PowMetricsData,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_block, build_candidate_block, build_coinbase_transaction, dev_difficulty_snapshot,
    dev_pow_accepts, AcceptSource,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MiningJobRecord {
    pub job_id: String,
    pub worker_id: String,
    pub miner_address: String,
    pub template_id: String,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
    pub submitted: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct ClaimMiningJobData {
    pub mode: String,
    pub job_id: String,
    pub template_id: String,
    pub worker_id: String,
    pub expires_at_unix: u64,
    pub block: pulsedag_core::types::Block,
    pub target_u64: u64,
    pub share_difficulty: u64,
    pub metrics_hint: PowMetricsData,
}

#[derive(Debug, serde::Serialize)]
pub struct CleanupMiningJobsData {
    pub removed_jobs: usize,
    pub removed_job_ids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SubmitMiningJobData {
    pub accepted: bool,
    pub job_id: String,
    pub block_hash: String,
    pub height: u64,
    pub pow_accepted_dev: bool,
}

pub async fn post_claim_mining_job<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<ClaimMiningJobRequest>,
) -> Json<ApiResponse<ClaimMiningJobData>> {
    let _ = cleanup_expired_jobs();
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let height = chain.dag.best_height + 1;
    let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    parents.sort();
    let snapshot = dev_difficulty_snapshot(&chain);
    let difficulty = snapshot.suggested_difficulty;
    let reward = 50;
    let template_id = format!("{}:{}", height, parents.join(","));
    let mut txs = vec![build_coinbase_transaction(
        &req.miner_address,
        reward,
        height,
    )];
    txs.extend(chain.mempool.transactions.values().cloned());
    let header_difficulty = u32::try_from(difficulty).unwrap_or(u32::MAX);
    let block = build_candidate_block(parents, height, header_difficulty, txs);
    let now = unix_now();
    let job_id = format!("job-{}-{}", req.worker_id, now);
    let expires_at_unix = now + 30;
    let share_difficulty = load_worker_config(&req.worker_id)
        .map(|c| c.share_difficulty)
        .unwrap_or(1);
    let record = MiningJobRecord {
        job_id: job_id.clone(),
        worker_id: req.worker_id.clone(),
        miner_address: req.miner_address.clone(),
        template_id: template_id.clone(),
        created_at_unix: now,
        expires_at_unix,
        submitted: false,
    };
    let _ = persist_job(&record);

    let metrics_hint = PowMetricsData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        best_height: chain.dag.best_height,
        window_size: snapshot.policy.window_size,
        observed_block_count: snapshot.observed_block_count,
        avg_block_interval_secs: snapshot.avg_block_interval_secs,
        suggested_difficulty: snapshot.suggested_difficulty,
        target_u64: snapshot.target_u64,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        notes: vec!["Mining template uses centralized runtime retarget policy".to_string()],
    };

    Json(ApiResponse::ok(ClaimMiningJobData {
        mode: "job-claim".to_string(),
        job_id,
        template_id,
        worker_id: req.worker_id,
        expires_at_unix,
        block,
        target_u64: snapshot.target_u64,
        share_difficulty,
        metrics_hint,
    }))
}

pub async fn post_submit_mining_job<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitMiningJobRequest>,
) -> Json<ApiResponse<SubmitMiningJobData>> {
    let Some(mut record) = load_job(&req.job_id) else {
        return Json(ApiResponse::err(
            "UNKNOWN_JOB",
            format!("job not found: {}", req.job_id),
        ));
    };
    if record.worker_id != req.worker_id {
        return Json(ApiResponse::err(
            "JOB_WORKER_MISMATCH",
            "job does not belong to this worker".to_string(),
        ));
    }
    if record.submitted {
        return Json(ApiResponse::err(
            "JOB_ALREADY_SUBMITTED",
            "job already submitted".to_string(),
        ));
    }
    if unix_now() > record.expires_at_unix {
        return Json(ApiResponse::err(
            "JOB_EXPIRED",
            "job expired before submission".to_string(),
        ));
    }
    if !dev_pow_accepts(&req.block.header) {
        return Json(ApiResponse::err(
            "INVALID_POW",
            "submitted block does not satisfy current dev pow check".to_string(),
        ));
    }

    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    if req.block.header.height <= chain.dag.best_height {
        return Json(ApiResponse::err(
            "STALE_JOB",
            format!(
                "stale job: current best height is {} and submitted block height is {}",
                chain.dag.best_height, req.block.header.height
            ),
        ));
    }

    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;
    match accept_block(req.block, &mut chain, AcceptSource::Rpc) {
        Ok(_) => {
            record.submitted = true;
            let _ = persist_job(&record);
            let _ = credit_block(&record.worker_id, &record.miner_address);
            Json(ApiResponse::ok(SubmitMiningJobData {
                accepted: true,
                job_id: req.job_id,
                block_hash,
                height,
                pow_accepted_dev: true,
            }))
        }
        Err(e) => Json(ApiResponse::err("SUBMIT_JOB_ERROR", e.to_string())),
    }
}

fn jobs_dir() -> PathBuf {
    PathBuf::from("./data/mining-jobs")
}

fn persist_job(record: &MiningJobRecord) -> std::io::Result<()> {
    let dir = jobs_dir();
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{}.json", sanitize(&record.job_id))),
        serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
}

pub fn load_job(job_id: &str) -> Option<MiningJobRecord> {
    let path = jobs_dir().join(format!("{}.json", sanitize(job_id)));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
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

pub async fn post_cleanup_mining_jobs() -> Json<ApiResponse<CleanupMiningJobsData>> {
    let dir = jobs_dir();
    let mut removed_job_ids = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(bytes) = fs::read(entry.path()) {
                if let Ok(record) = serde_json::from_slice::<MiningJobRecord>(&bytes) {
                    if !record.submitted && unix_now() > record.expires_at_unix {
                        let _ = fs::remove_file(entry.path());
                        removed_job_ids.push(record.job_id);
                    }
                }
            }
        }
    }
    Json(ApiResponse::ok(CleanupMiningJobsData {
        removed_jobs: removed_job_ids.len(),
        removed_job_ids,
    }))
}

fn cleanup_expired_jobs() -> usize {
    let dir = jobs_dir();
    let mut removed = 0usize;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(bytes) = fs::read(entry.path()) {
                if let Ok(record) = serde_json::from_slice::<MiningJobRecord>(&bytes) {
                    if !record.submitted && unix_now() > record.expires_at_unix {
                        if fs::remove_file(entry.path()).is_ok() {
                            removed += 1;
                        }
                    }
                }
            }
        }
    }
    removed
}
