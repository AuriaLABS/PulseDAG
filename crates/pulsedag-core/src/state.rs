use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError,
    types::{Address, Block, Hash, OutPoint, StateRoot, Transaction, Utxo},
};

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
    #[serde(default)]
    pub consensus_mode: ConsensusMode,
    #[serde(default)]
    pub selected_parents: HashMap<Hash, Option<Hash>>,
    #[serde(default)]
    pub selected_chain: Vec<Hash>,
    #[serde(default)]
    pub selected_parent_policy: SelectedParentPolicy,
    #[serde(default = "crate::ghostdag::default_merge_set_k")]
    pub merge_set_k: usize,
    #[serde(default)]
    pub merge_set_blues: HashMap<Hash, Vec<Hash>>,
    #[serde(default)]
    pub merge_set_reds: HashMap<Hash, Vec<Hash>>,
    #[serde(default)]
    pub blue_work: HashMap<Hash, u128>,
    #[serde(default)]
    pub merge_set_diagnostics: HashMap<Hash, crate::ghostdag::MergeSetDiagnostics>,
    #[serde(default)]
    pub ordered_dag: Vec<Hash>,
    #[serde(default = "crate::ordering::default_ordering_version")]
    pub ordering_version: String,
    #[serde(default)]
    pub ordered_dag_rebuild_total: u64,
    #[serde(default)]
    pub ordered_dag_rebuild_failed_total: u64,
    #[serde(default)]
    pub ordered_dag_state_root: Option<StateRoot>,
    #[serde(default)]
    pub ordered_dag_tip: Option<Hash>,
    #[serde(default)]
    pub ordered_dag_conflict_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusMode {
    #[default]
    Legacy,
    GhostdagDev,
}

impl ConsensusMode {
    pub fn ghostdag_metadata_active(self) -> bool {
        matches!(self, Self::GhostdagDev)
    }

    pub fn high_cadence_allowed(self) -> bool {
        false
    }
}

impl std::fmt::Display for ConsensusMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy => f.write_str("legacy"),
            Self::GhostdagDev => f.write_str("ghostdag_dev"),
        }
    }
}

