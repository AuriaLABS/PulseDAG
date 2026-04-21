use std::{sync::Arc, time::{SystemTime, UNIX_EPOCH}};

use pulsedag_core::{
    errors::PulseError,
    genesis::init_chain_state,
    rebuild_state_from_blocks,
    state::ChainState,
    types::{Block, Hash, OutPoint, Utxo},
};
use rocksdb::{ColumnFamilyDescriptor, DB};
use serde::{Deserialize, Serialize};

const CHAIN_STATE_KEY: &[u8] = b"chain_state";
const RUNTIME_EVENT_PREFIX: &str = "runtime_event:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub timestamp_unix: u64,
    pub level: String,
    pub kind: String,
    pub message: String,
}


pub struct Storage {
    pub db: Arc<DB>,
}

impl Storage {
    pub fn open(path: &str) -> Result<Self, PulseError> {
        let cfs = vec![
            ColumnFamilyDescriptor::new("blocks", Default::default()),
            ColumnFamilyDescriptor::new("utxos", Default::default()),
            ColumnFamilyDescriptor::new("meta", Default::default()),
            ColumnFamilyDescriptor::new("contracts_meta", Default::default()),
            ColumnFamilyDescriptor::new("contracts_storage", Default::default()),
            ColumnFamilyDescriptor::new("contracts_receipts", Default::default()),
        ];
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .map_err(|e| PulseError::StorageError(e.to_string()))?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn persist_block(&self, block: &Block) -> Result<(), PulseError> {
        let cf = self.db.cf_handle("blocks").ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        self.db.put_cf(cf, block.hash.as_bytes(), serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn list_blocks(&self) -> Result<Vec<Block>, PulseError> {
        let cf = self.db.cf_handle("blocks").ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut blocks = Vec::new();
        for item in iter {
            let (_, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            let block: Block = serde_json::from_slice(&value).map_err(|e| PulseError::StorageError(e.to_string()))?;
            blocks.push(block);
        }
        blocks.sort_by_key(|b| b.header.height);
        Ok(blocks)
    }

    pub fn persist_utxo(&self, outpoint: &OutPoint, utxo: &Utxo) -> Result<(), PulseError> {
        let cf = self.db.cf_handle("utxos").ok_or_else(|| PulseError::StorageError("missing cf utxos".into()))?;
        let key = serde_json::to_vec(outpoint).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let value = serde_json::to_vec(utxo).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db.put_cf(cf, key, value).map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn delete_utxo(&self, outpoint: &OutPoint) -> Result<(), PulseError> {
        let cf = self.db.cf_handle("utxos").ok_or_else(|| PulseError::StorageError("missing cf utxos".into()))?;
        let key = serde_json::to_vec(outpoint).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db.delete_cf(cf, key).map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn get_block(&self, hash: &Hash) -> Result<Option<Block>, PulseError> {
        let cf = self.db.cf_handle("blocks").ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let raw = self.db.get_cf(cf, hash.as_bytes()).map_err(|e| PulseError::StorageError(e.to_string()))?;
        match raw {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|e| PulseError::StorageError(e.to_string()))?)),
            None => Ok(None),
        }
    }

    pub fn persist_chain_state(&self, state: &ChainState) -> Result<(), PulseError> {
        let cf = self.db.cf_handle("meta").ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let value = bincode::serialize(state).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db.put_cf(cf, CHAIN_STATE_KEY, value).map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn load_chain_state(&self) -> Result<Option<ChainState>, PulseError> {
        let cf = self.db.cf_handle("meta").ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        match self.db.get_cf(cf, CHAIN_STATE_KEY).map_err(|e| PulseError::StorageError(e.to_string()))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes).map_err(|e| PulseError::StorageError(e.to_string()))?)),
            None => Ok(None),
        }
    }

    pub fn load_or_init_genesis(&self, chain_id: String) -> Result<ChainState, PulseError> {
        if let Some(state) = self.load_chain_state()? {
            return Ok(state);
        }
        let blocks = self.list_blocks()?;
        if !blocks.is_empty() {
            let rebuilt = rebuild_state_from_blocks(chain_id, blocks)?;
            self.persist_chain_state(&rebuilt)?;
            return Ok(rebuilt);
        }
        let state = init_chain_state(chain_id);
        self.persist_chain_state(&state)?;
        for block in state.dag.blocks.values() {
            self.persist_block(block)?;
        }
        Ok(state)
    }

    pub fn replay_blocks_or_init(&self, chain_id: String) -> Result<ChainState, PulseError> {
        let blocks = self.list_blocks()?;
        if blocks.is_empty() {
            return self.load_or_init_genesis(chain_id);
        }
        let state = rebuild_state_from_blocks(chain_id, blocks)?;
        self.persist_chain_state(&state)?;
        Ok(state)
    }

    pub fn snapshot_exists(&self) -> Result<bool, PulseError> {
        Ok(self.load_chain_state()?.is_some())
    }

    pub fn append_runtime_event(&self, level: &str, kind: &str, message: &str) -> Result<RuntimeEvent, PulseError> {
        let cf = self.db.cf_handle("meta").ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let timestamp_unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let unique_nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let event = RuntimeEvent {
            timestamp_unix,
            level: level.to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
        };
        let key = format!("{}{:020}", RUNTIME_EVENT_PREFIX, unique_nanos);
        let value = serde_json::to_vec(&event).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db.put_cf(cf, key.as_bytes(), value).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let _ = self.prune_runtime_events(2_000);
        Ok(event)
    }

    pub fn prune_runtime_events(&self, max_events: usize) -> Result<usize, PulseError> {
        let cf = self.db.cf_handle("meta").ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut keys = Vec::new();
        for item in iter {
            let (key, _) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with(RUNTIME_EVENT_PREFIX) {
                    keys.push(key.to_vec());
                }
            }
        }
        if keys.len() <= max_events {
            return Ok(0);
        }
        let to_delete = keys.len() - max_events;
        for key in keys.into_iter().take(to_delete) {
            self.db.delete_cf(cf, key).map_err(|e| PulseError::StorageError(e.to_string()))?;
        }
        Ok(to_delete)
    }

    pub fn list_runtime_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, PulseError> {
        let cf = self.db.cf_handle("meta").ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut events = Vec::new();
        for item in iter {
            let (key, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with(RUNTIME_EVENT_PREFIX) {
                    let event: RuntimeEvent = serde_json::from_slice(&value).map_err(|e| PulseError::StorageError(e.to_string()))?;
                    events.push(event);
                }
            }
        }
        events.sort_by_key(|e| e.timestamp_unix);
        if events.len() > limit {
            events = events.split_off(events.len() - limit);
        }
        Ok(events)
    }

    pub fn contract_namespaces_ready(&self) -> bool {
        self.db.cf_handle("contracts_meta").is_some()
            && self.db.cf_handle("contracts_storage").is_some()
            && self.db.cf_handle("contracts_receipts").is_some()
    }

}
