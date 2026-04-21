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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mempool {
    pub transactions: HashMap<Hash, Transaction>,
    pub spent_outpoints: HashSet<OutPoint>,
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
