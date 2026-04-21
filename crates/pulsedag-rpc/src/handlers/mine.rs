use axum::{extract::State, Json};
use pulsedag_core::{accept_block, dev_mine_header, dev_pow_accepts, dev_recommended_difficulty_for_chain, dev_surrogate_pow_hash, dev_target_u64, mining::{build_candidate_block, build_coinbase_transaction}, AcceptSource};
use crate::{api::{ApiResponse, MineRequest, RpcStateLike}};


fn suggested_difficulty_from_recent(chain: &pulsedag_core::ChainState) -> u64 {
    dev_recommended_difficulty_for_chain(chain)
}

#[derive(Debug, serde::Serialize)]
pub struct MineData { pub block_hash: String, pub height: u64, pub tx_count: usize, pub coinbase_amount: u64, pub parents: Vec<String>, pub pow_hash: String, pub pow_target_u64: u64, pub pow_accepted_dev: bool, pub pow_tries: u64, pub final_nonce: u64, pub difficulty: u64 }

#[derive(Debug, serde::Serialize)]
pub struct MinePreviewData {
    pub next_height: u64,
    pub parent_hashes: Vec<String>,
    pub mempool_tx_count: usize,
    pub candidate_tx_count: usize,
    pub coinbase_amount: u64,
    pub miner_address: String,
    pub pow_hash: String,
    pub pow_target_u64: u64,
    pub pow_accepted_dev: bool,
    pub pow_tries: u64,
    pub final_nonce: u64,
    pub difficulty: u64,
}

pub async fn post_mine_preview<S: RpcStateLike>(State(state): State<S>, Json(req): Json<MineRequest>) -> Json<ApiResponse<MinePreviewData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let parent_hashes = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    let next_height = chain.dag.best_height + 1;
    let coinbase_amount = 50;
    let mempool_tx_count = chain.mempool.transactions.len();
    let difficulty = suggested_difficulty_from_recent(&chain);
    let candidate = build_candidate_block(parent_hashes.clone(), next_height, difficulty as u32, vec![build_coinbase_transaction(&req.miner_address, coinbase_amount, next_height)]);
    let max_tries = req.pow_max_tries.unwrap_or(10_000).min(1_000_000);
    let (mined_header, _accepted, pow_tries, pow_hash) = dev_mine_header(candidate.header.clone(), max_tries);
    let pow_target_u64 = dev_target_u64(mined_header.difficulty as u64);
    let pow_accepted_dev = dev_pow_accepts(&mined_header);
    let final_nonce = mined_header.nonce;

    Json(ApiResponse::ok(MinePreviewData {
        next_height,
        parent_hashes,
        mempool_tx_count,
        candidate_tx_count: mempool_tx_count + 1,
        coinbase_amount,
        miner_address: req.miner_address,
        pow_hash,
        pow_target_u64,
        pow_accepted_dev,
        pow_tries,
        final_nonce,
        difficulty,
    }))
}

pub async fn post_mine<S: RpcStateLike>(State(state): State<S>, Json(req): Json<MineRequest>) -> Json<ApiResponse<MineData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    let height = chain.dag.best_height + 1;
    let reward = 50;
    let difficulty = suggested_difficulty_from_recent(&chain);
    let mut txs = vec![build_coinbase_transaction(&req.miner_address, reward, height)];
    txs.extend(chain.mempool.transactions.values().cloned());
    let mut block = build_candidate_block(parents.clone(), height, difficulty as u32, txs);
    let max_tries = req.pow_max_tries.unwrap_or(10_000).min(1_000_000);
    let (mined_header, _accepted, pow_tries, pow_hash) = dev_mine_header(block.header.clone(), max_tries);
    block.header = mined_header;
    match accept_block(block.clone(), &mut chain, AcceptSource::LocalMining) {
        Ok(_) => {
            let snapshot = chain.clone();
            drop(chain);
            if let Err(e) = state.storage().persist_block(&block) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Err(e) = state.storage().persist_chain_state(&snapshot) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Some(p2p) = state.p2p() { let _ = p2p.broadcast_block(&block); }
            let pow_target_u64 = dev_target_u64(block.header.difficulty as u64);
            let pow_accepted_dev = dev_pow_accepts(&block.header);
            let final_nonce = block.header.nonce;
            Json(ApiResponse::ok(MineData { block_hash: block.hash, height, tx_count: block.transactions.len(), coinbase_amount: reward, parents, pow_hash, pow_target_u64, pow_accepted_dev, pow_tries, final_nonce, difficulty }))
        }
        Err(e) => Json(ApiResponse::err("MINE_ERROR", e.to_string())),
    }
}
