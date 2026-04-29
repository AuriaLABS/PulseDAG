use crate::{api::ApiResponse, handlers::mining_accounting::load_all_records_public};
use axum::Json;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct PayoutRecord {
    pub payout_id: String,
    pub worker_id: String,
    pub miner_address: String,
    pub amount_units: u64,
    pub created_at_unix: u64,
    pub status: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RunPayoutsData {
    pub payout_count: usize,
    pub total_amount_units: u64,
    pub payouts: Vec<PayoutRecord>,
}

#[derive(Debug, serde::Serialize)]
pub struct PayoutHistoryData {
    pub payout_count: usize,
    pub payouts: Vec<PayoutRecord>,
}

pub async fn post_run_payouts() -> Json<ApiResponse<RunPayoutsData>> {
    let mut payouts = Vec::new();
    let mut total_amount_units = 0u64;
    for mut record in load_all_records_public() {
        if record.pending_balance_units == 0 {
            continue;
        }
        let amount = record.pending_balance_units;
        record.pending_balance_units = 0;
        record.paid_balance_units += amount;
        record.last_credit_unix = unix_now();
        let _ = persist_accounting_record(&record);

        let payout = PayoutRecord {
            payout_id: format!("payout-{}-{}", record.worker_id, unix_now()),
            worker_id: record.worker_id.clone(),
            miner_address: record.miner_address.clone(),
            amount_units: amount,
            created_at_unix: unix_now(),
            status: "simulated-paid".to_string(),
        };
        total_amount_units += amount;
        let _ = persist_payout(&payout);
        payouts.push(payout);
    }
    Json(ApiResponse::ok(RunPayoutsData {
        payout_count: payouts.len(),
        total_amount_units,
        payouts,
    }))
}

pub async fn get_payout_history() -> Json<ApiResponse<PayoutHistoryData>> {
    let mut payouts = load_all_payouts();
    payouts.sort_by(|a, b| b.created_at_unix.cmp(&a.created_at_unix));
    Json(ApiResponse::ok(PayoutHistoryData {
        payout_count: payouts.len(),
        payouts,
    }))
}

fn persist_accounting_record(
    record: &crate::handlers::mining_accounting::WorkerAccountingRecord,
) -> std::io::Result<()> {
    let dir = PathBuf::from("./data/mining-accounting");
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{}.json", sanitize(&record.worker_id))),
        serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
}

fn persist_payout(record: &PayoutRecord) -> std::io::Result<()> {
    let dir = PathBuf::from("./data/mining-payouts");
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{}.json", sanitize(&record.payout_id))),
        serde_json::to_vec_pretty(record).unwrap_or_default(),
    )
}

fn load_all_payouts() -> Vec<PayoutRecord> {
    let dir = PathBuf::from("./data/mining-payouts");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(bytes) = fs::read(entry.path()) {
                if let Ok(record) = serde_json::from_slice::<PayoutRecord>(&bytes) {
                    out.push(record);
                }
            }
        }
    }
    out
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
