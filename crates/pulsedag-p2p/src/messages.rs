use serde::{Deserialize, Serialize};

use pulsedag_core::types::{Block, Hash, Transaction};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NetworkMessage {
    NewTransaction {
        chain_id: String,
        transaction: Transaction,
    },
    NewBlock {
        chain_id: String,
        block: Block,
    },
    BlockAnnounce {
        chain_id: String,
        hash: Hash,
    },
    NewBlockHash {
        chain_id: String,
        hash: Hash,
    },
    InvBlock {
        chain_id: String,
        hashes: Vec<Hash>,
    },
    GetTips {
        chain_id: String,
    },
    Tips {
        chain_id: String,
        tips: Vec<Hash>,
    },
    GetBlock {
        chain_id: String,
        hash: Hash,
    },
    BlockData {
        chain_id: String,
        block: Option<Block>,
    },
    Block {
        chain_id: String,
        block: Block,
    },
    Reject {
        chain_id: String,
        reason: String,
    },
    Error {
        chain_id: String,
        message: String,
    },
}

pub fn topic_names(chain_id: &str) -> Vec<String> {
    vec![
        format!("{}-blocks", chain_id),
        format!("{}-txs", chain_id),
        format!("{}-sync", chain_id),
    ]
}

pub fn message_id_for_tx(tx: &Transaction) -> String {
    format!("tx:{}", tx.txid)
}

pub fn message_id_for_block(block: &Block) -> String {
    format!("block:{}", block.hash)
}
