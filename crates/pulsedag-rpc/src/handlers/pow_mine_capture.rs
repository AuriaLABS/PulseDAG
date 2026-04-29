use crate::api::{ApiResponse, MineRequest, RpcStateLike};
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

#[derive(Debug, serde::Serialize)]
pub struct PowMineCaptureData {
    pub block_hash: String,
    pub height: u64,
    pub difficulty: u64,
    pub pow_accepted_dev: bool,
    pub pow_tries: u64,
    pub final_nonce: u64,
    pub snapshot_path: String,
}

fn suggested_difficulty_from_recent(chain: &pulsedag_core::ChainState) -> u64 {
    dev_recommended_difficulty_for_chain(chain)
}

pub async fn post_pow_mine_capture<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<MineRequest>,
) -> Json<ApiResponse<PowMineCaptureData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let height = chain.dag.best_height + 1;
    let parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    let reward = 50;
    let difficulty = suggested_difficulty_from_recent(&chain);
    let mut txs = vec![build_coinbase_transaction(
        &req.miner_address,
        reward,
        height,
    )];
    txs.extend(chain.mempool.transactions.values().cloned());
    let mut block = build_candidate_block(parents, height, difficulty as u32, txs);
    let max_tries = req.pow_max_tries.unwrap_or(10_000).min(1_000_000);
    let (mined_header, _accepted, pow_tries, _pow_hash) =
        dev_mine_header(block.header.clone(), max_tries);
    block.header = mined_header;

    match accept_block(block.clone(), &mut chain, AcceptSource::LocalMining) {
        Ok(_) => {
            let pow_accepted_dev = dev_pow_accepts(&block.header);
            let final_nonce = block.header.nonce;
            let snapshot_dir = PathBuf::from("./data/metrics");
            let _ = fs::create_dir_all(&snapshot_dir);
            let snapshot_path = snapshot_dir.join(format!(
                "pow-mine-{}.json",
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
                "final_nonce": final_nonce,
                "target_u64": dev_target_u64(difficulty),
            });
            let _ = fs::write(
                &snapshot_path,
                serde_json::to_vec_pretty(&payload).unwrap_or_default(),
            );
            Json(ApiResponse::ok(PowMineCaptureData {
                block_hash: block.hash,
                height,
                difficulty,
                pow_accepted_dev,
                pow_tries,
                final_nonce,
                snapshot_path: snapshot_path.to_string_lossy().to_string(),
            }))
        }
        Err(e) => Json(ApiResponse::err("MINE_CAPTURE_ERROR", e.to_string())),
    }
}
