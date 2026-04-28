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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PruneSafetyPlan {
    pub requested_keep_from_height: u64,
    pub effective_keep_from_height: u64,
    pub minimum_safe_keep_from_height: u64,
    pub best_height: u64,
    pub snapshot_best_height: Option<u64>,
    pub safe_restore_anchor_present: bool,
    pub can_prune: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreDrillReport {
    pub chain_id: String,
    pub used_snapshot: bool,
    pub fallback_to_full_rebuild: bool,
    pub persisted_block_count: usize,
    pub best_height: u64,
    pub best_tip_hash: String,
    pub started_at_unix: u64,
    pub completed_at_unix: u64,
    pub restore_duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotExportBundle {
    pub format_version: u32,
    pub exported_at_unix: u64,
    pub snapshot_captured_at_unix: Option<u64>,
    pub snapshot: ChainState,
    pub persisted_blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotVerificationIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotVerificationReport {
    pub format_version: u32,
    pub chain_id: String,
    pub expected_chain_id: Option<String>,
    pub snapshot_best_height: u64,
    pub persisted_block_count: usize,
    pub snapshot_anchor_present: bool,
    pub lineage_coherent: bool,
    pub chain_id_matches_expected: bool,
    pub replay_viable: bool,
    pub restore_guarantees_explicit: bool,
    pub recovery_confidence: String,
    pub confidence_reason: String,
    pub issue_count: usize,
    pub issues: Vec<SnapshotVerificationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageAuditIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageAuditReport {
    pub ok: bool,
    pub read_only: bool,
    pub deep_check_performed: bool,
    pub snapshot_exists: bool,
    pub snapshot_anchor_present: bool,
    pub snapshot_best_height: Option<u64>,
    pub persisted_block_count: usize,
    pub persisted_best_height: Option<u64>,
    pub lineage_coherent: bool,
    pub deep_replay_viable: Option<bool>,
    pub restore_drill_confirms_recovery: bool,
    pub recovery_confidence: String,
    pub confidence_reason: String,
    pub issue_count: usize,
    pub issues: Vec<StorageAuditIssue>,
}

impl Storage {
    fn detect_lineage_issues(
        snapshot_hashes: &std::collections::BTreeSet<String>,
        persisted_hashes: &std::collections::BTreeSet<String>,
        persisted_blocks: &[Block],
        snapshot_best_height: u64,
    ) -> Vec<(String, String)> {
        let mut issues = Vec::new();
        for block in persisted_blocks {
            if block.header.height <= snapshot_best_height && !snapshot_hashes.contains(&block.hash)
            {
                issues.push((
                    "DELTA_NOT_IN_SNAPSHOT".to_string(),
                    format!(
                        "persisted block {} at height {} is not present in snapshot",
                        block.hash, block.header.height
                    ),
                ));
            }
            for parent in &block.header.parents {
                if !snapshot_hashes.contains(parent) && !persisted_hashes.contains(parent) {
                    issues.push((
                        "MISSING_PARENT".to_string(),
                        format!(
                            "persisted block {} references missing parent {}",
                            block.hash, parent
                        ),
                    ));
                }
            }
        }
        issues
    }

    pub fn plan_prune_with_safety(
        &self,
        requested_keep_from_height: u64,
        best_height: u64,
        min_rollback_blocks: u64,
    ) -> Result<PruneSafetyPlan, PulseError> {
        let min_rollback_blocks = min_rollback_blocks.max(1);
        let minimum_safe_keep_from_height =
            best_height.saturating_sub(min_rollback_blocks.saturating_sub(1));
        let effective_keep_from_height =
            requested_keep_from_height.min(minimum_safe_keep_from_height);

        let snapshot = self.load_chain_state()?;
        let snapshot_best_height = snapshot.as_ref().map(|s| s.dag.best_height);
        let snapshot_anchor_present = self.snapshot_captured_at_unix()?.is_some();

        if snapshot.is_none() {
            return Ok(PruneSafetyPlan {
                requested_keep_from_height,
                effective_keep_from_height,
                minimum_safe_keep_from_height,
                best_height,
                snapshot_best_height,
                safe_restore_anchor_present: false,
                can_prune: false,
                reason: Some("validated snapshot missing".to_string()),
            });
        }

        if !snapshot_anchor_present {
            return Ok(PruneSafetyPlan {
                requested_keep_from_height,
                effective_keep_from_height,
                minimum_safe_keep_from_height,
                best_height,
                snapshot_best_height,
                safe_restore_anchor_present: false,
                can_prune: false,
                reason: Some("snapshot restore anchor metadata missing".to_string()),
            });
        }

        let snapshot_best_height = snapshot_best_height.unwrap_or(0);
        if snapshot_best_height < effective_keep_from_height {
            return Ok(PruneSafetyPlan {
                requested_keep_from_height,
                effective_keep_from_height,
                minimum_safe_keep_from_height,
                best_height,
                snapshot_best_height: Some(snapshot_best_height),
                safe_restore_anchor_present: true,
                can_prune: false,
                reason: Some(format!(
                    "snapshot height {} below effective keep_from_height {}",
                    snapshot_best_height, effective_keep_from_height
                )),
            });
        }

        Ok(PruneSafetyPlan {
            requested_keep_from_height,
            effective_keep_from_height,
            minimum_safe_keep_from_height,
            best_height,
            snapshot_best_height: Some(snapshot_best_height),
            safe_restore_anchor_present: true,
            can_prune: true,
            reason: None,
        })
    }

    fn validate_restore_inputs(
        &self,
        expected_chain_id: Option<&str>,
    ) -> Result<(ChainState, Vec<Block>), PulseError> {
        let snapshot = self
            .load_chain_state()?
            .ok_or_else(|| PulseError::StorageError("validated snapshot missing".to_string()))?;
        if let Some(chain_id) = expected_chain_id {
            if snapshot.chain_id != chain_id {
                return Err(PulseError::StorageError(format!(
                    "validated snapshot chain_id={} does not match expected {}",
                    snapshot.chain_id, chain_id
                )));
            }
        }
        if !snapshot.dag.blocks.contains_key(&snapshot.dag.genesis_hash) {
            return Err(PulseError::StorageError(
                "validated snapshot missing genesis block".to_string(),
            ));
        }
        let snapshot_max_height = snapshot
            .dag
            .blocks
            .values()
            .map(|b| b.header.height)
            .max()
            .unwrap_or(0);
        if snapshot_max_height != snapshot.dag.best_height {
            return Err(PulseError::StorageError(format!(
                "validated snapshot best_height {} does not match max DAG height {}",
                snapshot.dag.best_height, snapshot_max_height
            )));
        }
        let blocks = self.list_blocks()?;
        let snapshot_hashes = snapshot
            .dag
            .blocks
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let persisted_hashes = blocks
            .iter()
            .map(|b| b.hash.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let lineage_issues = Self::detect_lineage_issues(
            &snapshot_hashes,
            &persisted_hashes,
            &blocks,
            snapshot.dag.best_height,
        );
        if let Some((_, message)) = lineage_issues.first() {
            return Err(PulseError::StorageError(message.clone()));
        }
        Ok((snapshot, blocks))
    }

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
        let cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        self.db
            .put_cf(
                cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?,
            )
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn list_blocks(&self) -> Result<Vec<Block>, PulseError> {
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
        blocks.sort_by_key(|b| b.header.height);
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
        self.persist_block_and_chain_state_with_write(block, state, |db, batch| {
            db.write(batch)
                .map_err(|e| PulseError::StorageError(e.to_string()))
        })
    }

    fn persist_block_and_chain_state_with_write<F>(
        &self,
        block: &Block,
        state: &ChainState,
        write_batch: F,
    ) -> Result<(), PulseError>
    where
        F: FnOnce(&Arc<DB>, WriteBatch) -> Result<(), PulseError>,
    {
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
        let mut batch = WriteBatch::default();
        batch.put_cf(&blocks_cf, block.hash.as_bytes(), block_value);
        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        write_batch(&self.db, batch)
    }

    fn stage_chain_state_snapshot(
        &self,
        batch: &mut WriteBatch,
        meta_cf: &impl rocksdb::AsColumnFamilyRef,
        state: &ChainState,
    ) -> Result<(), PulseError> {
        let captured_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.stage_chain_state_snapshot_with_captured_at(batch, meta_cf, state, captured_at_unix)
    }

    fn stage_chain_state_snapshot_with_captured_at(
        &self,
        batch: &mut WriteBatch,
        meta_cf: &impl rocksdb::AsColumnFamilyRef,
        state: &ChainState,
        captured_at_unix: u64,
    ) -> Result<(), PulseError> {
        let value =
            bincode::serialize(state).map_err(|e| PulseError::StorageError(e.to_string()))?;
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

    pub fn verify_snapshot_bundle(
        &self,
        bundle: &SnapshotExportBundle,
        expected_chain_id: Option<&str>,
    ) -> SnapshotVerificationReport {
        let mut issues = Vec::new();
        if bundle.format_version != 1 {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_UNSUPPORTED_FORMAT".to_string(),
                message: format!(
                    "snapshot bundle format_version={} is unsupported (expected 1)",
                    bundle.format_version
                ),
            });
        }
        let expected_chain_id_owned = expected_chain_id.map(|v| v.to_string());
        let chain_id_matches_expected = expected_chain_id
            .map(|v| bundle.snapshot.chain_id == v)
            .unwrap_or(true);
        if !chain_id_matches_expected {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_CHAIN_ID_MISMATCH".to_string(),
                message: format!(
                    "bundle chain_id={} does not match expected {}",
                    bundle.snapshot.chain_id,
                    expected_chain_id.unwrap_or_default()
                ),
            });
        }
        if !bundle
            .snapshot
            .dag
            .blocks
            .contains_key(&bundle.snapshot.dag.genesis_hash)
        {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_MISSING_GENESIS".to_string(),
                message: "snapshot in bundle is missing genesis block".to_string(),
            });
        }

        let snapshot_max_height = bundle
            .snapshot
            .dag
            .blocks
            .values()
            .map(|b| b.header.height)
            .max()
            .unwrap_or(0);
        if snapshot_max_height != bundle.snapshot.dag.best_height {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_BEST_HEIGHT_INCOHERENT".to_string(),
                message: format!(
                    "snapshot best_height {} does not match max DAG height {}",
                    bundle.snapshot.dag.best_height, snapshot_max_height
                ),
            });
        }

        let snapshot_hashes = bundle
            .snapshot
            .dag
            .blocks
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let persisted_hashes = bundle
            .persisted_blocks
            .iter()
            .map(|b| b.hash.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let lineage_issues = Self::detect_lineage_issues(
            &snapshot_hashes,
            &persisted_hashes,
            &bundle.persisted_blocks,
            bundle.snapshot.dag.best_height,
        );
        for (code, message) in lineage_issues {
            issues.push(SnapshotVerificationIssue {
                code: format!("SNAPSHOT_BUNDLE_{code}"),
                message,
            });
        }

        let replay_viable = rebuild_state_from_snapshot_and_blocks(
            bundle.snapshot.clone(),
            bundle.persisted_blocks.clone(),
        )
        .is_ok();
        if !replay_viable {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_REPLAY_FAILED".to_string(),
                message: "snapshot+delta replay validation failed".to_string(),
            });
        }
        let snapshot_anchor_present = bundle.snapshot_captured_at_unix.is_some();
        if !snapshot_anchor_present {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_ANCHOR_MISSING".to_string(),
                message: "snapshot restore anchor metadata missing from bundle".to_string(),
            });
        }
        let lineage_coherent = !issues.iter().any(|issue| {
            issue.code == "SNAPSHOT_BUNDLE_DELTA_NOT_IN_SNAPSHOT"
                || issue.code == "SNAPSHOT_BUNDLE_MISSING_PARENT"
        });
        let (recovery_confidence, confidence_reason) =
            if !snapshot_anchor_present || !lineage_coherent || !replay_viable {
                (
                    "low".to_string(),
                    "snapshot lineage or replay evidence is incomplete".to_string(),
                )
            } else if chain_id_matches_expected {
                (
                    "high".to_string(),
                    "snapshot anchor, lineage, and replay checks all passed".to_string(),
                )
            } else {
                (
                    "medium".to_string(),
                    "replay is viable but chain_id mismatch blocks operator trust".to_string(),
                )
            };
        let restore_guarantees_explicit = issues.is_empty();
        let issue_count = issues.len();
        SnapshotVerificationReport {
            format_version: bundle.format_version,
            chain_id: bundle.snapshot.chain_id.clone(),
            expected_chain_id: expected_chain_id_owned,
            snapshot_best_height: bundle.snapshot.dag.best_height,
            persisted_block_count: bundle.persisted_blocks.len(),
            snapshot_anchor_present,
            lineage_coherent,
            chain_id_matches_expected,
            replay_viable,
            restore_guarantees_explicit,
            recovery_confidence,
            confidence_reason,
            issue_count,
            issues,
        }
    }

    pub fn export_snapshot_bundle(
        &self,
        expected_chain_id: Option<&str>,
    ) -> Result<(SnapshotExportBundle, SnapshotVerificationReport), PulseError> {
        let snapshot = self
            .load_chain_state()?
            .ok_or_else(|| PulseError::StorageError("validated snapshot missing".to_string()))?;
        let persisted_blocks = self.list_blocks()?;
        let exported_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let bundle = SnapshotExportBundle {
            format_version: 1,
            exported_at_unix,
            snapshot_captured_at_unix: self.snapshot_captured_at_unix()?,
            snapshot,
            persisted_blocks,
        };
        let report = self.verify_snapshot_bundle(&bundle, expected_chain_id);
        if !report.restore_guarantees_explicit {
            return Err(PulseError::StorageError(format!(
                "snapshot export verification failed: {}",
                report
                    .issues
                    .iter()
                    .map(|issue| format!("{}={}", issue.code, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        Ok((bundle, report))
    }

    pub fn import_snapshot_bundle(
        &self,
        bundle: SnapshotExportBundle,
        expected_chain_id: Option<&str>,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        let report = self.verify_snapshot_bundle(&bundle, expected_chain_id);
        if !report.restore_guarantees_explicit {
            return Err(PulseError::StorageError(format!(
                "snapshot import verification failed: {}",
                report
                    .issues
                    .iter()
                    .map(|issue| format!("{}={}", issue.code, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }

        let blocks_cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let existing_blocks = self.list_blocks()?;
        let mut batch = WriteBatch::default();
        for block in existing_blocks {
            batch.delete_cf(&blocks_cf, block.hash.as_bytes());
        }
        for block in &bundle.persisted_blocks {
            batch.put_cf(
                &blocks_cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?,
            );
        }
        self.stage_chain_state_snapshot_with_captured_at(
            &mut batch,
            &meta_cf,
            &bundle.snapshot,
            bundle
                .snapshot_captured_at_unix
                .unwrap_or(bundle.exported_at_unix),
        )?;
        self.db
            .write(batch)
            .map_err(|e| PulseError::StorageError(e.to_string()))?;

        Ok(report)
    }

    pub fn replay_from_validated_snapshot_and_delta(
        &self,
        expected_chain_id: Option<&str>,
    ) -> Result<ChainState, PulseError> {
        let (snapshot, blocks) = self.validate_restore_inputs(expected_chain_id)?;
        let state = rebuild_state_from_snapshot_and_blocks(snapshot, blocks)?;
        self.persist_chain_state(&state)?;
        Ok(state)
    }

    pub fn restore_drill_snapshot_and_delta(
        &self,
        chain_id: String,
    ) -> Result<RestoreDrillReport, PulseError> {
        let started = std::time::Instant::now();
        let started_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (snapshot, persisted_blocks) = self.validate_restore_inputs(Some(&chain_id))?;
        let persisted_block_count = persisted_blocks.len();
        let state = rebuild_state_from_snapshot_and_blocks(snapshot, persisted_blocks)?;
        let best_tip_hash = state
            .dag
            .tips
            .iter()
            .min()
            .cloned()
            .unwrap_or_else(|| state.dag.genesis_hash.clone());

        self.persist_chain_state(&state)?;
        let restore_duration_ms = started.elapsed().as_millis();
        let completed_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(started_at_unix);
        let _ = self.append_runtime_event(
            "info",
            "restore_drill_completed",
            &format!(
                "restore drill completed in {} ms (chain_id={}, used_snapshot={}, fallback_to_full_rebuild={}, persisted_block_count={}, best_height={}, best_tip={})",
                restore_duration_ms, chain_id, true, false, persisted_block_count, state.dag.best_height, best_tip_hash
            ),
        );
        Ok(RestoreDrillReport {
            chain_id,
            used_snapshot: true,
            fallback_to_full_rebuild: false,
            persisted_block_count,
            best_height: state.dag.best_height,
            best_tip_hash,
            started_at_unix,
            completed_at_unix,
            restore_duration_ms,
        })
    }

    pub fn snapshot_exists(&self) -> Result<bool, PulseError> {
        Ok(self.load_chain_state()?.is_some())
    }

    pub fn audit_state_integrity(
        &self,
        expected_chain_id: Option<&str>,
        deep_check: bool,
    ) -> Result<StorageAuditReport, PulseError> {
        let mut issues = Vec::new();
        let snapshot = self.load_chain_state();
        let blocks = self.list_blocks()?;
        let persisted_block_count = blocks.len();
        let persisted_best_height = blocks.iter().map(|b| b.header.height).max();
        let snapshot_anchor_present = self.snapshot_captured_at_unix()?.is_some();
        let block_hashes = blocks
            .iter()
            .map(|b| b.hash.clone())
            .collect::<std::collections::BTreeSet<_>>();

        let mut snapshot_exists = false;
        let mut snapshot_best_height = None;
        let mut deep_replay_viable = None;
        match snapshot {
            Ok(Some(state)) => {
                snapshot_exists = true;
                snapshot_best_height = Some(state.dag.best_height);
                if let Some(chain_id) = expected_chain_id {
                    if state.chain_id != chain_id {
                        issues.push(StorageAuditIssue {
                            code: "SNAPSHOT_CHAIN_ID_MISMATCH".to_string(),
                            message: format!(
                                "snapshot chain_id={} does not match expected {}",
                                state.chain_id, chain_id
                            ),
                        });
                    }
                }
            }
            Ok(None) => {}
            Err(err) => {
                issues.push(StorageAuditIssue {
                    code: "SNAPSHOT_DECODE_FAILED".to_string(),
                    message: err.to_string(),
                });
            }
        }

        for block in &blocks {
            let header = &block.header;
            if header.height == 0 {
                continue;
            }
            if header.parents.is_empty() {
                issues.push(StorageAuditIssue {
                    code: "BLOCK_MISSING_PARENTS".to_string(),
                    message: format!(
                        "block {} at height {} has no parents",
                        block.hash, header.height
                    ),
                });
            }
            for parent in &header.parents {
                if !block_hashes.contains(parent) && !snapshot_exists {
                    issues.push(StorageAuditIssue {
                        code: "BLOCK_PARENT_MISSING_IN_STORAGE".to_string(),
                        message: format!(
                            "block {} references parent {} not found in persisted set",
                            block.hash, parent
                        ),
                    });
                }
            }
        }

        if deep_check {
            if let Ok(Some(snapshot_state)) = self.load_chain_state() {
                deep_replay_viable = Some(true);
                if let Err(err) =
                    rebuild_state_from_snapshot_and_blocks(snapshot_state, blocks.clone())
                {
                    deep_replay_viable = Some(false);
                    issues.push(StorageAuditIssue {
                        code: "DEEP_REPLAY_FAILED".to_string(),
                        message: err.to_string(),
                    });
                }
            } else if !blocks.is_empty() {
                deep_replay_viable = Some(false);
                if let Some(chain_id) = expected_chain_id {
                    if let Err(err) =
                        rebuild_state_from_blocks(chain_id.to_string(), blocks.clone())
                    {
                        issues.push(StorageAuditIssue {
                            code: "DEEP_REBUILD_FAILED".to_string(),
                            message: err.to_string(),
                        });
                    }
                }
            }
        }
        let lineage_coherent = {
            if let Ok(Some(snapshot_state)) = self.load_chain_state() {
                let snapshot_hashes = snapshot_state
                    .dag
                    .blocks
                    .keys()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                Self::detect_lineage_issues(
                    &snapshot_hashes,
                    &block_hashes,
                    &blocks,
                    snapshot_state.dag.best_height,
                )
                .is_empty()
            } else {
                issues
                    .iter()
                    .all(|i| i.code != "BLOCK_PARENT_MISSING_IN_STORAGE")
            }
        };
        let restore_drill_confirms_recovery = self
            .list_runtime_events(100)?
            .into_iter()
            .any(|e| e.kind == "restore_drill_completed");
        let (recovery_confidence, confidence_reason) =
            if !snapshot_exists || !snapshot_anchor_present {
                (
                    "low".to_string(),
                    "validated snapshot and anchor metadata are both required".to_string(),
                )
            } else if !lineage_coherent || deep_replay_viable == Some(false) {
                (
                    "low".to_string(),
                    "lineage or deep replay checks failed, so recovery confidence remains low"
                        .to_string(),
                )
            } else if restore_drill_confirms_recovery {
                (
                    "high".to_string(),
                    "snapshot lineage, deep replay, and recent restore drill evidence are coherent"
                        .to_string(),
                )
            } else {
                (
                "medium".to_string(),
                "snapshot lineage and replay checks passed without recent restore drill evidence"
                    .to_string(),
            )
            };

        let issue_count = issues.len();
        Ok(StorageAuditReport {
            ok: issue_count == 0,
            read_only: true,
            deep_check_performed: deep_check,
            snapshot_exists,
            snapshot_anchor_present,
            snapshot_best_height,
            persisted_block_count,
            persisted_best_height,
            lineage_coherent,
            deep_replay_viable,
            restore_drill_confirms_recovery,
            recovery_confidence,
            confidence_reason,
            issue_count,
            issues,
        })
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
        let cf = self
            .db
            .cf_handle("blocks")
            .ok_or_else(|| PulseError::StorageError("missing cf blocks".into()))?;
        let blocks = self.list_blocks()?;
        let mut removed = 0usize;
        for block in blocks {
            if block.header.height < keep_from_height {
                self.db
                    .delete_cf(cf, block.hash.as_bytes())
                    .map_err(|e| PulseError::StorageError(e.to_string()))?;
                removed += 1;
            }
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
    use super::{SnapshotExportBundle, Storage, CHAIN_STATE_KEY};
    use proptest::prelude::*;
    use pulsedag_core::{
        accept::{accept_block, AcceptSource},
        build_candidate_block, build_coinbase_transaction, dev_mine_header,
        genesis::init_chain_state,
    };

    fn best_tip_hash(state: &pulsedag_core::ChainState) -> String {
        state
            .dag
            .tips
            .iter()
            .min()
            .cloned()
            .unwrap_or_else(|| state.dag.genesis_hash.clone())
    }

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
            let parent = best_tip_hash(&state);
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
            .get(&best_tip_hash(&state))
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

        assert_eq!(best_tip_hash(&loaded_state), best_tip_hash(&state));
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
    fn accepted_block_and_snapshot_advance_coherently() {
        let path = temp_db_path("accepted-coherent");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = init_chain_state("testnet".to_string());
        let genesis = state
            .dag
            .blocks
            .get(&best_tip_hash(&state))
            .cloned()
            .expect("genesis block");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");

        let mut block = build_candidate_block(
            vec![best_tip_hash(&state)],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 1)],
        );
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block");
        block.header = header;
        block.hash = format!("accepted-coherent-{}", block.header.nonce);
        accept_block(block.clone(), &mut state, AcceptSource::LocalMining).expect("accept block");

        storage
            .persist_block_and_chain_state(&block, &state)
            .expect("persist accepted block + snapshot");

        let snapshot = storage
            .load_chain_state()
            .expect("load snapshot")
            .expect("snapshot present");
        let persisted_block = storage
            .get_block(&block.hash)
            .expect("get block")
            .expect("block present");
        assert_eq!(snapshot.dag.best_height, persisted_block.header.height);
        assert_eq!(best_tip_hash(&snapshot), persisted_block.hash);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn simulated_interruption_before_atomic_write_leaves_no_partial_advancement() {
        let path = temp_db_path("atomic-interruption");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = init_chain_state("testnet".to_string());
        let genesis = state
            .dag
            .blocks
            .get(&best_tip_hash(&state))
            .cloned()
            .expect("genesis block");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");
        let snapshot_before = storage
            .load_chain_state()
            .expect("load snapshot before")
            .expect("snapshot before present");

        let mut block = build_candidate_block(
            vec![best_tip_hash(&state)],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 1)],
        );
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block");
        block.header = header;
        block.hash = format!("atomic-interruption-{}", block.header.nonce);
        accept_block(block.clone(), &mut state, AcceptSource::LocalMining).expect("accept block");

        let err = storage
            .persist_block_and_chain_state_with_write(&block, &state, |_db, _batch| {
                Err(pulsedag_core::errors::PulseError::StorageError(
                    "simulated interruption".to_string(),
                ))
            })
            .expect_err("simulated interruption must fail before write");
        assert!(
            err.to_string().contains("simulated interruption"),
            "unexpected error: {err}"
        );
        assert!(
            storage
                .get_block(&block.hash)
                .expect("read block")
                .is_none(),
            "block must not persist on interrupted atomic write"
        );
        let snapshot_after = storage
            .load_chain_state()
            .expect("load snapshot after")
            .expect("snapshot after present");
        assert_eq!(
            best_tip_hash(&snapshot_before),
            best_tip_hash(&snapshot_after)
        );
        assert_eq!(
            snapshot_before.dag.best_height,
            snapshot_after.dag.best_height
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn restart_recovers_cleanly_from_legacy_partial_persisted_advancement() {
        let path = temp_db_path("restart-recovers-partial");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = init_chain_state("testnet".to_string());
        let genesis = state
            .dag
            .blocks
            .get(&best_tip_hash(&state))
            .cloned()
            .expect("genesis block");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");

        let mut block = build_candidate_block(
            vec![best_tip_hash(&state)],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 1)],
        );
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block");
        block.header = header;
        block.hash = format!("restart-recovers-{}", block.header.nonce);
        accept_block(block.clone(), &mut state, AcceptSource::LocalMining).expect("accept block");

        storage
            .persist_block(&block)
            .expect("simulate legacy partial persistence");

        let rebuilt = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("restart recovery should repair partial advancement");
        assert_eq!(rebuilt.dag.best_height, 1);
        assert_eq!(best_tip_hash(&rebuilt), block.hash);
        let persisted_snapshot = storage
            .load_chain_state()
            .expect("load repaired snapshot")
            .expect("snapshot should exist");
        assert_eq!(best_tip_hash(&persisted_snapshot), block.hash);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn accepted_block_atomic_persistence_path_has_no_regression() {
        let path = temp_db_path("atomic-path-regression");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = init_chain_state("testnet".to_string());
        let genesis = state
            .dag
            .blocks
            .get(&best_tip_hash(&state))
            .cloned()
            .expect("genesis block");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");

        for height in 1..=3 {
            let mut block = build_candidate_block(
                vec![best_tip_hash(&state)],
                height,
                1,
                vec![build_coinbase_transaction("miner", 50, height)],
            );
            let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
            assert!(mined, "failed to mine test block at height {}", height);
            block.header = header;
            block.hash = format!("atomic-path-regression-{}-{}", height, block.header.nonce);
            accept_block(block.clone(), &mut state, AcceptSource::LocalMining)
                .expect("accept block");
            storage
                .persist_block_and_chain_state(&block, &state)
                .expect("persist block + snapshot");
        }

        drop(storage);
        let reopened = Storage::open(&path).expect("reopen storage");
        let snapshot = reopened
            .load_chain_state()
            .expect("load snapshot")
            .expect("snapshot present");
        let blocks = reopened.list_blocks().expect("list blocks");
        assert_eq!(snapshot.dag.best_height, 3);
        assert_eq!(
            blocks.len(),
            4,
            "genesis + 3 accepted blocks should persist"
        );
        assert!(blocks
            .iter()
            .any(|b| b.hash == best_tip_hash(&snapshot)
                && b.header.height == snapshot.dag.best_height));

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

        assert_eq!(best_tip_hash(&loaded_state), best_tip_hash(&state));
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
        assert_eq!(blocks[0].hash, best_tip_hash(&state));
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
        assert_eq!(best_tip_hash(&rebuilt), best_tip_hash(&state));
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
            err.to_string().to_lowercase().contains("storage error"),
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
        assert_eq!(report.chain_id, "testnet");
        assert_eq!(report.best_height, 6);
        assert_eq!(report.best_tip_hash, best_tip_hash(&state));
        assert!(report.completed_at_unix >= report.started_at_unix);
        assert_eq!(best_tip_hash(&rebuilt), best_tip_hash(&state));
        assert!(report.restore_duration_ms < 30_000);

        let events = storage.list_runtime_events(25).expect("runtime events");
        assert!(
            events.iter().any(|e| e.kind == "restore_drill_completed"),
            "expected restore drill completion event"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_from_validated_snapshot_and_delta_succeeds_for_valid_restore_inputs() {
        let path = temp_db_path("validated-restore-success");
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
            .expect("prune history to exercise snapshot+delta restore");
        let restored = storage
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("validated restore should succeed");

        assert_eq!(restored.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&restored), best_tip_hash(&state));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_from_validated_snapshot_and_delta_rejects_incomplete_inputs_safely() {
        let path = temp_db_path("validated-restore-fails-incomplete");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        storage
            .persist_chain_state(&state)
            .expect("persist validated snapshot");

        let mut invalid_block = build_candidate_block(
            vec!["missing-parent-hash".to_string()],
            state.dag.best_height + 1,
            1,
            vec![build_coinbase_transaction(
                "miner",
                50,
                state.dag.best_height + 1,
            )],
        );
        let (header, mined, _, _) = dev_mine_header(invalid_block.header.clone(), 25_000);
        assert!(mined, "failed to mine invalid test block");
        invalid_block.header = header;
        invalid_block.hash = format!("incomplete-restore-{}", invalid_block.header.nonce);
        storage
            .persist_block(&invalid_block)
            .expect("persist invalid delta block");

        let err = storage
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect_err("restore must fail safely on incomplete inputs");
        assert!(
            err.to_string().contains("references missing parent"),
            "unexpected error: {err}"
        );
        let snapshot_after = storage
            .load_chain_state()
            .expect("load snapshot after failed restore")
            .expect("snapshot should still exist");
        assert_eq!(
            snapshot_after.dag.best_height, state.dag.best_height,
            "failed restore must not advance stored snapshot"
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn export_import_snapshot_bundle_round_trip_is_coherent() {
        let source_path = temp_db_path("snapshot-export-source");
        let source = Storage::open(&source_path).expect("open source storage");
        let state = build_linear_chain("testnet", 6);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            source.persist_block(block).expect("persist block");
        }
        source
            .persist_chain_state(&state)
            .expect("persist source snapshot");
        source
            .prune_blocks_below_height(5)
            .expect("prune source history");

        let (bundle, export_report) = source
            .export_snapshot_bundle(Some("testnet"))
            .expect("snapshot export should verify");
        assert!(export_report.restore_guarantees_explicit);
        assert!(export_report.replay_viable);

        let target_path = temp_db_path("snapshot-export-target");
        let target = Storage::open(&target_path).expect("open target storage");
        let import_report = target
            .import_snapshot_bundle(bundle.clone(), Some("testnet"))
            .expect("snapshot import should verify");
        assert!(import_report.restore_guarantees_explicit);

        let restored = target
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("imported snapshot should be restorable");
        assert_eq!(restored.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&restored), best_tip_hash(&state));
        assert_eq!(
            target
                .snapshot_captured_at_unix()
                .expect("imported anchor timestamp"),
            bundle.snapshot_captured_at_unix
        );

        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }

    #[test]
    fn verify_snapshot_bundle_signals_missing_anchor_explicitly() {
        let path = temp_db_path("snapshot-verify-anchor-missing");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let bundle = SnapshotExportBundle {
            format_version: 1,
            exported_at_unix: 1,
            snapshot_captured_at_unix: None,
            snapshot: state.clone(),
            persisted_blocks: state.dag.blocks.values().cloned().collect(),
        };

        let report = storage.verify_snapshot_bundle(&bundle, Some("testnet"));
        assert!(!report.restore_guarantees_explicit);
        assert!(!report.snapshot_anchor_present);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "SNAPSHOT_BUNDLE_ANCHOR_MISSING"));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn restore_drill_preserves_recovery_entrypoint_coherence() {
        let path = temp_db_path("restore-drill-recovery-coherent");
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
            .expect("retain snapshot + delta window");

        let report = storage
            .restore_drill_snapshot_and_delta("testnet".to_string())
            .expect("restore drill must succeed");
        assert!(report.used_snapshot);
        assert!(!report.fallback_to_full_rebuild);

        let restarted = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("normal recovery entrypoint should remain coherent after restore");
        assert_eq!(restarted.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&restarted), best_tip_hash(&state));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn restore_drill_repeated_runs_produce_coherent_timing_evidence() {
        let path = temp_db_path("restore-drill-repeatability");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 7);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");
        storage
            .prune_blocks_below_height(6)
            .expect("retain snapshot plus compact rollback/delta window");

        let mut reports = Vec::new();
        for _ in 0..3 {
            let report = storage
                .restore_drill_snapshot_and_delta("testnet".to_string())
                .expect("repeat restore drill run should succeed");
            reports.push(report);
        }

        assert_eq!(reports.len(), 3);
        for report in &reports {
            assert_eq!(report.chain_id, "testnet");
            assert!(report.used_snapshot);
            assert!(!report.fallback_to_full_rebuild);
            assert_eq!(report.best_height, state.dag.best_height);
            assert_eq!(report.best_tip_hash, best_tip_hash(&state));
            assert!(report.completed_at_unix >= report.started_at_unix);
            assert!(report.restore_duration_ms < 30_000);
        }
        assert!(reports
            .windows(2)
            .all(|w| w[1].started_at_unix >= w[0].started_at_unix));

        let events = storage.list_runtime_events(50).expect("runtime events");
        let drill_events = events
            .iter()
            .filter(|e| e.kind == "restore_drill_completed")
            .count();
        assert!(
            drill_events >= 3,
            "expected one completion event per repeated drill run"
        );

        let restarted = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("recovery entrypoint should remain coherent after repeated drill runs");
        assert_eq!(restarted.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&restarted), best_tip_hash(&state));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn audit_confidence_surfaces_align_with_restore_drill_outcome() {
        let path = temp_db_path("audit-confidence-aligns-with-drill");
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

        let pre_drill = storage
            .audit_state_integrity(Some("testnet"), true)
            .expect("audit before drill");
        assert_eq!(pre_drill.recovery_confidence, "medium");
        assert!(!pre_drill.restore_drill_confirms_recovery);

        storage
            .restore_drill_snapshot_and_delta("testnet".to_string())
            .expect("restore drill should succeed");

        let post_drill = storage
            .audit_state_integrity(Some("testnet"), true)
            .expect("audit after drill");
        assert_eq!(post_drill.recovery_confidence, "high");
        assert!(post_drill.restore_drill_confirms_recovery);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn replay_blocks_or_init_normal_startup_path_has_no_regression_without_snapshot() {
        let path = temp_db_path("normal-startup-no-regression");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 4);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }

        let rebuilt = storage
            .replay_blocks_or_init("testnet".to_string())
            .expect("normal startup replay must still succeed");
        assert_eq!(rebuilt.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&rebuilt), best_tip_hash(&state));
        assert!(storage
            .load_chain_state()
            .expect("load restored snapshot")
            .is_some());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn audit_self_check_detects_inconsistent_snapshot() {
        let path = temp_db_path("audit-detects-inconsistent");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        let meta_cf = storage.db.cf_handle("meta").expect("meta cf");
        storage
            .db
            .put_cf(&meta_cf, CHAIN_STATE_KEY, b"corrupt")
            .expect("inject corrupt snapshot");

        let report = storage
            .audit_state_integrity(Some("testnet"), true)
            .expect("run audit");
        assert!(!report.ok);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "SNAPSHOT_DECODE_FAILED"));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn audit_self_check_passes_on_healthy_state() {
        let path = temp_db_path("audit-healthy");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 4);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist snapshot");

        let report = storage
            .audit_state_integrity(Some("testnet"), true)
            .expect("run audit");
        assert!(report.ok, "issues: {:?}", report.issues);
        assert!(report.issue_count == 0);
        assert!(report.lineage_coherent);
        assert_eq!(report.deep_replay_viable, Some(true));
        assert_eq!(report.recovery_confidence, "medium");
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn snapshot_bundle_lineage_checks_remain_coherent() {
        let path = temp_db_path("snapshot-bundle-lineage-coherent");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 5);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist snapshot");

        let (_bundle, report) = storage
            .export_snapshot_bundle(Some("testnet"))
            .expect("snapshot export should pass lineage checks");
        assert!(report.lineage_coherent);
        assert!(report.replay_viable);
        assert_eq!(report.recovery_confidence, "high");
        assert!(report.restore_guarantees_explicit);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn audit_read_only_path_does_not_mutate_state() {
        let path = temp_db_path("audit-read-only");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 2);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist snapshot");

        let before_blocks = storage.list_blocks().expect("list before");
        let before_snapshot_ts = storage
            .snapshot_captured_at_unix()
            .expect("snapshot ts before");

        let report = storage
            .audit_state_integrity(Some("testnet"), false)
            .expect("run read-only audit");
        assert!(report.read_only);
        assert_eq!(report.deep_replay_viable, None);
        assert_eq!(report.recovery_confidence, "medium");
        let after_blocks = storage.list_blocks().expect("list after");
        let after_snapshot_ts = storage
            .snapshot_captured_at_unix()
            .expect("snapshot ts after");
        assert_eq!(before_blocks.len(), after_blocks.len());
        assert_eq!(before_snapshot_ts, after_snapshot_ts);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn audit_surfaces_reflect_missing_snapshot_anchor_state() {
        let path = temp_db_path("audit-missing-anchor-surface");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist snapshot");
        let meta_cf = storage.db.cf_handle("meta").expect("meta cf");
        storage
            .db
            .delete_cf(&meta_cf, SNAPSHOT_CAPTURED_AT_UNIX_KEY)
            .expect("clear snapshot anchor metadata");

        let report = storage
            .audit_state_integrity(Some("testnet"), true)
            .expect("run audit");
        assert!(report.snapshot_exists);
        assert!(!report.snapshot_anchor_present);
        assert_eq!(report.recovery_confidence, "low");
        assert!(
            report
                .confidence_reason
                .contains("snapshot and anchor metadata"),
            "reason should describe missing anchor constraint"
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn prune_safety_plan_preserves_minimum_rollback_window() {
        let path = temp_db_path("prune-safety-min-window");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 20);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");

        let plan = storage
            .plan_prune_with_safety(19, state.dag.best_height, 8)
            .expect("build prune safety plan");
        assert!(plan.can_prune);
        assert_eq!(plan.minimum_safe_keep_from_height, 13);
        assert_eq!(plan.effective_keep_from_height, 13);

        let removed = storage
            .prune_blocks_below_height(plan.effective_keep_from_height)
            .expect("apply safe prune");
        assert!(removed > 0);
        let remaining = storage.list_blocks().expect("list remaining blocks");
        assert!(remaining
            .iter()
            .all(|b| b.header.height >= plan.effective_keep_from_height));
        assert!(remaining
            .iter()
            .any(|b| b.header.height == state.dag.best_height));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn prune_safety_plan_refuses_when_snapshot_anchor_missing() {
        let path = temp_db_path("prune-safety-missing-anchor");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 6);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        let plan = storage
            .plan_prune_with_safety(5, state.dag.best_height, 4)
            .expect("build prune safety plan");

        assert!(!plan.can_prune);
        assert!(!plan.safe_restore_anchor_present);
        assert_eq!(
            plan.reason.as_deref(),
            Some("validated snapshot missing"),
            "prune should be deferred until snapshot+anchor exists"
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn prune_safety_retains_recovery_viability_after_cleanup() {
        let path = temp_db_path("prune-safety-recovery-viable");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 12);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");

        let plan = storage
            .plan_prune_with_safety(11, state.dag.best_height, 6)
            .expect("build prune safety plan");
        assert!(plan.can_prune);
        storage
            .prune_blocks_below_height(plan.effective_keep_from_height)
            .expect("prune safely");

        let recovered = storage
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("snapshot+delta recovery should stay viable");
        assert_eq!(recovered.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&recovered), best_tip_hash(&state));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn prune_safety_plan_has_no_regression_for_normal_prune_flow() {
        let path = temp_db_path("prune-safety-normal-flow");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 10);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");

        let requested_keep_from = 5;
        let plan = storage
            .plan_prune_with_safety(requested_keep_from, state.dag.best_height, 4)
            .expect("build prune safety plan");
        assert!(plan.can_prune);
        assert_eq!(plan.effective_keep_from_height, requested_keep_from);

        let removed = storage
            .prune_blocks_below_height(plan.effective_keep_from_height)
            .expect("prune with unmodified keep_from");
        assert!(removed > 0);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn prune_safety_plan_explicitly_caps_to_rollback_window_floor() {
        let path = temp_db_path("prune-safety-explicit-rollback-cap");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 18);
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|b| b.header.height);
        for block in &blocks {
            storage.persist_block(block).expect("persist block");
        }
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");

        let requested_keep_from_height = 17;
        let min_rollback_blocks = 6;
        let plan = storage
            .plan_prune_with_safety(
                requested_keep_from_height,
                state.dag.best_height,
                min_rollback_blocks,
            )
            .expect("build prune safety plan");

        assert!(plan.can_prune);
        assert!(plan.safe_restore_anchor_present);
        assert_eq!(plan.requested_keep_from_height, requested_keep_from_height);
        assert_eq!(plan.best_height, state.dag.best_height);
        assert_eq!(
            plan.minimum_safe_keep_from_height,
            state
                .dag
                .best_height
                .saturating_sub(min_rollback_blocks.saturating_sub(1))
        );
        assert_eq!(
            plan.effective_keep_from_height, plan.minimum_safe_keep_from_height,
            "effective keep_from should be explicitly bounded to preserve rollback floor"
        );
        assert_eq!(plan.reason, None);
        let _ = std::fs::remove_dir_all(path);
    }

    proptest! {
        #[test]
        fn replay_from_snapshot_plus_pruned_blocks_preserves_tip(blocks_to_add in 2usize..6usize, prune_below in 1u64..5u64) {
            let path = temp_db_path("prop-replay-pruned");
            let storage = Storage::open(&path).expect("open storage");
            let state = build_linear_chain("testnet", blocks_to_add);
            let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
            blocks.sort_by_key(|b| b.header.height);
            for block in &blocks {
                storage.persist_block(block).expect("persist block");
            }
            storage.persist_chain_state(&state).expect("persist snapshot");
            storage.prune_blocks_below_height(prune_below).expect("prune old blocks");

            let rebuilt = storage
                .replay_blocks_or_init("testnet".to_string())
                .expect("replay after prune");

            prop_assert_eq!(rebuilt.dag.best_height, state.dag.best_height);
            prop_assert_eq!(best_tip_hash(&rebuilt), best_tip_hash(&state));
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
