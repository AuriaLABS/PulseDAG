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
    /// Encoded as little-endian `u32` in the canonical PoW preimage.
    pub version: u32,
    /// Ordered list; serialized in-list-order with length-prefixed UTF-8 strings.
    pub parents: Vec<Hash>,
    /// Unix timestamp seconds, little-endian `u64`.
    pub timestamp: u64,
    /// Difficulty scalar, little-endian `u32`.
    pub difficulty: u32,
    /// Miner-controlled nonce, little-endian `u64`.
    pub nonce: u64,
    /// Length-prefixed UTF-8 in canonical PoW preimage.
    pub merkle_root: Hash,
    /// Length-prefixed UTF-8 in canonical PoW preimage.
    pub state_root: Hash,
    /// little-endian `u64`.
    pub blue_score: u64,
    /// little-endian `u64`.
    pub height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: Hash,
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}
