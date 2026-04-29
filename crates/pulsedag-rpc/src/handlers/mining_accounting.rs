use crate::api::ApiResponse;
use axum::{extract::Path, Json};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct WorkerAccountingRecord {
    pub worker_id: String,
    pub miner_address: String,
    pub accepted_shares: u64,
    pub accepted_blocks: u64,
    pub pending_balance_units: u64,
    pub paid_balance_units: u64,
    pub last_credit_unix: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MiningAccountingData {
    pub worker_count: usize,
    pub total_pending_balance_units: u64,
    pub total_paid_balance_units: u64,
    pub workers: Vec<WorkerAccountingRecord>,
}

pub async fn get_mining_accounting() -> Json<ApiResponse<MiningAccountingData>> {
    let mut workers = load_all_records();
    workers.sort_by(|a, b| b.pending_balance_units.cmp(&a.pending_balance_units));
    let total_pending_balance_units = workers.iter().map(|w| w.pending_balance_units).sum();
    let total_paid_balance_units = workers.iter().map(|w| w.paid_balance_units).sum();
    Json(ApiResponse::ok(MiningAccountingData {
        worker_count: workers.len(),
        total_pending_balance_units,
        total_paid_balance_units,
        workers,
    }))
}

pub async fn get_mining_accounting_worker(
    Path(worker_id): Path<String>,
) -> Json<ApiResponse<WorkerAccountingRecord>> {
    match load_record(&worker_id) {
        Some(record) => Json(ApiResponse::ok(record)),
        None => Json(ApiResponse::err(
            "ACCOUNT_NOT_FOUND",
            format!("worker accounting not found: {}", worker_id),
        )),
    }
}

pub fn credit_share(worker_id: &str, miner_address: &str) -> std::io::Result<()> {
    let mut record = load_record(worker_id).unwrap_or_else(|| WorkerAccountingRecord {
        worker_id: worker_id.to_string(),
        miner_address: miner_address.to_string(),
        ..Default::default()
    });
    record.miner_address = miner_address.to_string();
    record.accepted_shares += 1;
    record.pending_balance_units += 1;
    record.last_credit_unix = unix_now();
    persist_record(&record)
}

pub fn credit_block(worker_id: &str, miner_address: &str) -> std::io::Result<()> {
    let mut record = load_record(worker_id).unwrap_or_else(|| WorkerAccountingRecord {
        worker_id: worker_id.to_string(),
        miner_address: miner_address.to_string(),
        ..Default::default()
    });
    record.miner_address = miner_address.to_string();
    record.accepted_blocks += 1;
    record.pending_balance_units += 100;
    record.last_credit_unix = unix_now();
    persist_record(&record)
}

fn load_all_records() -> Vec<WorkerAccountingRecord> {
    let dir = accounting_dir();
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(bytes) = fs::read(entry.path()) {
                if let Ok(record) = serde_json::from_slice::<WorkerAccountingRecord>(&bytes) {
                    out.push(record);
                }
            }
        }
    }
    out
}

fn load_record(worker_id: &str) -> Option<WorkerAccountingRecord> {
    let path = accounting_dir().join(format!("{}.json", sanitize(worker_id)));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn persist_record(record: &WorkerAccountingRecord) -> std::io::Result<()> {
    let dir = accounting_dir();
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{}.json", sanitize(&record.worker_id))),
        serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
}

fn accounting_dir() -> PathBuf {
    PathBuf::from("./data/mining-accounting")
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

pub fn load_all_records_public() -> Vec<WorkerAccountingRecord> {
    load_all_records()
}
