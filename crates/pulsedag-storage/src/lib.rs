use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use pulsedag_core::{
    errors::PulseError,
    genesis::init_chain_state,
    rebuild_state_from_blocks, rebuild_state_from_snapshot_and_blocks,
    state::ChainState,
    types::{Block, Hash, OutPoint, Utxo},
};
use rocksdb::{ColumnFamilyDescriptor, WriteBatch, DB};
use serde::{Deserialize, Serialize};

const CHAIN_STATE_KEY: &[u8] = b"chain_state";
const SNAPSHOT_CAPTURED_AT_UNIX_KEY: &[u8] = b"snapshot_captured_at_unix";
const RUNTIME_EVENT_PREFIX: &str = "runtime_event:";
const BLOCK_INDEX_PREFIX: &str = "block_index:";
const BLOCK_INDEX_REPORT_INTERVAL: usize = 1_000;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreDrillReport {
    pub used_snapshot: bool,
    pub fallback_to_full_rebuild: bool,
    pub persisted_block_count: usize,
    pub best_height: u64,
    pub restore_duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIndexHealth {
    pub total_blocks: usize,
    pub index_entries: usize,
    pub rebuilt_entries: usize,
    pub removed_stale_entries: usize,
    pub used_full_scan: bool,
}

impl Storage {
    pub fn open(path: &str) -> Result<Self, PulseError> {
        let cfs = vec![
            ColumnFamilyDescriptor::new("blocks", Default::default()),
            ColumnFamilyDescriptor::new("block_index", Default::default()),
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
        let blocks_cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let block_index_cf = self
            .db
            .cf_handle("block_index")
            .ok_or_else(|| PulseError::StorageError("missing cf block_index".into()))?;
        let value =
            serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let mut batch = WriteBatch::default();
        batch.put_cf(&blocks_cf, block.hash.as_bytes(), value);
        batch.put_cf(
            &block_index_cf,
            Self::block_index_key(block.header.height, &block.hash).as_bytes(),
            block.hash.as_bytes(),
        );
        self.db
            .write(batch)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn list_blocks(&self) -> Result<Vec<Block>, PulseError> {
        let block_count = self.count_cf_entries("blocks")?;
        if block_count == 0 {
            return Ok(Vec::new());
        }

        let index_count = self.count_cf_entries("block_index")?;
        if index_count != block_count {
            self.ensure_block_index()?;
            return self.list_blocks_via_index();
        }

        match self.list_blocks_via_index() {
            Ok(blocks) if blocks.len() == block_count => Ok(blocks),
            _ => {
                self.ensure_block_index()?;
                self.list_blocks_via_index()
            }
        }
    }

    fn count_cf_entries(&self, cf_name: &str) -> Result<usize, PulseError> {
        let cf = self
            .db
            .cf_handle(cf_name)
            .ok_or_else(|| PulseError::StorageError(format!("missing cf {cf_name}")))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut count = 0usize;
        for item in iter {
            item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            count += 1;
        }
        Ok(count)
    }

    fn list_blocks_from_blocks_cf(&self) -> Result<Vec<Block>, PulseError> {
        let cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut blocks = Vec::new();
        for item in iter {
            let (_, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            let block: Block = serde_json::from_slice(&value)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            blocks.push(block);
        }
        blocks.sort_by(|a, b| {
            a.header
                .height
                .cmp(&b.header.height)
                .then_with(|| a.hash.cmp(&b.hash))
        });
        Ok(blocks)
    }

    fn list_blocks_via_index(&self) -> Result<Vec<Block>, PulseError> {
        let blocks_cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let block_index_cf = self
            .db
            .cf_handle("block_index")
            .ok_or_else(|| PulseError::StorageError("missing cf block_index".into()))?;
        let iter = self
            .db
            .iterator_cf(block_index_cf, rocksdb::IteratorMode::Start);
        let mut blocks = Vec::new();
        for item in iter {
            let (_, hash_bytes) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            let hash = std::str::from_utf8(&hash_bytes)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            let Some(block_bytes) = self
                .db
                .get_cf(blocks_cf, hash.as_bytes())
                .map_err(|e| PulseError::StorageError(e.to_string()))?
            else {
                return Err(PulseError::StorageError(format!(
                    "block index points to missing block {hash}"
                )));
            };
            let block: Block = serde_json::from_slice(&block_bytes)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            blocks.push(block);
        }
        Ok(blocks)
    }

    pub fn persist_utxo(&self, outpoint: &OutPoint, utxo: &Utxo) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle("utxos")
            .ok_or_else(|| PulseError::StorageError("missing cf utxos".into()))?;
        let key =
            serde_json::to_vec(outpoint).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let value =
            serde_json::to_vec(utxo).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db
            .put_cf(cf, key, value)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn delete_utxo(&self, outpoint: &OutPoint) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle("utxos")
            .ok_or_else(|| PulseError::StorageError("missing cf utxos".into()))?;
        let key =
            serde_json::to_vec(outpoint).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db
            .delete_cf(cf, key)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn get_block(&self, hash: &Hash) -> Result<Option<Block>, PulseError> {
        let cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let raw = self
            .db
            .get_cf(cf, hash.as_bytes())
            .map_err(|e| PulseError::StorageError(e.to_string()))?;
        match raw {
            Some(bytes) => Ok(Some(
                serde_json::from_slice(&bytes)
                    .map_err(|e| PulseError::StorageError(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    pub fn persist_chain_state(&self, state: &ChainState) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let mut batch = WriteBatch::default();
        self.stage_chain_state_snapshot(&mut batch, &cf, state)?;
        self.db
            .write(batch)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn persist_block_and_chain_state(
        &self,
        block: &Block,
        state: &ChainState,
    ) -> Result<(), PulseError> {
        let blocks_cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let block_value =
            serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let block_index_cf = self
            .db
            .cf_handle("block_index")
            .ok_or_else(|| PulseError::StorageError("missing cf block_index".into()))?;
        let mut batch = WriteBatch::default();
        batch.put_cf(&blocks_cf, block.hash.as_bytes(), block_value);
        batch.put_cf(
            &block_index_cf,
            Self::block_index_key(block.header.height, &block.hash).as_bytes(),
            block.hash.as_bytes(),
        );
        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        self.db
            .write(batch)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    fn block_index_key(height: u64, hash: &str) -> String {
        format!("{}{:020}:{}", BLOCK_INDEX_PREFIX, height, hash)
    }

    pub fn block_index_health(&self) -> Result<BlockIndexHealth, PulseError> {
        self.ensure_block_index()
    }

    fn ensure_block_index(&self) -> Result<BlockIndexHealth, PulseError> {
        let blocks_cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let block_index_cf = self
            .db
            .cf_handle("block_index")
            .ok_or_else(|| PulseError::StorageError("missing cf block_index".into()))?;

        let blocks_iter = self.db.iterator_cf(blocks_cf, rocksdb::IteratorMode::Start);
        let mut blocks = Vec::new();
        for item in blocks_iter {
            let (_, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            let block: Block = serde_json::from_slice(&value)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            blocks.push(block);
        }
        blocks.sort_by(|a, b| {
            a.header
                .height
                .cmp(&b.header.height)
                .then_with(|| a.hash.cmp(&b.hash))
        });

        let total_blocks = blocks.len();
        let mut batch = WriteBatch::default();
        let mut index_entries = 0usize;
        let mut rebuilt_entries = 0usize;
        let mut removed_stale_entries = 0usize;
        let mut needs_repair = false;
        let mut expected_keys = std::collections::BTreeSet::new();

        for block in &blocks {
            let key = Self::block_index_key(block.header.height, &block.hash);
            expected_keys.insert(key.clone());
            let existing = self
                .db
                .get_cf(block_index_cf, key.as_bytes())
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            match existing {
                Some(hash) if hash.as_ref() == block.hash.as_bytes() => {
                    index_entries += 1;
                }
                _ => {
                    batch.put_cf(&block_index_cf, key.as_bytes(), block.hash.as_bytes());
                    rebuilt_entries += 1;
                    needs_repair = true;
                    if rebuilt_entries % BLOCK_INDEX_REPORT_INTERVAL == 0 {
                        let _ = self.append_runtime_event(
                            "info",
                            "block_index_repair_progress",
                            &format!(
                                "block index repair progress: rebuilt_entries={} of total_blocks={}",
                                rebuilt_entries, total_blocks
                            ),
                        );
                    }
                }
            }
        }

        let index_iter = self
            .db
            .iterator_cf(block_index_cf, rocksdb::IteratorMode::Start);
        for item in index_iter {
            let (key, _) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            let key_str =
                std::str::from_utf8(&key).map_err(|e| PulseError::StorageError(e.to_string()))?;
            if key_str.starts_with(BLOCK_INDEX_PREFIX) && !expected_keys.contains(key_str) {
                batch.delete_cf(&block_index_cf, key);
                removed_stale_entries += 1;
                needs_repair = true;
            }
        }

        if needs_repair {
            self.db
                .write(batch)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            let _ = self.append_runtime_event(
                "info",
                "block_index_repaired",
                &format!(
                    "block index repair complete: total_blocks={}, repaired_entries={}, removed_stale_entries={}",
                    total_blocks, rebuilt_entries, removed_stale_entries
                ),
            );
        }

        Ok(BlockIndexHealth {
            total_blocks,
            index_entries: index_entries + rebuilt_entries,
            rebuilt_entries,
            removed_stale_entries,
            used_full_scan: total_blocks == 0 && (index_entries + rebuilt_entries) == 0,
        })
    }

    fn stage_chain_state_snapshot(
        &self,
        batch: &mut WriteBatch,
        meta_cf: &impl rocksdb::AsColumnFamilyRef,
        state: &ChainState,
    ) -> Result<(), PulseError> {
        let value =
            bincode::serialize(state).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let captured_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        batch.put_cf(meta_cf, CHAIN_STATE_KEY, value);
        batch.put_cf(
            meta_cf,
            SNAPSHOT_CAPTURED_AT_UNIX_KEY,
            captured_at_unix.to_string().into_bytes(),
        );
        Ok(())
    }

    pub fn load_chain_state(&self) -> Result<Option<ChainState>, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        match self
            .db
            .get_cf(cf, CHAIN_STATE_KEY)
            .map_err(|e| PulseError::StorageError(e.to_string()))?
        {
            Some(bytes) => Ok(Some(
                bincode::deserialize(&bytes)
                    .map_err(|e| PulseError::StorageError(e.to_string()))?,
            )),
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
        for block in state.dag.blocks.values() {
            self.persist_block_and_chain_state(block, &state)?;
        }
        Ok(state)
    }

    pub fn replay_blocks_or_init(&self, chain_id: String) -> Result<ChainState, PulseError> {
        let blocks = self.list_blocks()?;
        let snapshot = self.load_chain_state();
        if let Ok(Some(snapshot)) = snapshot {
            match rebuild_state_from_snapshot_and_blocks(snapshot, blocks.clone()) {
                Ok(state) => {
                    self.persist_chain_state(&state)?;
                    return Ok(state);
                }
                Err(snapshot_delta_err) => {
                    if blocks.is_empty() {
                        return Err(snapshot_delta_err);
                    }
                    let _ = self.append_runtime_event(
                        "warn",
                        "snapshot_delta_replay_failed_fallback_full",
                        &format!(
                            "snapshot+delta replay failed and full rebuild fallback engaged: {}",
                            snapshot_delta_err
                        ),
                    );
                    let state = rebuild_state_from_blocks(chain_id, blocks)?;
                    self.persist_chain_state(&state)?;
                    return Ok(state);
                }
            }
        } else if let Err(snapshot_err) = snapshot {
            if blocks.is_empty() {
                return Err(snapshot_err);
            }
            let _ = self.append_runtime_event(
                "warn",
                "snapshot_decode_failed_fallback_full",
                &format!(
                    "snapshot decode failed and full rebuild fallback engaged: {}",
                    snapshot_err
                ),
            );
            let state = rebuild_state_from_blocks(chain_id, blocks)?;
            self.persist_chain_state(&state)?;
            return Ok(state);
        }
        if blocks.is_empty() {
            return self.load_or_init_genesis(chain_id);
        }
        let state = rebuild_state_from_blocks(chain_id, blocks)?;
        self.persist_chain_state(&state)?;
        Ok(state)
    }

    pub fn replay_from_validated_snapshot_and_delta(&self) -> Result<ChainState, PulseError> {
        let snapshot = self
            .load_chain_state()?
            .ok_or_else(|| PulseError::StorageError("validated snapshot missing".to_string()))?;
        let blocks = self.list_blocks()?;
        let state = rebuild_state_from_snapshot_and_blocks(snapshot, blocks)?;
        self.persist_chain_state(&state)?;
        Ok(state)
    }

    pub fn restore_drill_snapshot_and_delta(
        &self,
        chain_id: String,
    ) -> Result<RestoreDrillReport, PulseError> {
        let started = std::time::Instant::now();
        let persisted_blocks = self.list_blocks()?;
        let persisted_block_count = persisted_blocks.len();
        let snapshot = self.load_chain_state();

        let (state, used_snapshot, fallback_to_full_rebuild) = match snapshot {
            Ok(Some(snapshot_state)) => {
                match rebuild_state_from_snapshot_and_blocks(
                    snapshot_state,
                    persisted_blocks.clone(),
                ) {
                    Ok(state) => (state, true, false),
                    Err(snapshot_delta_err) => {
                        if persisted_blocks.is_empty() {
                            return Err(snapshot_delta_err);
                        }
                        let _ = self.append_runtime_event(
                            "warn",
                            "restore_drill_snapshot_delta_failed_fallback_full",
                            &format!(
                                "restore drill snapshot+delta failed; fallback to full rebuild: {}",
                                snapshot_delta_err
                            ),
                        );
                        (
                            rebuild_state_from_blocks(chain_id.clone(), persisted_blocks)?,
                            true,
                            true,
                        )
                    }
                }
            }
            Ok(None) => {
                if persisted_blocks.is_empty() {
                    return Err(PulseError::StorageError(
                        "restore drill requires snapshot or persisted blocks".to_string(),
                    ));
                }
                (
                    rebuild_state_from_blocks(chain_id.clone(), persisted_blocks)?,
                    false,
                    true,
                )
            }
            Err(snapshot_err) => {
                if persisted_blocks.is_empty() {
                    return Err(snapshot_err);
                }
                let _ = self.append_runtime_event(
                    "warn",
                    "restore_drill_snapshot_decode_failed_fallback_full",
                    &format!(
                        "restore drill snapshot decode failed; fallback to full rebuild: {}",
                        snapshot_err
                    ),
                );
                (
                    rebuild_state_from_blocks(chain_id, persisted_blocks)?,
                    false,
                    true,
                )
            }
        };

        self.persist_chain_state(&state)?;
        let restore_duration_ms = started.elapsed().as_millis();
        let _ = self.append_runtime_event(
            "info",
            "restore_drill_completed",
            &format!(
                "restore drill completed in {} ms (used_snapshot={}, fallback_to_full_rebuild={}, best_height={})",
                restore_duration_ms, used_snapshot, fallback_to_full_rebuild, state.dag.best_height
            ),
        );
        Ok(RestoreDrillReport {
            used_snapshot,
            fallback_to_full_rebuild,
            persisted_block_count,
            best_height: state.dag.best_height,
            restore_duration_ms,
        })
    }

    pub fn snapshot_exists(&self) -> Result<bool, PulseError> {
        Ok(self.load_chain_state()?.is_some())
    }

    pub fn snapshot_captured_at_unix(&self) -> Result<Option<u64>, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let raw = self
            .db
            .get_cf(cf, SNAPSHOT_CAPTURED_AT_UNIX_KEY)
            .map_err(|e| PulseError::StorageError(e.to_string()))?;
        match raw {
            Some(bytes) => {
                let value = std::str::from_utf8(&bytes)
                    .map_err(|e| PulseError::StorageError(e.to_string()))?;
                let parsed = value
                    .parse::<u64>()
                    .map_err(|e| PulseError::StorageError(e.to_string()))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    pub fn prune_blocks_below_height(&self, keep_from_height: u64) -> Result<usize, PulseError> {
        let blocks_cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let block_index_cf = self
            .db
            .cf_handle("block_index")
            .ok_or_else(|| PulseError::StorageError("missing cf block_index".into()))?;
        let blocks = self.list_blocks()?;
        let mut removed = 0usize;
        let mut batch = WriteBatch::default();
        for block in blocks {
            if block.header.height < keep_from_height {
                batch.delete_cf(&blocks_cf, block.hash.as_bytes());
                batch.delete_cf(
                    &block_index_cf,
                    Self::block_index_key(block.header.height, &block.hash).as_bytes(),
                );
                removed += 1;
            }
        }
        if removed > 0 {
            self.db
                .write(batch)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
        }
        Ok(removed)
    }

    pub fn append_runtime_event(
        &self,
        level: &str,
        kind: &str,
        message: &str,
    ) -> Result<RuntimeEvent, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let timestamp_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let unique_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let event = RuntimeEvent {
            timestamp_unix,
            level: level.to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
        };
        let key = format!("{}{:020}", RUNTIME_EVENT_PREFIX, unique_nanos);
        let value =
            serde_json::to_vec(&event).map_err(|e| PulseError::StorageError(e.to_string()))?;
        self.db
            .put_cf(cf, key.as_bytes(), value)
            .map_err(|e| PulseError::StorageError(e.to_string()))?;
        let _ = self.prune_runtime_events(2_000);
        Ok(event)
    }

    pub fn prune_runtime_events(&self, max_events: usize) -> Result<usize, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
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
            self.db
                .delete_cf(cf, key)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
        }
        Ok(to_delete)
    }

    pub fn list_runtime_events(&self, limit: usize) -> Result<Vec<RuntimeEvent>, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut events = Vec::new();
        for item in iter {
            let (key, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            if let Ok(key_str) = std::str::from_utf8(&key) {
                if key_str.starts_with(RUNTIME_EVENT_PREFIX) {
                    let event: RuntimeEvent = serde_json::from_slice(&value)
                        .map_err(|e| PulseError::StorageError(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::Storage;
    use pulsedag_core::{
        accept::{accept_block, AcceptSource},
        build_candidate_block, build_coinbase_transaction, dev_mine_header,
        genesis::init_chain_state,
    };

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedag-storage-{}-{}", test_name, unique))
            .to_string_lossy()
            .into_owned()
    }

    fn build_linear_chain(chain_id: &str, blocks_to_add: usize) -> pulsedag_core::ChainState {
        let mut state = init_chain_state(chain_id.to_string());
        for i in 1..=blocks_to_add {
            let parent = state.dag.best_hash.clone();
            let mut block = build_candidate_block(
                vec![parent],
                i as u64,
                1,
                vec![build_coinbase_transaction("miner", 50, i as u64)],
            );
            let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
            assert!(mined, "failed to mine test block at height {}", i);
            block.header = header;
            block.hash = format!("block-{}-{}", i, block.header.nonce);
            accept_block(block, &mut state, AcceptSource::LocalMining).expect("accept mined block");
        }
        state
    }

    #[test]
    fn persist_block_and_chain_state_round_trips_genesis() {
        let path = temp_db_path("atomic-round-trip");
        let storage = Storage::open(&path).expect("open storage");
        let state = init_chain_state("testnet".to_string());
        let genesis = state
            .dag
            .blocks
            .get(&state.dag.best_hash)
            .cloned()
            .expect("genesis block");

        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist atomically");

        let loaded_state = storage
            .load_chain_state()
            .expect("load chain state")
            .expect("snapshot present");
        let loaded_block = storage
            .get_block(&genesis.hash)
            .expect("get block")
            .expect("block present");

        assert_eq!(loaded_state.dag.best_hash, state.dag.best_hash);
        assert_eq!(loaded_state.dag.best_height, state.dag.best_height);
        assert_eq!(loaded_block.hash, genesis.hash);
        assert_eq!(loaded_block.header.height, genesis.header.height);
        assert!(storage
            .snapshot_captured_at_unix()
            .expect("snapshot metadata")
            .is_some());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn persist_chain_state_still_supports_snapshot_only_updates() {
        let path = temp_db_path("snapshot-only");
        let storage = Storage::open(&path).expect("open storage");
        let state = init_chain_state("testnet".to_string());

        storage
            .persist_chain_state(&state)
            .expect("persist chain state only");

        let loaded_state = storage
            .load_chain_state()
            .expect("load chain state")
            .expect("snapshot present");
        let blocks = storage.list_blocks().expect("list blocks");

        assert_eq!(loaded_state.dag.best_hash, state.dag.best_hash);
        assert!(blocks.is_empty());
        assert!(storage
            .snapshot_captured_at_unix()
            .expect("snapshot metadata")
            .is_some());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn load_or_init_genesis_persists_block_and_snapshot() {
        let path = temp_db_path("genesis-init");
        let storage = Storage::open(&path).expect("open storage");

        let state = storage
            .load_or_init_genesis("testnet".to_string())
            .expect("load or init genesis");
        let blocks = storage.list_blocks().expect("list blocks");

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].hash, state.dag.best_hash);
        assert!(storage
            .load_chain_state()
            .expect("load chain state")
            .is_some());
        assert!(storage
            .snapshot_captured_at_unix()
            .expect("snapshot metadata")
            .is_some());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_blocks_or_init_uses_snapshot_plus_delta_after_prune() {
        let path = temp_db_path("snapshot-plus-delta");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 5);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);

        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist validated snapshot");

        storage
            .prune_blocks_below_height(4)
            .expect("prune old blocks while keeping 4+");

        let rebuilt = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("rebuild from validated snapshot plus retained delta");
        assert_eq!(rebuilt.dag.best_height, 5);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_blocks_or_init_rejects_truncated_history_without_snapshot() {
        let path = temp_db_path("reject-truncated-without-snapshot");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 4);
        let mut blocks: Vec<_> = state
            .dag
            .blocks
            .values()
            .filter(|b| b.header.height >= 3)
            .cloned()
            .collect();
        blocks.sort_by_key(|b| b.header.height);

        for block in &blocks {
            storage
                .persist_block(block)
                .expect("persist retained-only blocks");
        }

        let err = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect_err("must reject replay from truncated history without snapshot");
        let message = err.to_string();
        assert!(
            message.contains("missing parent"),
            "expected missing parent error, got: {message}"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_blocks_or_init_falls_back_when_snapshot_is_corrupt() {
        let path = temp_db_path("fallback-corrupt-snapshot");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 5);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }

        let meta_cf = storage.db.cf_handle("meta").expect("meta cf");
        storage
            .db
            .put_cf(&meta_cf, b"chain_state", b"{invalid-bincode")
            .expect("write corrupt snapshot bytes");

        let rebuilt = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("must fall back to full rebuild");
        assert_eq!(rebuilt.dag.best_height, state.dag.best_height);
        assert_eq!(rebuilt.dag.best_hash, state.dag.best_hash);
        assert_eq!(
            storage.list_blocks().expect("list persisted blocks").len(),
            blocks.len()
        );
        let events = storage.list_runtime_events(25).expect("runtime events");
        assert!(
            events
                .iter()
                .any(|e| e.kind == "snapshot_decode_failed_fallback_full"),
            "expected fallback runtime event"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_blocks_or_init_fails_explicitly_with_corrupt_snapshot_and_no_blocks() {
        let path = temp_db_path("corrupt-snapshot-no-blocks");
        let storage = Storage::open(&path).expect("open storage");
        let meta_cf = storage.db.cf_handle("meta").expect("meta cf");
        storage
            .db
            .put_cf(&meta_cf, b"chain_state", b"corrupt-bytes")
            .expect("write corrupt snapshot bytes");

        let err = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect_err("must fail explicitly when no block replay fallback exists");
        assert!(
            err.to_string().contains("Storage error"),
            "unexpected error message: {err}"
        );
        assert!(
            storage
                .list_blocks()
                .expect("list blocks after failure")
                .is_empty(),
            "no blocks should be mutated by failed replay"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn restore_drill_snapshot_and_delta_reports_timing_and_preserves_coherence() {
        let path = temp_db_path("restore-drill-report");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 6);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");
        storage
            .prune_blocks_below_height(5)
            .expect("prune history below drill retention");

        let report = storage
            .restore_drill_snapshot_and_delta("testnet".to_string())
            .expect("restore drill should succeed");
        let rebuilt = storage
            .load_chain_state()
            .expect("load state")
            .expect("snapshot should exist");

        assert!(report.used_snapshot);
        assert!(!report.fallback_to_full_rebuild);
        assert_eq!(report.best_height, 6);
        assert_eq!(rebuilt.dag.best_hash, state.dag.best_hash);
        assert!(report.restore_duration_ms < 30_000);

        let events = storage.list_runtime_events(25).expect("runtime events");
        assert!(
            events.iter().any(|e| e.kind == "restore_drill_completed"),
            "expected restore drill completion event"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn rebuild_path_matches_normal_path_final_state() {
        let path = temp_db_path("rebuild-final-state-equivalence");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 7);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }

        let rebuilt = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("replay should match normal path state");
        assert_eq!(rebuilt.dag.best_hash, state.dag.best_hash);
        assert_eq!(rebuilt.dag.best_height, state.dag.best_height);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn interrupted_rebuild_fails_cleanly_without_mutating_blocks() {
        let path = temp_db_path("interrupted-rebuild-clean-failure");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 6);
        let mut partial_blocks: Vec<_> = state
            .dag
            .blocks
            .values()
            .filter(|b| b.header.height >= 4)
            .cloned()
            .collect();
        partial_blocks.sort_by_key(|b| b.header.height);
        for block in &partial_blocks {
            storage
                .persist_block(block)
                .expect("persist partial block set");
        }

        let err = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect_err("rebuild should fail cleanly with missing parents");
        assert!(err.to_string().contains("missing parent"));
        assert_eq!(
            storage
                .list_blocks()
                .expect("read persisted blocks after failed rebuild")
                .len(),
            partial_blocks.len()
        );
    }

    #[test]
    fn index_recovers_after_partial_loss_and_stays_coherent() {
        let path = temp_db_path("index-recovery-partial-loss");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 8);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }

        let block_index_cf = storage.db.cf_handle("block_index").expect("block index cf");
        for block in blocks.iter().take(3) {
            storage
                .db
                .delete_cf(
                    &block_index_cf,
                    Storage::block_index_key(block.header.height, &block.hash).as_bytes(),
                )
                .expect("simulate partial index loss");
        }

        let loaded_blocks = storage.list_blocks().expect("list blocks repairs index");
        assert_eq!(loaded_blocks.len(), blocks.len());
        let health = storage.block_index_health().expect("index health");
        assert_eq!(health.total_blocks, blocks.len());
        assert!(health.index_entries >= blocks.len());
    }

    #[test]
    fn block_index_repair_emits_progress_events_when_repairs_occur() {
        let path = temp_db_path("index-progress-events");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }

        let block_index_cf = storage.db.cf_handle("block_index").expect("block index cf");
        let block = blocks.last().expect("have at least one block");
        storage
            .db
            .delete_cf(
                &block_index_cf,
                Storage::block_index_key(block.header.height, &block.hash).as_bytes(),
            )
            .expect("drop one index entry");

        let _ = storage.list_blocks().expect("trigger index repair");
        let events = storage.list_runtime_events(50).expect("runtime events");
        assert!(
            events.iter().any(|e| e.kind == "block_index_repaired"),
            "expected index repair event"
        );
    }
}
