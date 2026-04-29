use crate::api::{ApiResponse, RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_block, dev_mine_header, dev_pow_accepts, dev_recommended_difficulty_for_chain,
    dev_target_u64,
    mining::{build_candidate_block, build_coinbase_transaction},
    AcceptSource,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, serde::Deserialize)]
pub struct PowAutoRunRequest {
    pub miner_address: String,
    pub rounds: Option<u64>,
    pub pow_max_tries: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct PowAutoRunItem {
    pub block_hash: String,
    pub height: u64,
    pub difficulty: u64,
    pub pow_accepted_dev: bool,
    pub pow_tries: u64,
    pub snapshot_path: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PowAutoRunData {
    pub rounds: u64,
    pub mined: u64,
    pub items: Vec<PowAutoRunItem>,
}

fn suggested_difficulty_from_recent(chain: &pulsedag_core::ChainState) -> u64 {
    dev_recommended_difficulty_for_chain(chain)
}

pub async fn post_pow_auto_run<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<PowAutoRunRequest>,
) -> Json<ApiResponse<PowAutoRunData>> {
    let rounds = req.rounds.unwrap_or(3).clamp(1, 25);
    let max_tries = req.pow_max_tries.unwrap_or(10_000).min(1_000_000);
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let mut items = Vec::new();

    for _ in 0..rounds {
        let height = chain.dag.best_height + 1;
        let parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        let reward = 50;
        let difficulty = suggested_difficulty_from_recent(&chain);
        let txs = vec![build_coinbase_transaction(
            &req.miner_address,
            reward,
            height,
        )];
        let mut block = build_candidate_block(parents, height, difficulty as u32, txs);
        let (mined_header, _accepted, pow_tries, _pow_hash) =
            dev_mine_header(block.header.clone(), max_tries);
        block.header = mined_header;

        match accept_block(block.clone(), &mut chain, AcceptSource::LocalMining) {
            Ok(_) => {
                let pow_accepted_dev = dev_pow_accepts(&block.header);
                let snapshot_dir = PathBuf::from("./data/metrics");
                let _ = fs::create_dir_all(&snapshot_dir);
                let snapshot_path = snapshot_dir.join(format!(
                    "pow-auto-{}-{}.json",
                    height,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                ));
                let payload = serde_json::json!({
                    "block_hash": block.hash,
                    "height": height,
                    "difficulty": difficulty,
                    "pow_accepted_dev": pow_accepted_dev,
                    "pow_tries": pow_tries,
                    "final_nonce": block.header.nonce,
                    "target_u64": dev_target_u64(difficulty),
                });
                let _ = fs::write(
                    &snapshot_path,
                    serde_json::to_vec_pretty(&payload).unwrap_or_default(),
                );
                items.push(PowAutoRunItem {
                    block_hash: block.hash,
                    height,
                    difficulty,
                    pow_accepted_dev,
                    pow_tries,
                    snapshot_path: snapshot_path.to_string_lossy().to_string(),
                });
            }
            Err(e) => return Json(ApiResponse::err("AUTO_RUN_ERROR", e.to_string())),
        }
    }

    let mined = items.len() as u64;
    Json(ApiResponse::ok(PowAutoRunData {
        rounds,
        mined,
        items,
    }))
}
