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
pub struct MempoolCounters {
    pub accepted_total: u64,
    pub rejected_total: u64,
    pub rejected_low_priority_total: u64,
    pub evicted_total: u64,
    pub pressure_events_total: u64,
    pub reconcile_runs_total: u64,
    pub reconcile_removed_total: u64,
    pub orphaned_total: u64,
    pub orphan_promoted_total: u64,
    pub orphan_dropped_total: u64,
    pub orphan_pruned_total: u64,
}

fn default_mempool_max_transactions() -> usize {
    1_024
}

fn default_mempool_max_orphans() -> usize {
    512
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mempool {
    pub transactions: HashMap<Hash, Transaction>,
    pub spent_outpoints: HashSet<OutPoint>,
    #[serde(default)]
    pub orphan_transactions: HashMap<Hash, Transaction>,
    #[serde(default)]
    pub orphan_missing_outpoints: HashMap<Hash, Vec<OutPoint>>,
    #[serde(default)]
    pub orphan_received_order: HashMap<Hash, u64>,
    #[serde(default)]
    pub next_orphan_order: u64,
    #[serde(default)]
    pub counters: MempoolCounters,
    #[serde(default = "default_mempool_max_transactions")]
    pub max_transactions: usize,
    #[serde(default = "default_mempool_max_orphans")]
    pub max_orphans: usize,
}

impl Default for Mempool {
    fn default() -> Self {
        Self {
            transactions: HashMap::new(),
            spent_outpoints: HashSet::new(),
            orphan_transactions: HashMap::new(),
            orphan_missing_outpoints: HashMap::new(),
            orphan_received_order: HashMap::new(),
            next_orphan_order: 0,
            counters: MempoolCounters::default(),
            max_transactions: default_mempool_max_transactions(),
            max_orphans: default_mempool_max_orphans(),
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