impl std::str::FromStr for ConsensusMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "legacy" => Ok(Self::Legacy),
            "ghostdag_dev" => Ok(Self::GhostdagDev),
            other => Err(format!(
                "invalid consensus mode {other:?}; expected legacy or ghostdag_dev"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SelectedParentPolicy {
    #[default]
    GhostdagInspired,
    LegacyTip,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UtxoState {
    pub utxos: HashMap<OutPoint, Utxo>,
    pub address_index: HashMap<Address, Vec<OutPoint>>,
}

const UTXO_STATE_ROOT_DOMAIN: &[u8] = b"PulseDAG:utxo-state-root:v1";

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

fn outpoint_label(outpoint: &OutPoint) -> String {
    format!("{}:{}", outpoint.txid, outpoint.index)
}

impl UtxoState {
    pub fn validate_deterministic_indexes(&self) -> Result<(), PulseError> {
        let mut indexed = BTreeMap::<OutPoint, Address>::new();
        for (address, outpoints) in &self.address_index {
            let mut seen_for_address = BTreeSet::new();
            for outpoint in outpoints {
                if !seen_for_address.insert(outpoint.clone()) {
                    return Err(PulseError::NonDeterministicState(format!(
                        "duplicate address index entry {} for address {}",
                        outpoint_label(outpoint),
                        address
                    )));
                }
                if indexed.insert(outpoint.clone(), address.clone()).is_some() {
                    return Err(PulseError::NonDeterministicState(format!(
                        "outpoint {} appears under multiple addresses",
                        outpoint_label(outpoint)
                    )));
                }
            }
        }

        for (outpoint, utxo) in &self.utxos {
            if &utxo.outpoint != outpoint {
                return Err(PulseError::NonDeterministicState(format!(
                    "utxo key {} does not match embedded outpoint {}",
                    outpoint_label(outpoint),
                    outpoint_label(&utxo.outpoint)
                )));
            }
            match indexed.get(outpoint) {
                Some(address) if address == &utxo.address => {}
                Some(address) => {
                    return Err(PulseError::NonDeterministicState(format!(
                        "address index maps {} to {}, expected {}",
                        outpoint_label(outpoint),
                        address,
                        utxo.address
                    )));
                }
                None => {
                    return Err(PulseError::NonDeterministicState(format!(
                        "missing address index entry for {}",
                        outpoint_label(outpoint)
                    )));
                }
            }
        }

        for outpoint in indexed.keys() {
            if !self.utxos.contains_key(outpoint) {
                return Err(PulseError::NonDeterministicState(format!(
                    "address index references missing outpoint {}",
                    outpoint_label(outpoint)
                )));
            }
        }

        Ok(())
    }

    pub fn compute_state_root(&self) -> Result<StateRoot, PulseError> {
        self.validate_deterministic_indexes()?;

        let mut ordered = BTreeMap::new();
        for (outpoint, utxo) in &self.utxos {
            ordered.insert(outpoint.clone(), utxo);
        }

        let mut bytes = Vec::new();
        encode_len_prefixed_bytes(&mut bytes, UTXO_STATE_ROOT_DOMAIN);
        let count = u64::try_from(ordered.len()).expect("utxo set length exceeds u64::MAX");
        bytes.extend_from_slice(&count.to_le_bytes());
        for (outpoint, utxo) in ordered {
            encode_len_prefixed_str(&mut bytes, &outpoint.txid);
            bytes.extend_from_slice(&outpoint.index.to_le_bytes());
            encode_len_prefixed_str(&mut bytes, &utxo.address);
            bytes.extend_from_slice(&utxo.amount.to_le_bytes());
            bytes.push(u8::from(utxo.coinbase));
            bytes.extend_from_slice(&utxo.height.to_le_bytes());
        }

        Ok(hex::encode(Sha256::digest(bytes)))
    }
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
    pub orphan_parent_index: HashMap<Hash, BTreeSet<Hash>>,
    #[serde(default)]
    pub orphan_received_at_ms: HashMap<Hash, u64>,
    #[serde(default)]
    pub terminal_missing_parents: HashMap<Hash, MissingParentTerminalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingParentState {
    Requestable,
    Pending(String),
    Retryable(String),
    Backoff(u64),
    Exhausted(Vec<String>),
    ExhaustedResidual,
    Peerless,
    TerminalEvicted,
    Quarantined,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingParentTerminalEntry {
    pub state: MissingParentState,
    pub transitioned_at_ms: u64,
    pub waiting_orphans: Vec<Hash>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utxo(txid: &str, index: u32, address: &str, amount: u64, height: u64) -> (OutPoint, Utxo) {
        let outpoint = OutPoint {
            txid: txid.to_string(),
            index,
        };
        let utxo = Utxo {
            outpoint: outpoint.clone(),
            address: address.to_string(),
            amount,
            coinbase: false,
            height,
        };
        (outpoint, utxo)
    }

    fn insert_utxo(state: &mut UtxoState, outpoint: OutPoint, utxo: Utxo) {
        state
            .address_index
            .entry(utxo.address.clone())
            .or_default()
            .push(outpoint.clone());
        state.utxos.insert(outpoint, utxo);
    }

    #[test]
    fn state_root_is_independent_of_hashmap_insertion_order() {
        let entries = vec![
            utxo("tx-b", 1, "bob", 25, 2),
            utxo("tx-a", 0, "alice", 10, 1),
            utxo("tx-b", 0, "carol", 30, 2),
        ];
        let mut forward = UtxoState::default();
        let mut reverse = UtxoState::default();

        for (outpoint, utxo) in entries.clone() {
            insert_utxo(&mut forward, outpoint, utxo);
        }
        for (outpoint, utxo) in entries.into_iter().rev() {
            insert_utxo(&mut reverse, outpoint, utxo);
        }

        assert_eq!(
            forward.compute_state_root().unwrap(),
            reverse.compute_state_root().unwrap()
        );
    }

    #[test]
    fn state_root_changes_when_utxo_state_changes() {
        let (outpoint, utxo) = utxo("tx-a", 0, "alice", 10, 1);
        let mut state = UtxoState::default();
        insert_utxo(&mut state, outpoint.clone(), utxo);
        let before = state.compute_state_root().unwrap();

        state.utxos.get_mut(&outpoint).unwrap().amount = 11;

        assert_ne!(before, state.compute_state_root().unwrap());
    }

    #[test]
    fn non_deterministic_address_index_is_rejected() {
        let (outpoint, utxo) = utxo("tx-a", 0, "alice", 10, 1);
        let mut state = UtxoState::default();
        insert_utxo(&mut state, outpoint.clone(), utxo);
        state.address_index.get_mut("alice").unwrap().push(outpoint);

        assert!(matches!(
            state.compute_state_root(),
            Err(PulseError::NonDeterministicState(_))
        ));
    }
}
