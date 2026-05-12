use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Hash = String;
pub type Hash32 = Hash;
pub type BlockId = Hash32;
pub type TxId = Hash32;
pub type MerkleRoot = Hash32;
pub type StateRoot = Hash32;
pub type Address = String;
pub type PublicKeyHex = String;
pub type SignatureHex = String;

const BLOCK_HEADER_DOMAIN: &[u8] = b"PulseDAG:block-header:v1";
const MERKLE_EMPTY_DOMAIN: &[u8] = b"PulseDAG:merkle-empty:v1";
const MERKLE_LEAF_DOMAIN: &[u8] = b"PulseDAG:merkle-leaf:v1";
const MERKLE_NODE_DOMAIN: &[u8] = b"PulseDAG:merkle-node:v1";
const MINING_PREIMAGE_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutPoint {
    pub txid: TxId,
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
    pub txid: TxId,
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
    /// Encoded as little-endian `u32` in canonical header and PoW preimages.
    pub version: u32,
    /// Ordered list; serialized in-list-order for the consensus block hash.
    pub parents: Vec<BlockId>,
    /// Unix timestamp seconds, little-endian `u64`.
    pub timestamp: u64,
    /// Difficulty scalar, little-endian `u32`.
    pub difficulty: u32,
    /// Miner-controlled nonce, little-endian `u64`.
    pub nonce: u64,
    /// Canonical transaction Merkle root.
    pub merkle_root: MerkleRoot,
    /// Canonical state root placeholder until v3 state roots are introduced.
    pub state_root: StateRoot,
    /// little-endian `u64`.
    pub blue_score: u64,
    /// little-endian `u64`.
    pub height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: BlockId,
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

fn encode_string_vec(out: &mut Vec<u8>, values: &[String]) {
    let len = u32::try_from(values.len()).expect("canonical vector length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    for value in values {
        encode_len_prefixed_str(out, value);
    }
}

/// Canonical consensus serialization for a block header, including nonce.
pub fn canonical_block_header_bytes(header: &BlockHeader) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    encode_len_prefixed_bytes(&mut out, BLOCK_HEADER_DOMAIN);
    out.extend_from_slice(&header.version.to_le_bytes());
    encode_string_vec(&mut out, &header.parents);
    out.extend_from_slice(&header.timestamp.to_le_bytes());
    out.extend_from_slice(&header.difficulty.to_le_bytes());
    out.extend_from_slice(&header.nonce.to_le_bytes());
    encode_len_prefixed_str(&mut out, &header.merkle_root);
    encode_len_prefixed_str(&mut out, &header.state_root);
    out.extend_from_slice(&header.blue_score.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    out
}

/// Canonical mining preimage serialization. Nonce is deliberately excluded so
/// mining engines can combine this stable preimage with candidate nonces.
pub fn canonical_mining_preimage_bytes(header: &BlockHeader) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.push(MINING_PREIMAGE_VERSION);
    out.extend_from_slice(&header.version.to_le_bytes());
    let mut parents = header.parents.clone();
    parents.sort_unstable();
    let parent_count = u16::try_from(parents.len()).expect("parent count exceeds u16::MAX");
    out.extend_from_slice(&parent_count.to_le_bytes());
    for parent in &parents {
        let len = u16::try_from(parent.len()).expect("parent hash length exceeds u16::MAX");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(parent.as_bytes());
    }
    out.extend_from_slice(&header.timestamp.to_le_bytes());
    out.extend_from_slice(&header.difficulty.to_le_bytes());
    let merkle_len = u16::try_from(header.merkle_root.len()).expect("merkle root too long");
    out.extend_from_slice(&merkle_len.to_le_bytes());
    out.extend_from_slice(header.merkle_root.as_bytes());
    let state_len = u16::try_from(header.state_root.len()).expect("state root too long");
    out.extend_from_slice(&state_len.to_le_bytes());
    out.extend_from_slice(header.state_root.as_bytes());
    out.extend_from_slice(&header.blue_score.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    out
}

pub fn compute_block_hash(header: &BlockHeader) -> BlockId {
    let digest = Sha256::digest(canonical_block_header_bytes(header));
    hex::encode(digest)
}

fn merkle_leaf(txid: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MERKLE_LEAF_DOMAIN);
    match hex::decode(txid) {
        Ok(bytes) if bytes.len() == 32 => hasher.update(bytes),
        _ => hasher.update(txid.as_bytes()),
    }
    hasher.finalize().into()
}

fn merkle_parent(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(MERKLE_NODE_DOMAIN);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

pub fn compute_merkle_root_from_txids(txids: &[TxId]) -> MerkleRoot {
    if txids.is_empty() {
        return hex::encode(Sha256::digest(MERKLE_EMPTY_DOMAIN));
    }

    let mut level = txids
        .iter()
        .map(|txid| merkle_leaf(txid))
        .collect::<Vec<_>>();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = &pair[0];
            let right = pair.get(1).unwrap_or(left);
            next.push(merkle_parent(left, right));
        }
        level = next;
    }
    hex::encode(level[0])
}

pub fn compute_merkle_root(transactions: &[Transaction]) -> MerkleRoot {
    let txids = transactions
        .iter()
        .map(|tx| tx.txid.clone())
        .collect::<Vec<_>>();
    compute_merkle_root_from_txids(&txids)
}
