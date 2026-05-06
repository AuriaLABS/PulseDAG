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

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::types::{BlockHeader, TxOutput};

    fn sample_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 1,
            nonce: 7,
        }
    }

    fn sample_block(hash: &str) -> Block {
        Block {
            hash: hash.into(),
            header: BlockHeader {
                version: 1,
                parents: vec!["parent".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "mr".into(),
                state_root: "sr".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![sample_tx("coinbase-like")],
        }
    }

    fn chain_id(message: &NetworkMessage) -> &str {
        match message {
            NetworkMessage::NewTransaction { chain_id, .. }
            | NetworkMessage::NewBlock { chain_id, .. }
            | NetworkMessage::BlockAnnounce { chain_id, .. }
            | NetworkMessage::NewBlockHash { chain_id, .. }
            | NetworkMessage::InvBlock { chain_id, .. }
            | NetworkMessage::GetTips { chain_id }
            | NetworkMessage::Tips { chain_id, .. }
            | NetworkMessage::GetBlock { chain_id, .. }
            | NetworkMessage::BlockData { chain_id, .. }
            | NetworkMessage::Block { chain_id, .. }
            | NetworkMessage::Reject { chain_id, .. }
            | NetworkMessage::Error { chain_id, .. } => chain_id,
        }
    }

    fn message_kind(message: &NetworkMessage) -> &'static str {
        match message {
            NetworkMessage::NewTransaction { .. } => "NewTransaction",
            NetworkMessage::NewBlock { .. } => "NewBlock",
            NetworkMessage::BlockAnnounce { .. } => "BlockAnnounce",
            NetworkMessage::NewBlockHash { .. } => "NewBlockHash",
            NetworkMessage::InvBlock { .. } => "InvBlock",
            NetworkMessage::GetTips { .. } => "GetTips",
            NetworkMessage::Tips { .. } => "Tips",
            NetworkMessage::GetBlock { .. } => "GetBlock",
            NetworkMessage::BlockData { .. } => "BlockData",
            NetworkMessage::Block { .. } => "Block",
            NetworkMessage::Reject { .. } => "Reject",
            NetworkMessage::Error { .. } => "Error",
        }
    }

    #[test]
    fn serializes_and_deserializes_every_network_message_variant() {
        let tx = sample_tx("tx-all-variants");
        let block = sample_block("block-all-variants");
        let messages = vec![
            NetworkMessage::NewTransaction {
                chain_id: "testnet".into(),
                transaction: tx.clone(),
            },
            NetworkMessage::NewBlock {
                chain_id: "testnet".into(),
                block: block.clone(),
            },
            NetworkMessage::BlockAnnounce {
                chain_id: "testnet".into(),
                hash: block.hash.clone(),
            },
            NetworkMessage::NewBlockHash {
                chain_id: "testnet".into(),
                hash: block.hash.clone(),
            },
            NetworkMessage::InvBlock {
                chain_id: "testnet".into(),
                hashes: vec![block.hash.clone()],
            },
            NetworkMessage::GetTips {
                chain_id: "testnet".into(),
            },
            NetworkMessage::Tips {
                chain_id: "testnet".into(),
                tips: vec![block.hash.clone()],
            },
            NetworkMessage::GetBlock {
                chain_id: "testnet".into(),
                hash: block.hash.clone(),
            },
            NetworkMessage::BlockData {
                chain_id: "testnet".into(),
                block: Some(block.clone()),
            },
            NetworkMessage::BlockData {
                chain_id: "testnet".into(),
                block: None,
            },
            NetworkMessage::Block {
                chain_id: "testnet".into(),
                block,
            },
            NetworkMessage::Reject {
                chain_id: "testnet".into(),
                reason: "not found".into(),
            },
            NetworkMessage::Error {
                chain_id: "testnet".into(),
                message: "malformed".into(),
            },
        ];

        for message in messages {
            let encoded = serde_json::to_vec(&message).expect("message serializes");
            let decoded: NetworkMessage =
                serde_json::from_slice(&encoded).expect("message deserializes");
            assert_eq!(message_kind(&decoded), message_kind(&message));
            assert_eq!(chain_id(&decoded), "testnet");
        }
    }

    #[test]
    fn rejects_malformed_payloads_during_decode() {
        let malformed_json = br#"{"type":"GetBlock","chain_id":"testnet","hash":42}"#;
        assert!(serde_json::from_slice::<NetworkMessage>(malformed_json).is_err());

        let unknown_variant = br#"{"type":"Unknown","chain_id":"testnet"}"#;
        assert!(serde_json::from_slice::<NetworkMessage>(unknown_variant).is_err());
    }

    #[test]
    fn message_ids_for_tx_and_block_are_stable_and_content_addressed() {
        let tx = sample_tx("stable-tx");
        let mut tx_with_different_body = tx.clone();
        tx_with_different_body.fee = 99;
        assert_eq!(message_id_for_tx(&tx), "tx:stable-tx");
        assert_eq!(
            message_id_for_tx(&tx),
            message_id_for_tx(&tx_with_different_body)
        );

        let block = sample_block("stable-block");
        let mut block_with_different_body = block.clone();
        block_with_different_body.header.nonce = 99;
        assert_eq!(message_id_for_block(&block), "block:stable-block");
        assert_eq!(
            message_id_for_block(&block),
            message_id_for_block(&block_with_different_body)
        );
    }
}
