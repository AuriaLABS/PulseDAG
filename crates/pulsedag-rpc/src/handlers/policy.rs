use axum::{extract::State, Json};

use crate::{
    api::{ApiResponse, RpcStateLike},
    handlers::release::operator_stage,
};

#[derive(Debug, serde::Serialize)]
pub struct PolicyData {
    pub stage: String,
    pub mempool_policy: Vec<String>,
    pub transaction_rules: Vec<String>,
    pub block_rules: Vec<String>,
    pub dag_rules: Vec<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
}

pub async fn get_policy<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<PolicyData>> {
    let chain = state.chain().read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);

    Json(ApiResponse::ok(PolicyData {
        stage: operator_stage().to_string(),
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
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
    }))
}
