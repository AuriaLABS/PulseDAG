use crate::types::{Block, BlockHeader, Transaction, TxOutput};

pub fn current_ts() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

pub fn is_coinbase(tx: &Transaction) -> bool {
    tx.inputs.is_empty() && tx.outputs.len() == 1 && tx.fee == 0
}

pub fn build_coinbase_transaction(miner_address: &str, reward: u64, nonce: u64) -> Transaction {
    Transaction {
        txid: format!("coinbase-{miner_address}-{nonce}"),
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput { address: miner_address.to_string(), amount: reward }],
        fee: 0,
        nonce,
    }
}

pub fn build_candidate_block(parents: Vec<String>, height: u64, difficulty: u32, txs: Vec<Transaction>) -> Block {
    Block {
        hash: format!("block-{height}"),
        header: BlockHeader {
            version: 1,
            parents,
            timestamp: current_ts(),
            difficulty,
            nonce: 0,
            merkle_root: format!("merkle-{height}"),
            state_root: format!("state-{height}"),
            blue_score: height,
            height,
        },
        transactions: txs,
    }
}
