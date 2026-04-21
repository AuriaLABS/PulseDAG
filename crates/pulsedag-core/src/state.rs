use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::types::{Address, Block, Hash, OutPoint, Transaction, Utxo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRuntimeConfig {
    pub enabled: bool,
    pub vm_version: String,
    pub max_gas_per_tx: u64,
    pub max_contract_size_bytes: u64,
    pub max_storage_key_bytes: u32,
    pub max_storage_value_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRuntimeState {
    pub config: ContractRuntimeConfig,
    pub contract_count: u64,
    pub storage_slots: u64,
    pub receipt_count: u64,
    pub last_receipt_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagState {
    pub blocks: HashMap<Hash, Block>,
    pub tips: HashSet<Hash>,
    pub children: HashMap<Hash, Vec<Hash>>,
    pub genesis_hash: Hash,
    pub best_height: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UtxoState {
    pub utxos: HashMap<OutPoint, Utxo>,
    pub address_index: HashMap<Address, Vec<OutPoint>>,
}

fn default_mempool_limit() -> usize {
    4096
}
fn default_mempool_fee_floor() -> u64 {
    0
}
fn default_mempool_ttl_secs() -> u64 {
    3600
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mempool {
    #[serde(default)]
    pub transactions: HashMap<Hash, Transaction>,
    #[serde(default)]
    pub spent_outpoints: HashSet<OutPoint>,
    #[serde(default)]
    pub received_at_unix: HashMap<Hash, u64>,
    #[serde(default)]
    pub tx_sequence: HashMap<Hash, u64>,
    #[serde(default)]
    pub next_sequence: u64,
    #[serde(default = "default_mempool_limit")]
    pub limit: usize,
    #[serde(default = "default_mempool_fee_floor")]
    pub fee_floor: u64,
    #[serde(default = "default_mempool_ttl_secs")]
    pub ttl_secs: u64,
    #[serde(default)]
    pub evicted_total: u64,
    #[serde(default)]
    pub rejected_total: u64,
    #[serde(default)]
    pub rejected_fee_floor_total: u64,
    #[serde(default)]
    pub sanitize_runs: u64,
}

impl Default for Mempool {
    fn default() -> Self {
        Self {
            transactions: HashMap::new(),
            spent_outpoints: HashSet::new(),
            received_at_unix: HashMap::new(),
            tx_sequence: HashMap::new(),
            next_sequence: 0,
            limit: default_mempool_limit(),
            fee_floor: default_mempool_fee_floor(),
            ttl_secs: default_mempool_ttl_secs(),
            evicted_total: 0,
            rejected_total: 0,
            rejected_fee_floor_total: 0,
            sanitize_runs: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub chain_id: String,
    pub dag: DagState,
    pub utxo: UtxoState,
    pub mempool: Mempool,
    pub contracts: ContractRuntimeState,
    #[serde(default)]
    pub orphan_blocks: HashMap<Hash, Block>,
    #[serde(default)]
    pub orphan_missing_parents: HashMap<Hash, Vec<Hash>>,
    #[serde(default)]
    pub orphan_received_at_ms: HashMap<Hash, u64>,
}
