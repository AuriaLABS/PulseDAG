use serde::{Deserialize, Serialize};

pub type Hash = String;
pub type Address = String;
pub type PublicKeyHex = String;
pub type SignatureHex = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutPoint {
    pub txid: Hash,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub previous_output: OutPoint,
    pub public_key: PublicKeyHex,
    pub signature: SignatureHex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub address: Address,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub txid: Hash,
    pub version: u32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub fee: u64,
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub outpoint: OutPoint,
    pub address: Address,
    pub amount: u64,
    pub coinbase: bool,
    pub height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub parents: Vec<Hash>,
    pub timestamp: u64,
    pub difficulty: u32,
    pub nonce: u64,
    pub merkle_root: Hash,
    pub state_root: Hash,
    pub blue_score: u64,
    pub height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: Hash,
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}
