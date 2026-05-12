use std::collections::{HashMap, HashSet};

use crate::{
    state::{
        ChainState, ContractRuntimeConfig, ContractRuntimeState, DagState, Mempool, UtxoState,
    },
    tx::compute_txid,
    types::{
        compute_block_hash, compute_merkle_root, Block, BlockHeader, OutPoint, Transaction,
        TxOutput, Utxo,
    },
};

pub const GENESIS_TREASURY: &str = "genesis-treasury";
pub const GENESIS_SUPPLY: u64 = 1_000_000_000;

pub fn genesis_transaction() -> Transaction {
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs: vec![],
        outputs: vec![TxOutput {
            address: GENESIS_TREASURY.into(),
            amount: GENESIS_SUPPLY,
        }],
        fee: 0,
        nonce: 0,
    };
    tx.txid = compute_txid(&tx);
    tx
}

pub fn genesis_block() -> Block {
    let txs = vec![genesis_transaction()];
    let tx = &txs[0];
    let outpoint = OutPoint {
        txid: tx.txid.clone(),
        index: 0,
    };
    let utxo = Utxo {
        outpoint: outpoint.clone(),
        address: GENESIS_TREASURY.into(),
        amount: GENESIS_SUPPLY,
        coinbase: false,
        height: 0,
    };
    let mut utxos = HashMap::new();
    utxos.insert(outpoint.clone(), utxo);
    let mut address_index = HashMap::new();
    address_index.insert(GENESIS_TREASURY.into(), vec![outpoint]);
    let state_root = UtxoState {
        utxos,
        address_index,
    }
    .compute_state_root()
    .expect("genesis UTXO state must be deterministic");

    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: 1,
            parents: vec![],
            timestamp: 0,
            difficulty: 1,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root,
            blue_score: 0,
            height: 0,
        },
        transactions: txs,
    };
    block.hash = compute_block_hash(&block.header);
    block
}

pub fn init_chain_state(chain_id: String) -> ChainState {
    let genesis = genesis_block();
    let tx = genesis.transactions[0].clone();
    let outpoint = OutPoint {
        txid: tx.txid.clone(),
        index: 0,
    };
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
        utxo: UtxoState {
            utxos,
            address_index,
        },
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
