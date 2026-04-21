use axum::Json;
use crate::api::ApiResponse;

#[derive(Debug, serde::Serialize)]
pub struct PolicyData {
    pub stage: String,
    pub mempool_policy: Vec<String>,
    pub transaction_rules: Vec<String>,
    pub block_rules: Vec<String>,
    pub dag_rules: Vec<String>,
}

pub async fn get_policy() -> Json<ApiResponse<PolicyData>> {
    Json(ApiResponse::ok(PolicyData {
        stage: "v1.1.0".to_string(),
        mempool_policy: vec![
            "reject double spends".into(),
            "require structurally valid transactions".into(),
            "keep pending transactions in in-memory mempool".into(),
            "prioritize visible fee-bearing transactions in explorer recent views".into(),
        ],
        transaction_rules: vec![
            "transaction must have a stable txid".into(),
            "inputs must reference spendable utxos".into(),
            "sum(inputs) must cover amount plus fee".into(),
            "coinbase transactions are only valid in mined blocks".into(),
        ],
        block_rules: vec![
            "block parents must be declared".into(),
            "block height must follow dag best height progression".into(),
            "coinbase reward is included by miner address".into(),
            "block acceptance mutates chain state only after validation".into(),
        ],
        dag_rules: vec![
            "genesis block must exist".into(),
            "tips set must not be empty".into(),
            "persisted and in-memory blocks should remain aligned".into(),
            "sync diagnostics should be checked before release".into(),
        ],
    }))
}
