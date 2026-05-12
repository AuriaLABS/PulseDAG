use crate::{
    tx::compute_txid,
    types::{compute_block_hash, compute_merkle_root, Block, BlockHeader, Transaction, TxOutput},
};

pub fn current_ts() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub fn is_coinbase(tx: &Transaction) -> bool {
    tx.inputs.is_empty() && tx.outputs.len() == 1 && tx.fee == 0
}

pub fn build_coinbase_transaction(miner_address: &str, reward: u64, nonce: u64) -> Transaction {
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput {
            address: miner_address.to_string(),
            amount: reward,
        }],
        fee: 0,
        nonce,
    };
    tx.txid = compute_txid(&tx);
    tx
}

pub fn build_candidate_block(
    parents: Vec<String>,
    height: u64,
    difficulty: u32,
    txs: Vec<Transaction>,
) -> Block {
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: 1,
            parents,
            timestamp: current_ts(),
            difficulty,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: format!("state-{height}"),
            blue_score: height,
            height,
        },
        transactions: txs,
    };
    block.hash = compute_block_hash(&block.header);
    block
}

pub fn refresh_block_consensus_ids(block: &mut Block) {
    block.header.merkle_root = compute_merkle_root(&block.transactions);
    block.hash = compute_block_hash(&block.header);
}
