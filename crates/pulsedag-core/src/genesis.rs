use std::collections::{HashMap, HashSet};

use crate::{
    state::{ChainState, ContractRuntimeConfig, ContractRuntimeState, DagState, Mempool, UtxoState},
    types::{Block, BlockHeader, OutPoint, Transaction, TxOutput, Utxo},
};

pub const GENESIS_TREASURY: &str = "genesis-treasury";
pub const GENESIS_SUPPLY: u64 = 1_000_000_000;

pub fn genesis_transaction() -> Transaction {
    Transaction {
        txid: "genesis-tx".into(),
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput {
            address: GENESIS_TREASURY.into(),
            amount: GENESIS_SUPPLY,
        }],
        fee: 0,
        nonce: 0,
    }
}

pub fn genesis_block() -> Block {
    Block {
        hash: "genesis-block".into(),
        header: BlockHeader {
            version: 1,
            parents: vec![],
            timestamp: 0,
            difficulty: 1,
            nonce: 0,
            merkle_root: "genesis-merkle".into(),
            state_root: "genesis-state".into(),
            blue_score: 0,
            height: 0,
        },
        transactions: vec![genesis_transaction()],
    }
}

pub fn init_chain_state(chain_id: String) -> ChainState {
    let genesis = genesis_block();
    let tx = genesis.transactions[0].clone();
    let outpoint = OutPoint { txid: tx.txid.clone(), index: 0 };
    let utxo = Utxo {
        outpoint: outpoint.clone(),
        address: GENESIS_TREASURY.into(),
        amount: GENESIS_SUPPLY,
        coinbase: false,
        height: 0,
    };

    let mut blocks = HashMap::new();
    blocks.insert(genesis.hash.clone(), genesis.clone());

    let mut tips = HashSet::new();
    tips.insert(genesis.hash.clone());

    let mut utxos = HashMap::new();
    utxos.insert(outpoint.clone(), utxo);

    let mut address_index = HashMap::new();
    address_index.insert(GENESIS_TREASURY.into(), vec![outpoint]);

    ChainState {
        chain_id,
        dag: DagState {
            blocks,
            tips,
            children: HashMap::new(),
            genesis_hash: genesis.hash,
            best_height: 0,
        },
        utxo: UtxoState { utxos, address_index },
        mempool: Mempool::default(),
        contracts: ContractRuntimeState {
            config: ContractRuntimeConfig {
                enabled: false,
                vm_version: "reserved-v1".into(),
                max_gas_per_tx: 10_000_000,
                max_contract_size_bytes: 64 * 1024,
                max_storage_key_bytes: 128,
                max_storage_value_bytes: 64 * 1024,
            },
            contract_count: 0,
            storage_slots: 0,
            receipt_count: 0,
            last_receipt_id: None,
        },
        orphan_blocks: HashMap::new(),
        orphan_missing_parents: HashMap::new(),
        orphan_received_at_ms: HashMap::new(),
    }
}
