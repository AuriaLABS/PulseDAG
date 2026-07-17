use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash as StdHash, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use pulsedag_core::{
    errors::PulseError,
    genesis::init_chain_state,
    rebuild_state_from_blocks, rebuild_state_from_snapshot_and_blocks,
    sort_blocks_for_deterministic_replay,
    state::ChainState,
    types::{Block, Hash, OutPoint, Utxo},
};
use rocksdb::{ColumnFamilyDescriptor, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const CHAIN_STATE_KEY: &[u8] = b"chain_state";
pub const STORAGE_SCHEMA_VERSION: u32 = 1;
const STORAGE_SCHEMA_VERSION_KEY: &[u8] = b"storage_schema_version";
const CHAIN_ID_KEY: &[u8] = b"chain_id";
const SNAPSHOT_CAPTURED_AT_UNIX_KEY: &[u8] = b"snapshot_captured_at_unix";
const SNAPSHOT_METADATA_KEY: &[u8] = b"snapshot_metadata";
const ACCEPTED_STORAGE_GENERATION_KEY: &[u8] = b"accepted_storage_generation";
const RUNTIME_EVENT_PREFIX: &str = "runtime_event:";
const ACCEPTED_BLOCKS_CF: &str = "blocks";
const ORPHAN_STAGED_BLOCKS_CF: &str = "orphan_staged_blocks";
const TERMINAL_MISSING_PARENT_CF: &str = "terminal_missing_parent_metadata";
const REJECTED_BLOCK_DIAGNOSTICS_CF: &str = "rejected_block_diagnostics";

pub static ACCEPTED_STORAGE_MEMORY_MISMATCH_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static ACCEPTED_STORAGE_ORPHAN_RECORD_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static BLOCK_COMMIT_BATCH_FAILED_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static BLOCK_COMMIT_ROLLBACK_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static STARTUP_STORAGE_RECONCILIATION_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static STARTUP_STORAGE_RECONCILIATION_FAILED_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_VERIFICATION_STABLE_FAILURE_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_VERIFICATION_RETRY_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_VERIFICATION_LAST_GENERATION: AtomicU64 = AtomicU64::new(0);
pub static SNAPSHOT_VERIFICATION_LAST_FAILED_HASH: AtomicU64 = AtomicU64::new(0);

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
pub struct AcceptedStorageInvariantReport {
    pub memory_generation: String,
    pub storage_generation: String,
    pub accepted_storage_count: usize,
    pub in_memory_dag_count: usize,
    pub in_memory_accepted_hashes: Vec<Hash>,
    pub persisted_accepted_hashes: Vec<Hash>,
    pub storage_only_hashes: Vec<Hash>,
    pub memory_only_hashes: Vec<Hash>,
    pub mismatch_acceptance_sources: BTreeMap<Hash, String>,
    pub staged_hashes_present_in_accepted_storage: Vec<Hash>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetainedSetReport {
    pub prune_boundary_height: u64,
    pub blocks_considered_total: usize,
    pub blocks_pruned_total: usize,
    pub selected_blocks_retained: usize,
    pub side_dag_blocks_retained: usize,
    pub parent_closure_blocks_retained: usize,
    pub finality_window_blocks_retained: usize,
    pub retained_storage_hash_digest: String,
    pub retained_memory_hash_digest: String,
    pub storage_only_retained_hashes: Vec<Hash>,
    pub memory_only_retained_hashes: Vec<Hash>,
    pub historical_blocks_eligible_for_deletion: Vec<Hash>,
}

fn invariant_generation(hashes: &[Hash]) -> String {
    let first = hashes.first().map(String::as_str).unwrap_or("-");
    let last = hashes.last().map(String::as_str).unwrap_or("-");
    format!("count:{}:first:{}:last:{}", hashes.len(), first, last)
}

fn retained_hash_digest(hashes: &BTreeSet<Hash>) -> String {
    let mut hasher = DefaultHasher::new();
    for hash in hashes {
        hash.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

impl AcceptedStorageInvariantReport {
    pub fn is_ok(&self) -> bool {
        self.accepted_storage_count == self.in_memory_dag_count
            && self.storage_only_hashes.is_empty()
            && self.memory_only_hashes.is_empty()
            && self.staged_hashes_present_in_accepted_storage.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartupStorageReconciliationReport {
    pub repaired: bool,
    pub quarantined_accepted_records: Vec<Hash>,
    pub missing_accepted_records: Vec<Hash>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageSchemaMetadata {
    pub schema_version: u32,
    pub compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotMetadata {
    pub chain_id: String,
    pub schema_version: u32,
    pub best_height: u64,
    pub selected_tip: String,
    pub state_root: String,
    pub created_at: u64,
    #[serde(default)]
    pub prune_boundary_height: Option<u64>,
    #[serde(default)]
    pub original_genesis_hash: Option<Hash>,
    #[serde(default)]
    pub omitted_parent_hashes: Vec<Hash>,
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
    pub snapshot_metadata: SnapshotMetadata,
    pub snapshot: ChainState,
    pub persisted_blocks: Vec<Block>,
    #[serde(default)]
    pub chain_state_generation_at_capture: u64,
    #[serde(default)]
    pub accepted_storage_generation: u64,
    #[serde(default)]
    pub snapshot_generation: u64,
    #[serde(default)]
    pub delta_start_generation: u64,
    #[serde(default)]
    pub delta_end_generation: u64,
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
    pub lineage_issue_count: usize,
    pub restore_guarantees_explicit: bool,
    pub recovery_confidence: String,
    pub confidence_reason: String,
    pub issue_count: usize,
    pub issues: Vec<SnapshotVerificationIssue>,
    #[serde(default)]
    pub verification_start_generation: u64,
    #[serde(default)]
    pub verification_end_generation: u64,
    #[serde(default)]
    pub verification_generation_changed: bool,
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
    pub recovery_confidence_non_misleading: bool,
    pub confidence_evidence_path: String,
    pub recovery_confidence: String,
    pub confidence_reason: String,
    pub issue_count: usize,
    pub issues: Vec<StorageAuditIssue>,
}

impl Storage {
    fn snapshot_bundle_for_state(
        snapshot: ChainState,
        persisted_blocks: Vec<Block>,
        exported_at_unix: u64,
        snapshot_captured_at_unix: Option<u64>,
        snapshot_metadata: SnapshotMetadata,
        accepted_storage_generation: u64,
    ) -> SnapshotExportBundle {
        let chain_state_generation_at_capture = snapshot.chain_state_generation;
        SnapshotExportBundle {
            format_version: 1,
            exported_at_unix,
            snapshot_captured_at_unix,
            snapshot_metadata,
            snapshot,
            persisted_blocks,
            chain_state_generation_at_capture,
            accepted_storage_generation,
            snapshot_generation: chain_state_generation_at_capture,
            delta_start_generation: accepted_storage_generation,
            delta_end_generation: accepted_storage_generation,
        }
    }

    fn failed_hash_metric(hash: &str) -> u64 {
        use std::hash::{Hash as StdHash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hash.hash(&mut hasher);
        hasher.finish()
    }

    fn valid_prune_checkpoint(metadata: &SnapshotMetadata, state: &ChainState) -> bool {
        let Some(boundary_height) = metadata.prune_boundary_height else {
            return false;
        };
        boundary_height > 0
            && metadata.original_genesis_hash.as_deref() == Some(state.dag.genesis_hash.as_str())
            && !metadata.omitted_parent_hashes.is_empty()
            && !state.dag.blocks.contains_key(&state.dag.genesis_hash)
            && state
                .dag
                .blocks
                .values()
                .map(|block| block.header.height)
                .min()
                == Some(boundary_height)
    }

    fn detect_lineage_issues(
        snapshot_hashes: &std::collections::BTreeSet<String>,
        persisted_hashes: &std::collections::BTreeSet<String>,
        persisted_blocks: &[Block],
        snapshot_best_height: u64,
        prune_metadata: Option<&SnapshotMetadata>,
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
                    let checkpoint_parent = prune_metadata.is_some_and(|metadata| {
                        metadata.prune_boundary_height == Some(block.header.height)
                            && metadata
                                .omitted_parent_hashes
                                .iter()
                                .any(|hash| hash == parent)
                    });
                    if !checkpoint_parent {
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
        let metadata = self.snapshot_metadata()?.ok_or_else(|| {
            PulseError::StorageError(
                "snapshot metadata missing; restore gate requires metadata".into(),
            )
        })?;
        Self::verify_snapshot_metadata_for_state(
            Some(metadata.clone()),
            &snapshot,
            expected_chain_id,
        )?;
        if !snapshot.dag.blocks.contains_key(&snapshot.dag.genesis_hash)
            && !Self::valid_prune_checkpoint(&metadata, &snapshot)
        {
            return Err(PulseError::StorageError(
                "validated snapshot missing genesis block and valid prune metadata".to_string(),
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
            Some(&metadata),
        );
        if let Some((_, message)) = lineage_issues.first() {
            return Err(PulseError::StorageError(message.clone()));
        }
        Ok((snapshot, blocks))
    }

    pub fn open(path: &str) -> Result<Self, PulseError> {
        let cfs = vec![
            ColumnFamilyDescriptor::new(ACCEPTED_BLOCKS_CF, Default::default()),
            ColumnFamilyDescriptor::new(ORPHAN_STAGED_BLOCKS_CF, Default::default()),
            ColumnFamilyDescriptor::new(TERMINAL_MISSING_PARENT_CF, Default::default()),
            ColumnFamilyDescriptor::new(REJECTED_BLOCK_DIAGNOSTICS_CF, Default::default()),
            ColumnFamilyDescriptor::new("utxos", Default::default()),
            ColumnFamilyDescriptor::new("meta", Default::default()),
            ColumnFamilyDescriptor::new("contracts_meta", Default::default()),
            ColumnFamilyDescriptor::new("contracts_storage", Default::default()),
            ColumnFamilyDescriptor::new("contracts_receipts", Default::default()),
        ];
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let storage = Self {
            db: Arc::new(
                DB::open_cf_descriptors(&opts, path, cfs)
                    .map_err(|e| PulseError::StorageError(e.to_string()))?,
            ),
        };
        storage.ensure_schema_compatible()?;
        storage.reconcile_accepted_storage_at_startup()?;
        Ok(storage)
    }

    pub fn storage_schema_metadata(&self) -> Result<StorageSchemaMetadata, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let schema_version = match self
            .db
            .get_cf(&cf, STORAGE_SCHEMA_VERSION_KEY)
            .map_err(|e| PulseError::StorageError(e.to_string()))?
        {
            Some(bytes) => std::str::from_utf8(&bytes)
                .map_err(|_| {
                    PulseError::StorageError(
                        "storage schema version metadata is corrupt or not utf-8".into(),
                    )
                })?
                .parse::<u32>()
                .map_err(|_| {
                    PulseError::StorageError(
                        "storage schema version metadata is corrupt or not a number".into(),
                    )
                })?,
            None => 0,
        };
        Ok(StorageSchemaMetadata {
            schema_version,
            compatible: schema_version == STORAGE_SCHEMA_VERSION,
        })
    }

    pub fn ensure_schema_compatible(&self) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let raw = self
            .db
            .get_cf(&cf, STORAGE_SCHEMA_VERSION_KEY)
            .map_err(|e| PulseError::StorageError(e.to_string()))?;
        match raw {
            Some(bytes) => {
                let version = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|raw| raw.parse::<u32>().ok())
                    .ok_or_else(|| {
                        PulseError::StorageError(
                            "storage schema version metadata is corrupt or invalid; export/snapshot the database before attempting manual repair".into(),
                        )
                    })?;
                if version > STORAGE_SCHEMA_VERSION {
                    return Err(PulseError::StorageError(format!(
                        "unsupported future storage schema version {version}; this node supports schema {STORAGE_SCHEMA_VERSION}. Start with a newer PulseDAG binary or restore from a compatible snapshot/export."
                    )));
                }
                if version < STORAGE_SCHEMA_VERSION {
                    return Err(PulseError::StorageError(format!(
                        "storage schema version {version} is older than node schema {STORAGE_SCHEMA_VERSION}; no automatic migration is available in v2.2.14. Export/snapshot before migrating."
                    )));
                }
            }
            None => {
                self.db
                    .put_cf(
                        &cf,
                        STORAGE_SCHEMA_VERSION_KEY,
                        STORAGE_SCHEMA_VERSION.to_string(),
                    )
                    .map_err(|e| PulseError::StorageError(e.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn reconcile_accepted_storage_at_startup(
        &self,
    ) -> Result<StartupStorageReconciliationReport, PulseError> {
        let Some(snapshot) = self.load_chain_state()? else {
            return Ok(StartupStorageReconciliationReport {
                repaired: false,
                quarantined_accepted_records: Vec::new(),
                missing_accepted_records: Vec::new(),
            });
        };
        let accepted_cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let quarantine_cf = self
            .db
            .cf_handle(TERMINAL_MISSING_PARENT_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf terminal missing parent".into()))?;
        let accepted = self.list_blocks()?;
        let accepted_hashes = accepted
            .iter()
            .map(|block| block.hash.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let memory_hashes = snapshot
            .dag
            .blocks
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();

        let mut batch = WriteBatch::default();
        let mut quarantined = Vec::new();
        for block in accepted {
            if !memory_hashes.contains(&block.hash) {
                batch.delete_cf(&accepted_cf, block.hash.as_bytes());
                batch.put_cf(
                    &quarantine_cf,
                    block.hash.as_bytes(),
                    serde_json::to_vec(&block)
                        .map_err(|e| PulseError::StorageError(e.to_string()))?,
                );
                quarantined.push(block.hash);
            }
        }
        let missing = memory_hashes
            .difference(&accepted_hashes)
            .cloned()
            .collect::<Vec<_>>();
        let min_accepted_height = accepted_hashes
            .iter()
            .filter_map(|hash| {
                snapshot
                    .dag
                    .blocks
                    .get(hash)
                    .map(|block| block.header.height)
            })
            .min();
        let missing_only_below_retained_floor = min_accepted_height
            .map(|floor| {
                missing.iter().all(|hash| {
                    snapshot
                        .dag
                        .blocks
                        .get(hash)
                        .map(|block| block.header.height < floor)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !missing.is_empty() && !missing_only_below_retained_floor {
            STARTUP_STORAGE_RECONCILIATION_FAILED_TOTAL.fetch_add(1, Ordering::Relaxed);
            return Err(PulseError::StorageError(format!(
                "startup accepted storage reconciliation failed: snapshot references missing accepted block records: {}",
                missing.join(",")
            )));
        }
        if !quarantined.is_empty() {
            STARTUP_STORAGE_RECONCILIATION_TOTAL.fetch_add(1, Ordering::Relaxed);
            self.db
                .write(batch)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            let _ = self.append_runtime_event(
                "warn",
                "startup_storage_reconciliation",
                &format!(
                    "quarantined {} accepted block record(s) not referenced by persisted chain metadata",
                    quarantined.len()
                ),
            );
        }
        Ok(StartupStorageReconciliationReport {
            repaired: !quarantined.is_empty(),
            quarantined_accepted_records: quarantined,
            missing_accepted_records: missing,
        })
    }

    pub fn persist_block(&self, block: &Block) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let mut batch = WriteBatch::default();
        batch.put_cf(
            &cf,
            block.hash.as_bytes(),
            serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?,
        );
        self.stage_accepted_storage_generation_advance(&mut batch, &meta_cf)?;
        self.db
            .write(batch)
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn accepted_storage_generation(&self) -> Result<u64, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        match self
            .db
            .get_cf(cf, ACCEPTED_STORAGE_GENERATION_KEY)
            .map_err(|e| PulseError::StorageError(e.to_string()))?
        {
            Some(bytes) => std::str::from_utf8(&bytes)
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .ok_or_else(|| {
                    PulseError::StorageError(
                        "accepted storage generation metadata is corrupt".into(),
                    )
                }),
            None => Ok(0),
        }
    }

    fn stage_accepted_storage_generation_advance(
        &self,
        batch: &mut WriteBatch,
        meta_cf: &impl rocksdb::AsColumnFamilyRef,
    ) -> Result<u64, PulseError> {
        let next = self.accepted_storage_generation()?.saturating_add(1);
        batch.put_cf(
            meta_cf,
            ACCEPTED_STORAGE_GENERATION_KEY,
            next.to_string().into_bytes(),
        );
        Ok(next)
    }

    pub fn list_blocks(&self) -> Result<Vec<Block>, PulseError> {
        let cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut blocks = Vec::new();
        for item in iter {
            let (_, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            let block: Block = serde_json::from_slice(&value)
                .map_err(|e| PulseError::StorageError(e.to_string()))?;
            blocks.push(block);
        }
        sort_blocks_for_deterministic_replay(&mut blocks);
        Ok(blocks)
    }

    pub fn block_count(&self) -> Result<usize, PulseError> {
        let cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut count = 0usize;
        for item in iter {
            item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            count = count.saturating_add(1);
        }
        Ok(count)
    }

    pub fn persist_staged_orphan_block(&self, block: &Block) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle(ORPHAN_STAGED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf orphan staged blocks".into()))?;
        ACCEPTED_STORAGE_ORPHAN_RECORD_TOTAL.fetch_add(1, Ordering::Relaxed);
        self.db
            .put_cf(
                cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?,
            )
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn delete_staged_orphan_block(&self, hash: &Hash) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle(ORPHAN_STAGED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf orphan staged blocks".into()))?;
        self.db
            .delete_cf(cf, hash.as_bytes())
            .map_err(|e| PulseError::StorageError(e.to_string()))
    }

    pub fn list_staged_orphan_blocks(&self) -> Result<Vec<Block>, PulseError> {
        let cf = self
            .db
            .cf_handle(ORPHAN_STAGED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf orphan staged blocks".into()))?;
        let mut blocks = Vec::new();
        for item in self.db.iterator_cf(cf, rocksdb::IteratorMode::Start) {
            let (_, value) = item.map_err(|e| PulseError::StorageError(e.to_string()))?;
            blocks.push(
                serde_json::from_slice(&value)
                    .map_err(|e| PulseError::StorageError(e.to_string()))?,
            );
        }
        sort_blocks_for_deterministic_replay(&mut blocks);
        Ok(blocks)
    }

    pub fn verify_accepted_storage_invariants(
        &self,
        state: &ChainState,
    ) -> Result<AcceptedStorageInvariantReport, PulseError> {
        let accepted_hashes = self
            .list_blocks()?
            .into_iter()
            .map(|block| block.hash)
            .collect::<BTreeSet<_>>();
        let memory_hashes = state.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
        let staged_hashes = self
            .list_staged_orphan_blocks()?
            .into_iter()
            .map(|block| block.hash)
            .collect::<BTreeSet<_>>();
        let storage_only_hashes = accepted_hashes
            .difference(&memory_hashes)
            .cloned()
            .collect::<Vec<_>>();
        let memory_only_hashes = memory_hashes
            .difference(&accepted_hashes)
            .cloned()
            .collect::<Vec<_>>();
        let mut mismatch_acceptance_sources = BTreeMap::new();
        for hash in storage_only_hashes.iter().chain(memory_only_hashes.iter()) {
            let source = if state.orphan_blocks.contains_key(hash) || staged_hashes.contains(hash) {
                "staged_or_missing_parent"
            } else if state.terminal_missing_parents.contains_key(hash) {
                "terminal_or_quarantined_missing_parent"
            } else if state.dag.blocks.contains_key(hash) {
                "in_memory_accepted"
            } else if accepted_hashes.contains(hash) {
                "persisted_accepted_unreferenced"
            } else {
                "unknown"
            };
            mismatch_acceptance_sources.insert(hash.clone(), source.to_string());
        }
        let in_memory_accepted_hashes = memory_hashes.iter().cloned().collect::<Vec<_>>();
        let persisted_accepted_hashes = accepted_hashes.iter().cloned().collect::<Vec<_>>();
        let report = AcceptedStorageInvariantReport {
            memory_generation: invariant_generation(&in_memory_accepted_hashes),
            storage_generation: invariant_generation(&persisted_accepted_hashes),
            accepted_storage_count: accepted_hashes.len(),
            in_memory_dag_count: memory_hashes.len(),
            in_memory_accepted_hashes,
            persisted_accepted_hashes,
            storage_only_hashes,
            memory_only_hashes,
            mismatch_acceptance_sources,
            staged_hashes_present_in_accepted_storage: staged_hashes
                .intersection(&accepted_hashes)
                .cloned()
                .collect(),
        };
        if !report.is_ok() {
            ACCEPTED_STORAGE_MEMORY_MISMATCH_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
        Ok(report)
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
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
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
        let captured_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.persist_chain_state_with_captured_at(state, captured_at_unix)
    }

    fn persist_chain_state_with_captured_at(
        &self,
        state: &ChainState,
        captured_at_unix: u64,
    ) -> Result<(), PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let mut batch = WriteBatch::default();
        self.stage_chain_state_snapshot_with_captured_at(&mut batch, &cf, state, captured_at_unix)?;
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
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let block_value =
            serde_json::to_vec(block).map_err(|e| PulseError::StorageError(e.to_string()))?;
        let mut batch = WriteBatch::default();
        batch.put_cf(&blocks_cf, block.hash.as_bytes(), block_value);
        self.stage_accepted_storage_generation_advance(&mut batch, &meta_cf)?;
        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        match write_batch(&self.db, batch) {
            Ok(()) => Ok(()),
            Err(err) => {
                BLOCK_COMMIT_BATCH_FAILED_TOTAL.fetch_add(1, Ordering::Relaxed);
                Err(err)
            }
        }
    }

    fn snapshot_metadata_for_state(state: &ChainState, created_at: u64) -> SnapshotMetadata {
        let selected_tip = state
            .dag
            .tips
            .iter()
            .min()
            .cloned()
            .unwrap_or_else(|| state.dag.genesis_hash.clone());
        let state_root = state
            .dag
            .blocks
            .get(&selected_tip)
            .map(|block| block.header.state_root.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let prune_boundary_height = if state.dag.blocks.contains_key(&state.dag.genesis_hash) {
            None
        } else {
            state
                .dag
                .blocks
                .values()
                .map(|block| block.header.height)
                .min()
        };
        let mut omitted_parent_hashes = prune_boundary_height
            .map(|boundary| {
                state
                    .dag
                    .blocks
                    .values()
                    .filter(|block| block.header.height == boundary)
                    .flat_map(|block| block.header.parents.iter().cloned())
                    .filter(|parent| !state.dag.blocks.contains_key(parent))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        omitted_parent_hashes.sort();
        SnapshotMetadata {
            chain_id: state.chain_id.clone(),
            schema_version: STORAGE_SCHEMA_VERSION,
            best_height: state.dag.best_height,
            selected_tip,
            state_root,
            created_at,
            prune_boundary_height,
            original_genesis_hash: prune_boundary_height.map(|_| state.dag.genesis_hash.clone()),
            omitted_parent_hashes,
        }
    }

    pub fn snapshot_metadata(&self) -> Result<Option<SnapshotMetadata>, PulseError> {
        let cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        match self
            .db
            .get_cf(cf, SNAPSHOT_METADATA_KEY)
            .map_err(|e| PulseError::StorageError(e.to_string()))?
        {
            Some(bytes) => Ok(Some(
                serde_json::from_slice(&bytes)
                    .map_err(|e| PulseError::StorageError(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    fn verify_snapshot_metadata_for_state(
        metadata: Option<SnapshotMetadata>,
        state: &ChainState,
        expected_chain_id: Option<&str>,
    ) -> Result<(), PulseError> {
        if let Some(chain_id) = expected_chain_id {
            if state.chain_id != chain_id {
                return Err(PulseError::StorageError(format!(
                    "snapshot chain_id={} does not match expected {}",
                    state.chain_id, chain_id
                )));
            }
        }
        let Some(metadata) = metadata else {
            return Err(PulseError::StorageError(
                "snapshot metadata missing; restore gate requires metadata".to_string(),
            ));
        };
        if metadata.chain_id != state.chain_id {
            return Err(PulseError::StorageError(format!(
                "snapshot metadata chain_id={} does not match state chain_id={}",
                metadata.chain_id, state.chain_id
            )));
        }
        if metadata.schema_version != STORAGE_SCHEMA_VERSION {
            return Err(PulseError::StorageError(format!(
                "snapshot schema_version={} is not compatible with node schema {}",
                metadata.schema_version, STORAGE_SCHEMA_VERSION
            )));
        }
        let computed = Self::snapshot_metadata_for_state(state, metadata.created_at);
        if metadata.best_height != computed.best_height
            || metadata.selected_tip != computed.selected_tip
            || metadata.state_root != computed.state_root
            || metadata.prune_boundary_height != computed.prune_boundary_height
            || metadata.original_genesis_hash != computed.original_genesis_hash
            || metadata.omitted_parent_hashes != computed.omitted_parent_hashes
        {
            return Err(PulseError::StorageError(format!(
                "snapshot metadata mismatch: expected height={} tip={} state_root={}, got height={} tip={} state_root={}",
                computed.best_height,
                computed.selected_tip,
                computed.state_root,
                metadata.best_height,
                metadata.selected_tip,
                metadata.state_root
            )));
        }
        Ok(())
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
        let metadata = Self::snapshot_metadata_for_state(state, captured_at_unix);
        batch.put_cf(meta_cf, CHAIN_STATE_KEY, value);
        batch.put_cf(
            meta_cf,
            STORAGE_SCHEMA_VERSION_KEY,
            STORAGE_SCHEMA_VERSION.to_string(),
        );
        batch.put_cf(meta_cf, CHAIN_ID_KEY, state.chain_id.as_bytes());
        batch.put_cf(
            meta_cf,
            SNAPSHOT_METADATA_KEY,
            serde_json::to_vec(&metadata).map_err(|e| PulseError::StorageError(e.to_string()))?,
        );
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
        match self.load_chain_state() {
            Ok(Some(state)) => return Ok(state),
            Ok(None) => {}
            Err(snapshot_err) => {
                let blocks = self.list_blocks()?;
                if blocks.is_empty() {
                    return Err(snapshot_err);
                }
                let _ = self.append_runtime_event(
                    "warn",
                    "startup_snapshot_decode_failed_fallback_full",
                    &format!(
                        "startup snapshot decode failed and full rebuild fallback engaged: {}",
                        snapshot_err
                    ),
                );
                let rebuilt = rebuild_state_from_blocks(chain_id, blocks)?;
                self.persist_chain_state(&rebuilt)?;
                return Ok(rebuilt);
            }
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
        let verification_start_generation = self.accepted_storage_generation().unwrap_or(0);
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
            && !Self::valid_prune_checkpoint(&bundle.snapshot_metadata, &bundle.snapshot)
        {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_MISSING_GENESIS".to_string(),
                message: "snapshot is missing genesis and valid prune metadata".to_string(),
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

        let verification_generation_changed = bundle.delta_start_generation
            != bundle.delta_end_generation
            || bundle.snapshot_generation != bundle.chain_state_generation_at_capture;
        if verification_generation_changed {
            SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL.fetch_add(1, Ordering::Relaxed);
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_VERIFICATION_GENERATION_CHANGED".to_string(),
                message: format!(
                    "verification_generation_changed: snapshot_generation={} capture_generation={} delta_start_generation={} delta_end_generation={} accepted_storage_generation={} verification_start_generation={}",
                    bundle.snapshot_generation,
                    bundle.chain_state_generation_at_capture,
                    bundle.delta_start_generation,
                    bundle.delta_end_generation,
                    bundle.accepted_storage_generation,
                    verification_start_generation
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
        let lineage_issues = if verification_generation_changed {
            Vec::new()
        } else {
            Self::detect_lineage_issues(
                &snapshot_hashes,
                &persisted_hashes,
                &bundle.persisted_blocks,
                bundle.snapshot.dag.best_height,
                Some(&bundle.snapshot_metadata),
            )
        };
        let lineage_issue_count = lineage_issues.len();
        for (code, message) in lineage_issues {
            if code == "DELTA_NOT_IN_SNAPSHOT" {
                SNAPSHOT_VERIFICATION_STABLE_FAILURE_TOTAL.fetch_add(1, Ordering::Relaxed);
                SNAPSHOT_VERIFICATION_LAST_GENERATION
                    .store(bundle.snapshot_generation, Ordering::Relaxed);
                if let Some(hash) = message.split_whitespace().nth(2) {
                    SNAPSHOT_VERIFICATION_LAST_FAILED_HASH
                        .store(Self::failed_hash_metric(hash), Ordering::Relaxed);
                }
            }
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
        let expected_metadata = Self::snapshot_metadata_for_state(
            &bundle.snapshot,
            bundle.snapshot_metadata.created_at,
        );
        if bundle.snapshot_metadata.chain_id != expected_metadata.chain_id {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_METADATA_CHAIN_ID_MISMATCH".to_string(),
                message: format!(
                    "snapshot metadata chain_id={} does not match state chain_id={}",
                    bundle.snapshot_metadata.chain_id, expected_metadata.chain_id
                ),
            });
        }
        if bundle.snapshot_metadata.schema_version != STORAGE_SCHEMA_VERSION {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_SCHEMA_VERSION_MISMATCH".to_string(),
                message: format!(
                    "snapshot schema_version={} is unsupported (expected {})",
                    bundle.snapshot_metadata.schema_version, STORAGE_SCHEMA_VERSION
                ),
            });
        }
        if bundle.snapshot_metadata.best_height != expected_metadata.best_height
            || bundle.snapshot_metadata.selected_tip != expected_metadata.selected_tip
            || bundle.snapshot_metadata.state_root != expected_metadata.state_root
            || bundle.snapshot_metadata.prune_boundary_height
                != expected_metadata.prune_boundary_height
            || bundle.snapshot_metadata.original_genesis_hash
                != expected_metadata.original_genesis_hash
            || bundle.snapshot_metadata.omitted_parent_hashes
                != expected_metadata.omitted_parent_hashes
        {
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_METADATA_STATE_ROOT_MISMATCH".to_string(),
                message: format!(
                    "snapshot metadata does not match state: expected height={} tip={} state_root={}, got height={} tip={} state_root={}",
                    expected_metadata.best_height,
                    expected_metadata.selected_tip,
                    expected_metadata.state_root,
                    bundle.snapshot_metadata.best_height,
                    bundle.snapshot_metadata.selected_tip,
                    bundle.snapshot_metadata.state_root
                ),
            });
        }
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
        let verification_end_generation = self.accepted_storage_generation().unwrap_or(0);
        let verification_generation_changed = verification_generation_changed
            || verification_start_generation != verification_end_generation;
        if verification_start_generation != verification_end_generation {
            SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL.fetch_add(1, Ordering::Relaxed);
            issues.push(SnapshotVerificationIssue {
                code: "SNAPSHOT_BUNDLE_VERIFICATION_GENERATION_CHANGED".to_string(),
                message: format!(
                    "verification_generation_changed: verification_start_generation={} verification_end_generation={}",
                    verification_start_generation, verification_end_generation
                ),
            });
        }
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
            lineage_issue_count,
            restore_guarantees_explicit,
            recovery_confidence,
            confidence_reason,
            issue_count,
            issues,
            verification_start_generation,
            verification_end_generation,
            verification_generation_changed,
        }
    }

    pub fn export_snapshot_bundle(
        &self,
        expected_chain_id: Option<&str>,
    ) -> Result<(SnapshotExportBundle, SnapshotVerificationReport), PulseError> {
        for attempt in 0..3 {
            let accepted_start_generation = self.accepted_storage_generation()?;
            let snapshot = self.load_chain_state()?.ok_or_else(|| {
                PulseError::StorageError("validated snapshot missing".to_string())
            })?;
            let persisted_blocks = self.list_blocks()?;
            let accepted_end_generation = self.accepted_storage_generation()?;
            let exported_at_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut bundle = Self::snapshot_bundle_for_state(
                snapshot.clone(),
                persisted_blocks,
                exported_at_unix,
                self.snapshot_captured_at_unix()?,
                self.snapshot_metadata()?.unwrap_or_else(|| {
                    Self::snapshot_metadata_for_state(&snapshot, exported_at_unix)
                }),
                accepted_end_generation,
            );
            bundle.delta_start_generation = accepted_start_generation;
            bundle.delta_end_generation = accepted_end_generation;

            let report = self.verify_snapshot_bundle(&bundle, expected_chain_id);
            if report.verification_generation_changed {
                SNAPSHOT_VERIFICATION_RETRY_TOTAL.fetch_add(1, Ordering::Relaxed);
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(10 * (attempt + 1)));
                    continue;
                }
                return Err(PulseError::StorageError(format!(
                    "snapshot export verification discarded after generation-changed retries: {}",
                    report
                        .issues
                        .iter()
                        .map(|issue| format!("{}={}", issue.code, issue.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
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
            return Ok((bundle, report));
        }
        unreachable!("bounded snapshot verification retry loop always returns")
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
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
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
        let captured_at_unix = self.snapshot_captured_at_unix()?.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
        self.persist_chain_state_with_captured_at(&state, captured_at_unix)?;
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
                let metadata = self.snapshot_metadata().ok().flatten();
                Self::detect_lineage_issues(
                    &snapshot_hashes,
                    &block_hashes,
                    &blocks,
                    snapshot_state.dag.best_height,
                    metadata.as_ref(),
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
        let (recovery_confidence, confidence_reason, confidence_evidence_path) =
            if !snapshot_exists || !snapshot_anchor_present {
                (
                    "low".to_string(),
                    "validated snapshot and anchor metadata are both required".to_string(),
                    "snapshot+anchor".to_string(),
                )
            } else if !lineage_coherent || deep_replay_viable == Some(false) {
                (
                    "low".to_string(),
                    "lineage or deep replay checks failed, so recovery confidence remains low"
                        .to_string(),
                    "lineage+deep_replay".to_string(),
                )
            } else if restore_drill_confirms_recovery {
                (
                    "high".to_string(),
                    "snapshot lineage, deep replay, and recent restore drill evidence are coherent"
                        .to_string(),
                    "lineage+deep_replay+restore_drill".to_string(),
                )
            } else {
                (
                "medium".to_string(),
                "snapshot lineage and replay checks passed without recent restore drill evidence"
                    .to_string(),
                "lineage+deep_replay".to_string(),
            )
            };
        let recovery_confidence_non_misleading =
            recovery_confidence != "high" || restore_drill_confirms_recovery;

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
            recovery_confidence_non_misleading,
            confidence_evidence_path,
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

    /// Atomically replace the persisted snapshot and accepted block table with
    /// one verified retained set. Callers must hold the chain mutation lock.
    pub fn commit_compact_prune(
        &self,
        compact_state: &ChainState,
        retained_hashes: &BTreeSet<Hash>,
        expected_generation: u64,
    ) -> Result<usize, PulseError> {
        self.commit_compact_prune_with_write(
            compact_state,
            retained_hashes,
            expected_generation,
            |db, batch| {
                db.write(batch)
                    .map_err(|e| PulseError::StorageError(e.to_string()))
            },
        )
    }

    fn commit_compact_prune_with_write<F>(
        &self,
        compact_state: &ChainState,
        retained_hashes: &BTreeSet<Hash>,
        expected_generation: u64,
        write_batch: F,
    ) -> Result<usize, PulseError>
    where
        F: FnOnce(&Arc<DB>, WriteBatch) -> Result<(), PulseError>,
    {
        let current_generation = self.accepted_storage_generation()?;
        if current_generation != expected_generation {
            return Err(PulseError::StorageError(format!(
                "accepted storage generation changed during prune: expected {}, found {}",
                expected_generation, current_generation
            )));
        }
        let state_hashes = compact_state
            .dag
            .blocks
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        if &state_hashes != retained_hashes {
            return Err(PulseError::StorageError(format!(
                "compact state retained set has {} hashes but commit requested {}",
                state_hashes.len(),
                retained_hashes.len()
            )));
        }

        let blocks_cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| PulseError::StorageError("missing cf meta".into()))?;
        let persisted_blocks = self.list_blocks()?;
        let persisted_hashes = persisted_blocks
            .iter()
            .map(|block| block.hash.clone())
            .collect::<BTreeSet<_>>();
        let missing = retained_hashes
            .difference(&persisted_hashes)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(PulseError::StorageError(format!(
                "compact prune retained hashes are absent from accepted storage: {}",
                missing.join(",")
            )));
        }

        let mut batch = WriteBatch::default();
        let mut removed = 0usize;
        for block in persisted_blocks {
            if !retained_hashes.contains(&block.hash) {
                batch.delete_cf(&blocks_cf, block.hash.as_bytes());
                removed = removed.saturating_add(1);
            }
        }
        self.stage_accepted_storage_generation_advance(&mut batch, &meta_cf)?;
        self.stage_chain_state_snapshot(&mut batch, &meta_cf, compact_state)?;
        write_batch(&self.db, batch)?;
        Ok(removed)
    }

    pub fn prune_blocks_below_height(&self, keep_from_height: u64) -> Result<usize, PulseError> {
        let cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
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

    pub fn retained_set_report(
        &self,
        state: &ChainState,
        prune_boundary_height: u64,
    ) -> Result<RetainedSetReport, PulseError> {
        let storage_blocks = self.list_blocks()?;
        let storage_hashes = storage_blocks
            .iter()
            .map(|block| block.hash.clone())
            .collect::<BTreeSet<_>>();
        let finality_window = state
            .dag
            .blocks
            .values()
            .filter(|block| block.header.height >= prune_boundary_height)
            .map(|block| block.hash.clone())
            .collect::<BTreeSet<_>>();
        let selected = state
            .dag
            .selected_chain
            .iter()
            .filter(|hash| finality_window.contains(*hash))
            .cloned()
            .collect::<BTreeSet<_>>();
        let side_dag = finality_window
            .difference(&selected)
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut parent_closure = BTreeSet::new();
        let mut stack = finality_window.iter().cloned().collect::<Vec<_>>();
        while let Some(hash) = stack.pop() {
            if !parent_closure.insert(hash.clone()) {
                continue;
            }
            if let Some(block) = state.dag.blocks.get(&hash) {
                for parent in &block.header.parents {
                    if let Some(parent_block) = state.dag.blocks.get(parent) {
                        parent_closure.insert(parent.clone());
                        if parent_block.header.height >= prune_boundary_height {
                            stack.push(parent.clone());
                        }
                    }
                }
            }
        }
        let retained_memory_hashes = finality_window
            .union(&parent_closure)
            .cloned()
            .collect::<BTreeSet<_>>();
        let retained_storage_hashes = storage_hashes
            .intersection(&retained_memory_hashes)
            .cloned()
            .collect::<BTreeSet<_>>();
        let historical_blocks_eligible_for_deletion = state
            .dag
            .blocks
            .values()
            .filter(|block| {
                block.header.height < prune_boundary_height
                    && !retained_memory_hashes.contains(&block.hash)
            })
            .map(|block| block.hash.clone())
            .collect::<Vec<_>>();
        Ok(RetainedSetReport {
            prune_boundary_height,
            blocks_considered_total: storage_blocks.len(),
            blocks_pruned_total: historical_blocks_eligible_for_deletion
                .iter()
                .filter(|hash| !storage_hashes.contains(*hash))
                .count(),
            selected_blocks_retained: selected.len(),
            side_dag_blocks_retained: side_dag.len(),
            parent_closure_blocks_retained: parent_closure.difference(&finality_window).count(),
            finality_window_blocks_retained: finality_window.len(),
            retained_storage_hash_digest: retained_hash_digest(&retained_storage_hashes),
            retained_memory_hash_digest: retained_hash_digest(&retained_memory_hashes),
            storage_only_retained_hashes: retained_storage_hashes
                .difference(&retained_memory_hashes)
                .cloned()
                .collect(),
            memory_only_retained_hashes: retained_memory_hashes
                .difference(&retained_storage_hashes)
                .cloned()
                .collect(),
            historical_blocks_eligible_for_deletion,
        })
    }

    pub fn prune_blocks_with_retained_set(
        &self,
        state: &ChainState,
        prune_boundary_height: u64,
    ) -> Result<RetainedSetReport, PulseError> {
        let before = self.retained_set_report(state, prune_boundary_height)?;
        let cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| PulseError::StorageError("missing cf accepted blocks".into()))?;
        let mut pruned = 0usize;
        for hash in &before.historical_blocks_eligible_for_deletion {
            if self.get_block(hash)?.is_some() {
                self.db
                    .delete_cf(cf, hash.as_bytes())
                    .map_err(|e| PulseError::StorageError(e.to_string()))?;
                pruned += 1;
            }
        }
        let mut after = self.retained_set_report(state, prune_boundary_height)?;
        after.blocks_pruned_total = pruned;
        Ok(after)
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
    use super::{
        Storage, ACCEPTED_STORAGE_MEMORY_MISMATCH_TOTAL, BLOCK_COMMIT_BATCH_FAILED_TOTAL,
        CHAIN_STATE_KEY, SNAPSHOT_CAPTURED_AT_UNIX_KEY, SNAPSHOT_VERIFICATION_STABLE_FAILURE_TOTAL,
        STARTUP_STORAGE_RECONCILIATION_TOTAL, STORAGE_SCHEMA_VERSION, STORAGE_SCHEMA_VERSION_KEY,
    };
    use std::{
        collections::{BTreeSet, HashMap, HashSet},
        sync::atomic::Ordering,
    };

    use proptest::prelude::*;
    use pulsedag_core::{
        accept::{accept_block, AcceptSource},
        build_candidate_block, build_coinbase_transaction, compact_snapshot_to_retained_blocks,
        dev_mine_header,
        errors::PulseError,
        genesis::init_chain_state,
        refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
        state::{ContractRuntimeState, Mempool, UtxoState},
        types::{Block, Hash},
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

    fn write_schema_version(storage: &Storage, value: &[u8]) {
        let meta_cf = storage.db.cf_handle("meta").expect("meta cf");
        storage
            .db
            .put_cf(&meta_cf, STORAGE_SCHEMA_VERSION_KEY, value)
            .expect("write schema metadata");
    }

    #[test]
    fn storage_schema_metadata_missing_is_initialized_on_open() {
        let path = temp_db_path("schema-missing");
        let storage = Storage::open(&path).expect("open storage with missing schema metadata");
        let metadata = storage
            .storage_schema_metadata()
            .expect("read schema metadata");

        assert_eq!(metadata.schema_version, STORAGE_SCHEMA_VERSION);
        assert!(metadata.compatible);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn storage_schema_metadata_valid_is_accepted() {
        let path = temp_db_path("schema-valid");
        {
            let storage = Storage::open(&path).expect("open storage");
            write_schema_version(&storage, STORAGE_SCHEMA_VERSION.to_string().as_bytes());
        }

        let reopened = Storage::open(&path).expect("valid schema metadata must open");
        let metadata = reopened
            .storage_schema_metadata()
            .expect("read schema metadata");
        assert_eq!(metadata.schema_version, STORAGE_SCHEMA_VERSION);
        assert!(metadata.compatible);

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn storage_schema_metadata_future_version_is_rejected() {
        let path = temp_db_path("schema-future");
        {
            let storage = Storage::open(&path).expect("open storage");
            write_schema_version(
                &storage,
                (STORAGE_SCHEMA_VERSION + 1).to_string().as_bytes(),
            );
        }

        let err = Storage::open(&path)
            .err()
            .expect("future schema must be rejected");
        let message = err.to_string();
        assert!(
            message.contains("unsupported future storage schema version"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("newer PulseDAG binary") || message.contains("compatible snapshot"),
            "error should guide operators: {message}"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn storage_schema_metadata_corrupt_version_is_rejected() {
        let path = temp_db_path("schema-corrupt");
        {
            let storage = Storage::open(&path).expect("open storage");
            write_schema_version(&storage, b"not-a-version");
        }

        let err = Storage::open(&path)
            .err()
            .expect("corrupt schema must be rejected");
        let message = err.to_string();
        assert!(
            message.contains("schema version metadata is corrupt"),
            "unexpected error: {message}"
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn list_blocks_orders_equal_height_by_timestamp_then_hash() {
        let path = temp_db_path("list-equal-height-order");
        let storage = Storage::open(&path).expect("open storage");
        let state = init_chain_state("testnet".to_string());
        let parent = best_tip_hash(&state);
        let mut later_low_hash = build_candidate_block(
            vec![parent.clone()],
            1,
            1,
            vec![build_coinbase_transaction("miner-later-low", 50, 101)],
        );
        later_low_hash.header.timestamp = 50;
        later_low_hash.hash = "000-later-low".to_string();
        let mut earlier_high_hash = build_candidate_block(
            vec![parent.clone()],
            1,
            1,
            vec![build_coinbase_transaction("miner-earlier", 50, 102)],
        );
        earlier_high_hash.header.timestamp = 40;
        earlier_high_hash.hash = "999-earlier-high".to_string();
        let mut same_time_low_hash = build_candidate_block(
            vec![parent],
            1,
            1,
            vec![build_coinbase_transaction("miner-same-low", 50, 103)],
        );
        same_time_low_hash.header.timestamp = 50;
        same_time_low_hash.hash = "001-same-time-low".to_string();

        storage
            .persist_block(&later_low_hash)
            .expect("persist later low hash first");
        storage
            .persist_block(&same_time_low_hash)
            .expect("persist same timestamp second");
        storage
            .persist_block(&earlier_high_hash)
            .expect("persist earlier high hash last");

        let ordered = storage.list_blocks().expect("list blocks");
        let ordered_hashes = ordered.iter().map(|b| b.hash.as_str()).collect::<Vec<_>>();
        assert_eq!(
            ordered_hashes,
            vec!["999-earlier-high", "000-later-low", "001-same-time-low"]
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[derive(serde::Serialize)]
    struct LegacyDagState {
        blocks: HashMap<Hash, Block>,
        tips: HashSet<Hash>,
        children: HashMap<Hash, Vec<Hash>>,
        genesis_hash: Hash,
        best_height: u64,
    }

    #[derive(serde::Serialize)]
    struct LegacyChainState {
        chain_id: String,
        dag: LegacyDagState,
        utxo: UtxoState,
        mempool: Mempool,
        contracts: ContractRuntimeState,
    }

    fn legacy_chain_state_bytes(state: &pulsedag_core::ChainState) -> Vec<u8> {
        let legacy = LegacyChainState {
            chain_id: state.chain_id.clone(),
            dag: LegacyDagState {
                blocks: state.dag.blocks.clone(),
                tips: state.dag.tips.clone(),
                children: state.dag.children.clone(),
                genesis_hash: state.dag.genesis_hash.clone(),
                best_height: state.dag.best_height,
            },
            utxo: state.utxo.clone(),
            mempool: state.mempool.clone(),
            contracts: state.contracts.clone(),
        };
        bincode::serialize(&legacy).expect("serialize legacy state")
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
            refresh_block_consensus_ids_with_state(&mut block, &state)
                .expect("prepare state root for test block");
            let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
            assert!(mined, "failed to mine test block at height {}", i);
            block.header = header;
            refresh_block_consensus_ids(&mut block);
            accept_block(block, &mut state, AcceptSource::LocalMining).expect("accept mined block");
        }
        state
    }

    fn persist_all_blocks(storage: &Storage, state: &pulsedag_core::ChainState) {
        let mut blocks: Vec<_> = state.dag.blocks.values().cloned().collect();
        blocks.sort_by_key(|block| block.header.height);
        for block in blocks {
            storage.persist_block(&block).expect("persist block");
        }
    }

    fn append_test_block(
        state: &mut pulsedag_core::ChainState,
        parents: Vec<Hash>,
        height: u64,
    ) -> Block {
        let coinbase_nonce = height
            .saturating_mul(1_000)
            .saturating_add(state.dag.blocks.len() as u64);
        let mut block = build_candidate_block(
            parents,
            height,
            1,
            vec![build_coinbase_transaction("miner", 50, coinbase_nonce)],
        );
        refresh_block_consensus_ids_with_state(&mut block, state)
            .expect("prepare state root for test block");
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block at height {height}");
        block.header = header;
        refresh_block_consensus_ids(&mut block);
        accept_block(block.clone(), state, AcceptSource::LocalMining).expect("accept block");
        block
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
        refresh_block_consensus_ids_with_state(&mut block, &state)
            .expect("prepare state root for test block");
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block");
        block.header = header;
        refresh_block_consensus_ids(&mut block);
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
        refresh_block_consensus_ids_with_state(&mut block, &state)
            .expect("prepare state root for test block");
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block");
        block.header = header;
        refresh_block_consensus_ids(&mut block);
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
        refresh_block_consensus_ids_with_state(&mut block, &state)
            .expect("prepare state root for test block");
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
        assert!(mined, "failed to mine test block");
        block.header = header;
        refresh_block_consensus_ids(&mut block);
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
            refresh_block_consensus_ids_with_state(&mut block, &state)
                .expect("prepare state root for test block");
            let (header, mined, _, _) = dev_mine_header(block.header.clone(), 25_000);
            assert!(mined, "failed to mine test block at height {}", height);
            block.header = header;
            refresh_block_consensus_ids(&mut block);
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
    fn block_commit_invariant_snapshot_identifies_extra_persisted_hash_and_source() {
        let path = temp_db_path("block-commit-invariant-extra");
        let storage = Storage::open(&path).expect("open storage");
        let state = init_chain_state("testnet".to_string());
        let genesis = state.dag.blocks.values().next().cloned().expect("genesis");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");
        let extra = build_candidate_block(
            vec!["missing-parent".into()],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 98)],
        );
        storage
            .persist_block(&extra)
            .expect("simulate legacy rejected/staged block leak into accepted storage");

        let report = storage
            .verify_accepted_storage_invariants(&state)
            .expect("coherent invariant snapshot");

        assert!(!report.is_ok());
        assert_eq!(report.storage_only_hashes, vec![extra.hash.clone()]);
        assert_eq!(
            report.mismatch_acceptance_sources.get(&extra.hash),
            Some(&"persisted_accepted_unreferenced".to_string())
        );
        assert_eq!(report.memory_only_hashes, Vec::<Hash>::new());
        assert_eq!(
            report.in_memory_accepted_hashes.len(),
            state.dag.blocks.len()
        );
        assert_eq!(
            report.persisted_accepted_hashes.len(),
            state.dag.blocks.len() + 1
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn block_commit_missing_parent_stays_out_of_accepted_storage() {
        let path = temp_db_path("block-commit-missing-parent");
        let storage = Storage::open(&path).expect("open storage");
        let state = init_chain_state("testnet".to_string());
        let genesis = state.dag.blocks.values().next().cloned().expect("genesis");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");
        let missing_parent = build_candidate_block(
            vec!["missing-parent".into()],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 99)],
        );

        storage
            .persist_staged_orphan_block(&missing_parent)
            .expect("stage orphan separately");

        assert_eq!(storage.block_count().expect("accepted count"), 1);
        assert_eq!(
            storage
                .verify_accepted_storage_invariants(&state)
                .expect("invariants")
                .accepted_storage_count,
            state.dag.blocks.len()
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn block_commit_failure_leaves_memory_and_storage_unchanged() {
        let path = temp_db_path("block-commit-failure");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = init_chain_state("testnet".to_string());
        let genesis = state.dag.blocks.values().next().cloned().expect("genesis");
        storage
            .persist_block_and_chain_state(&genesis, &state)
            .expect("persist genesis");
        let before_state = state.clone();
        let before_count = storage.block_count().expect("before count");
        let before_failures = BLOCK_COMMIT_BATCH_FAILED_TOTAL.load(Ordering::Relaxed);
        let mut block = build_candidate_block(
            vec![best_tip_hash(&state)],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 100)],
        );
        refresh_block_consensus_ids_with_state(&mut block, &state).expect("state root");
        accept_block(block.clone(), &mut state, AcceptSource::LocalMining).expect("accept in temp");
        let prepared = state.clone();
        state = before_state.clone();

        let err = storage
            .persist_block_and_chain_state_with_write(&block, &prepared, |_db, _batch| {
                Err(PulseError::StorageError("injected write failure".into()))
            })
            .expect_err("write should fail");
        assert!(err.to_string().contains("injected write failure"));
        assert_eq!(state.dag.blocks.len(), before_state.dag.blocks.len());
        assert_eq!(storage.block_count().expect("after count"), before_count);
        assert_eq!(
            BLOCK_COMMIT_BATCH_FAILED_TOTAL.load(Ordering::Relaxed),
            before_failures + 1
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn block_commit_startup_reconciliation_quarantines_unreferenced_accepted_record() {
        let path = temp_db_path("block-commit-startup-reconcile");
        {
            let storage = Storage::open(&path).expect("open storage");
            let mut state = init_chain_state("testnet".to_string());
            let genesis = state.dag.blocks.values().next().cloned().expect("genesis");
            storage
                .persist_block_and_chain_state(&genesis, &state)
                .expect("persist genesis");
            let mut block = build_candidate_block(
                vec![best_tip_hash(&state)],
                1,
                1,
                vec![build_coinbase_transaction("miner", 50, 101)],
            );
            refresh_block_consensus_ids_with_state(&mut block, &state).expect("state root");
            accept_block(block.clone(), &mut state, AcceptSource::LocalMining).expect("accept");
            storage
                .persist_block(&block)
                .expect("simulate legacy orphaned accepted record");
            assert_eq!(storage.block_count().expect("legacy count"), 2);
        }

        let before = STARTUP_STORAGE_RECONCILIATION_TOTAL.load(Ordering::Relaxed);
        let reopened = Storage::open(&path).expect("reopen reconciles");
        assert_eq!(reopened.block_count().expect("repaired count"), 1);
        assert_eq!(
            STARTUP_STORAGE_RECONCILIATION_TOTAL.load(Ordering::Relaxed),
            before + 1
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn block_commit_invariants_detect_and_count_mismatch() {
        let path = temp_db_path("block-commit-invariant-mismatch");
        let storage = Storage::open(&path).expect("open storage");
        let state = init_chain_state("testnet".to_string());
        let before = ACCEPTED_STORAGE_MEMORY_MISMATCH_TOTAL.load(Ordering::Relaxed);
        let report = storage
            .verify_accepted_storage_invariants(&state)
            .expect("invariants");
        assert!(!report.is_ok());
        assert_eq!(report.memory_only_hashes.len(), state.dag.blocks.len());
        assert_eq!(
            ACCEPTED_STORAGE_MEMORY_MISMATCH_TOTAL.load(Ordering::Relaxed),
            before + 1
        );

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
    fn load_or_init_genesis_replays_blocks_when_legacy_snapshot_truncates() {
        let path = temp_db_path("legacy-snapshot-fallback");
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
            .put_cf(&meta_cf, CHAIN_STATE_KEY, legacy_chain_state_bytes(&state))
            .expect("write legacy snapshot bytes");

        let rebuilt = storage
            .load_or_init_genesis("testnet".to_string())
            .expect("legacy snapshot must fall back to block replay");
        assert_eq!(rebuilt.dag.best_height, state.dag.best_height);
        assert_eq!(best_tip_hash(&rebuilt), best_tip_hash(&state));
        assert!(rebuilt
            .dag
            .selected_chain
            .iter()
            .any(|hash| hash == &rebuilt.dag.genesis_hash));
        let events = storage.list_runtime_events(25).expect("runtime events");
        assert!(
            events
                .iter()
                .any(|e| e.kind == "startup_snapshot_decode_failed_fallback_full"),
            "expected startup fallback runtime event"
        );

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
        refresh_block_consensus_ids_with_state(&mut invalid_block, &state)
            .expect("prepare state root for invalid delta block");
        let (header, mined, _, _) = dev_mine_header(invalid_block.header.clone(), 25_000);
        assert!(mined, "failed to mine invalid test block");
        invalid_block.header = header;
        refresh_block_consensus_ids(&mut invalid_block);
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
    fn export_import_workflow_is_repeatable_across_multiple_targets() {
        let source_path = temp_db_path("snapshot-repeatable-source");
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

        let (bundle, report) = source
            .export_snapshot_bundle(Some("testnet"))
            .expect("export snapshot bundle");
        assert!(report.restore_guarantees_explicit);

        for run in 0..2 {
            let target_path = temp_db_path(&format!("snapshot-repeatable-target-{run}"));
            let target = Storage::open(&target_path).expect("open target storage");
            let import_report = target
                .import_snapshot_bundle(bundle.clone(), Some("testnet"))
                .expect("import snapshot bundle");
            assert!(import_report.restore_guarantees_explicit);
            assert!(import_report.replay_viable);

            let restored = target
                .replay_from_validated_snapshot_and_delta(Some("testnet"))
                .expect("restored from imported snapshot");
            assert_eq!(restored.dag.best_height, state.dag.best_height);
            assert_eq!(best_tip_hash(&restored), best_tip_hash(&state));
            assert_eq!(
                target
                    .snapshot_captured_at_unix()
                    .expect("imported anchor timestamp"),
                bundle.snapshot_captured_at_unix
            );
            let _ = std::fs::remove_dir_all(target_path);
        }

        let _ = std::fs::remove_dir_all(source_path);
    }

    #[test]
    fn verification_signals_are_explicit_for_chain_mismatch() {
        let path = temp_db_path("snapshot-verify-chain-mismatch");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let bundle = Storage::snapshot_bundle_for_state(
            state.clone(),
            state.dag.blocks.values().cloned().collect(),
            1,
            Some(1),
            Storage::snapshot_metadata_for_state(&state, 1),
            0,
        );

        let report = storage.verify_snapshot_bundle(&bundle, Some("stagingnet"));
        assert!(!report.restore_guarantees_explicit);
        assert!(!report.chain_id_matches_expected);
        assert_eq!(report.recovery_confidence, "medium");
        assert_eq!(report.issue_count, report.issues.len());
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "SNAPSHOT_BUNDLE_CHAIN_ID_MISMATCH"));
        let _ = std::fs::remove_dir_all(path);
    }
    #[test]
    fn verify_snapshot_bundle_signals_missing_anchor_explicitly() {
        let path = temp_db_path("snapshot-verify-anchor-missing");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let bundle = Storage::snapshot_bundle_for_state(
            state.clone(),
            state.dag.blocks.values().cloned().collect(),
            1,
            None,
            Storage::snapshot_metadata_for_state(&state, 1),
            0,
        );

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
    fn snapshot_verification_generation_changed_discards_lineage_failure_for_retry() {
        let path = temp_db_path("snapshot-generation-changed-retry");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let mut persisted = state.dag.blocks.values().cloned().collect::<Vec<_>>();
        let mut newer = persisted
            .iter()
            .find(|block| block.header.height == 2)
            .cloned()
            .expect("height two block");
        newer.hash = "newer-generation-side-dag".to_string();
        persisted.push(newer);
        let mut bundle = Storage::snapshot_bundle_for_state(
            state.clone(),
            persisted,
            1,
            Some(1),
            Storage::snapshot_metadata_for_state(&state, 1),
            storage
                .accepted_storage_generation()
                .expect("storage generation"),
        );
        bundle.delta_start_generation = 1;
        bundle.delta_end_generation = 2;

        let report = storage.verify_snapshot_bundle(&bundle, Some("testnet"));
        assert!(report.verification_generation_changed);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "SNAPSHOT_BUNDLE_VERIFICATION_GENERATION_CHANGED"));
        assert!(!report
            .issues
            .iter()
            .any(|issue| issue.code == "SNAPSHOT_BUNDLE_DELTA_NOT_IN_SNAPSHOT"));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn snapshot_verification_stable_missing_hash_is_real_failure() {
        let path = temp_db_path("snapshot-stable-missing-hash");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 3);
        let mut snapshot = state.clone();
        let missing = snapshot
            .dag
            .blocks
            .iter()
            .find(|(_, block)| block.header.height == 2)
            .map(|(hash, _)| hash.clone())
            .expect("height two hash");
        snapshot.dag.blocks.remove(&missing);
        let bundle = Storage::snapshot_bundle_for_state(
            snapshot,
            state.dag.blocks.values().cloned().collect(),
            1,
            Some(1),
            Storage::snapshot_metadata_for_state(&state, 1),
            storage
                .accepted_storage_generation()
                .expect("storage generation"),
        );

        let before = SNAPSHOT_VERIFICATION_STABLE_FAILURE_TOTAL.load(Ordering::Relaxed);
        let report = storage.verify_snapshot_bundle(&bundle, Some("testnet"));
        assert!(!report.verification_generation_changed);
        assert!(!report.restore_guarantees_explicit);
        assert!(report.issues.iter().any(|issue| issue.code
            == "SNAPSHOT_BUNDLE_DELTA_NOT_IN_SNAPSHOT"
            && issue.message.contains(&missing)));
        assert!(SNAPSHOT_VERIFICATION_STABLE_FAILURE_TOTAL.load(Ordering::Relaxed) > before);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn snapshot_verification_newer_delta_side_dag_is_not_snapshot_corruption() {
        let path = temp_db_path("snapshot-newer-side-dag");
        let storage = Storage::open(&path).expect("open storage");
        let snapshot = build_linear_chain("testnet", 2);
        let mut final_state = snapshot.clone();
        let mut side = build_candidate_block(
            vec![best_tip_hash(&snapshot)],
            3,
            1,
            vec![build_coinbase_transaction("side", 50, 30)],
        );
        refresh_block_consensus_ids_with_state(&mut side, &final_state)
            .expect("prepare side block");
        let (header, mined, _, _) = dev_mine_header(side.header.clone(), 25_000);
        assert!(mined, "failed to mine side-dag block");
        side.header = header;
        refresh_block_consensus_ids(&mut side);
        accept_block(side.clone(), &mut final_state, AcceptSource::LocalMining)
            .expect("accept side block");
        let bundle = Storage::snapshot_bundle_for_state(
            snapshot.clone(),
            vec![side],
            1,
            Some(1),
            Storage::snapshot_metadata_for_state(&snapshot, 1),
            storage
                .accepted_storage_generation()
                .expect("storage generation"),
        );

        let report = storage.verify_snapshot_bundle(&bundle, Some("testnet"));
        assert!(!report
            .issues
            .iter()
            .any(|issue| issue.code == "SNAPSHOT_BUNDLE_DELTA_NOT_IN_SNAPSHOT"));
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
        assert!(pre_drill.recovery_confidence_non_misleading);
        assert_eq!(pre_drill.confidence_evidence_path, "lineage+deep_replay");

        storage
            .restore_drill_snapshot_and_delta("testnet".to_string())
            .expect("restore drill should succeed");

        let post_drill = storage
            .audit_state_integrity(Some("testnet"), true)
            .expect("audit after drill");
        assert_eq!(post_drill.recovery_confidence, "high");
        assert!(post_drill.restore_drill_confirms_recovery);
        assert!(post_drill.recovery_confidence_non_misleading);
        assert_eq!(
            post_drill.confidence_evidence_path,
            "lineage+deep_replay+restore_drill"
        );
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
        assert!(report.recovery_confidence_non_misleading);
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
        assert_eq!(report.lineage_issue_count, 0);
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
        assert!(report.recovery_confidence_non_misleading);
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
        assert_eq!(report.confidence_evidence_path, "snapshot+anchor");
        assert!(
            report
                .confidence_reason
                .contains("snapshot and anchor metadata"),
            "reason should describe missing anchor constraint"
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn compact_prune_batch_failure_leaves_snapshot_and_blocks_unchanged() {
        let path = temp_db_path("compact-prune-atomic-failure");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 12);
        persist_all_blocks(&storage, &state);
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");
        let retained_blocks = storage
            .list_blocks()
            .expect("list blocks")
            .into_iter()
            .filter(|block| block.header.height >= 7)
            .collect::<Vec<_>>();
        let compact = compact_snapshot_to_retained_blocks(state.clone(), &retained_blocks)
            .expect("compact snapshot");
        let retained_hashes = retained_blocks
            .iter()
            .map(|block| block.hash.clone())
            .collect::<BTreeSet<_>>();
        let generation = storage.accepted_storage_generation().expect("generation");
        let before_blocks = storage.list_blocks().expect("before blocks");
        let before_snapshot = storage
            .load_chain_state()
            .expect("load snapshot")
            .expect("snapshot");

        let err = storage
            .commit_compact_prune_with_write(
                &compact,
                &retained_hashes,
                generation,
                |_db, _batch| Err(PulseError::StorageError("injected write failure".into())),
            )
            .expect_err("injected batch failure");
        assert!(err.to_string().contains("injected write failure"));
        assert_eq!(
            storage
                .list_blocks()
                .expect("after blocks")
                .into_iter()
                .map(|block| block.hash)
                .collect::<Vec<_>>(),
            before_blocks
                .into_iter()
                .map(|block| block.hash)
                .collect::<Vec<_>>()
        );
        let after_snapshot = storage
            .load_chain_state()
            .expect("load after snapshot")
            .expect("after snapshot");
        assert_eq!(
            after_snapshot.dag.blocks.len(),
            before_snapshot.dag.blocks.len()
        );
        assert_eq!(
            after_snapshot.dag.best_height,
            before_snapshot.dag.best_height
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn compact_prune_97_to_24_restarts_with_identical_tip_and_state_root() {
        let path = temp_db_path("compact-prune-97-to-24");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 96);
        persist_all_blocks(&storage, &state);
        storage
            .persist_chain_state(&state)
            .expect("persist baseline snapshot");
        assert_eq!(storage.block_count().expect("baseline count"), 97);
        let retained_blocks = storage
            .list_blocks()
            .expect("list blocks")
            .into_iter()
            .filter(|block| block.header.height >= 73)
            .collect::<Vec<_>>();
        assert_eq!(retained_blocks.len(), 24);
        let compact = compact_snapshot_to_retained_blocks(state.clone(), &retained_blocks)
            .expect("compact snapshot");
        let retained_hashes = retained_blocks
            .iter()
            .map(|block| block.hash.clone())
            .collect::<BTreeSet<_>>();
        let generation = storage.accepted_storage_generation().expect("generation");
        let removed = storage
            .commit_compact_prune(&compact, &retained_hashes, generation)
            .expect("atomic compact prune");
        assert_eq!(removed, 73);
        assert_eq!(storage.block_count().expect("retained count"), 24);
        let metadata = storage
            .snapshot_metadata()
            .expect("snapshot metadata")
            .expect("metadata exists");
        assert_eq!(metadata.prune_boundary_height, Some(73));
        assert_eq!(
            metadata.original_genesis_hash.as_deref(),
            Some(state.dag.genesis_hash.as_str())
        );
        assert!(!metadata.omitted_parent_hashes.is_empty());
        let invariants = storage
            .verify_accepted_storage_invariants(&compact)
            .expect("retained invariants");
        assert!(invariants.is_ok());
        let expected_tip = best_tip_hash(&state);
        let expected_state_root = state.utxo.compute_state_root().expect("state root");
        drop(storage);

        let restarted = Storage::open(&path).expect("restart storage");
        let restored = restarted
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("restore compact snapshot");
        assert_eq!(restored.dag.blocks.len(), 24);
        assert_eq!(best_tip_hash(&restored), expected_tip);
        assert_eq!(
            restored
                .utxo
                .compute_state_root()
                .expect("restored state root"),
            expected_state_root
        );
        assert!(pulsedag_core::dag_consistency_issues(&restored).is_empty());
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
    fn non_zero_selected_chain_pruning_reports_retained_set_metrics() {
        let path = temp_db_path("non-zero-retained-prune");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 10);
        persist_all_blocks(&storage, &state);
        storage
            .persist_chain_state(&state)
            .expect("persist selected snapshot state");

        let report = storage
            .prune_blocks_with_retained_set(&state, 8)
            .expect("prune with retained-set model");

        assert!(
            report.blocks_pruned_total > 0,
            "pruning validation must fail closed if all attempted cycles report pruned=0"
        );
        assert_eq!(report.prune_boundary_height, 8);
        assert_eq!(report.selected_blocks_retained, 3);
        assert_eq!(report.finality_window_blocks_retained, 3);
        assert_eq!(
            report.retained_storage_hash_digest,
            report.retained_memory_hash_digest
        );
        assert!(report.storage_only_retained_hashes.is_empty());
        assert!(report.memory_only_retained_hashes.is_empty());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn side_dag_and_parent_closure_are_retained_inside_finality_window() {
        let path = temp_db_path("side-dag-retained");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = build_linear_chain("testnet", 6);
        let parent_at_height_four = state
            .dag
            .blocks
            .values()
            .find(|block| block.header.height == 4)
            .expect("height four parent")
            .hash
            .clone();
        let mut side = build_candidate_block(
            vec![parent_at_height_four.clone()],
            5,
            1,
            vec![build_coinbase_transaction("side-miner", 50, 5_999)],
        );
        let (header, mined, _, _) = dev_mine_header(side.header.clone(), 25_000);
        assert!(mined, "failed to mine side DAG block");
        side.header = header;
        refresh_block_consensus_ids(&mut side);
        state
            .dag
            .children
            .entry(parent_at_height_four.clone())
            .or_default()
            .push(side.hash.clone());
        state.dag.blocks.insert(side.hash.clone(), side.clone());
        state.dag.tips.insert(side.hash.clone());
        persist_all_blocks(&storage, &state);
        storage
            .persist_chain_state(&state)
            .expect("persist selected snapshot state");

        let report = storage
            .prune_blocks_with_retained_set(&state, 5)
            .expect("prune with side DAG retained");

        assert!(report.blocks_pruned_total > 0);
        assert!(report.side_dag_blocks_retained >= 1);
        assert!(report.parent_closure_blocks_retained >= 1);
        assert!(storage.get_block(&side.hash).expect("read side").is_some());
        assert!(storage
            .get_block(&parent_at_height_four)
            .expect("read parent closure")
            .is_some());
        assert_eq!(
            report.retained_storage_hash_digest,
            report.retained_memory_hash_digest
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn acceptance_racing_prune_keeps_concurrent_child_parent_visible() {
        let path = temp_db_path("accept-racing-prune");
        let storage = Storage::open(&path).expect("open storage");
        let mut state = build_linear_chain("testnet", 7);
        let race_parent_parent = best_tip_hash(&state);
        let race_parent = append_test_block(&mut state, vec![race_parent_parent], 8);
        let child = append_test_block(&mut state, vec![race_parent.hash.clone()], 9);
        persist_all_blocks(&storage, &state);
        storage
            .persist_chain_state(&state)
            .expect("persist selected snapshot state");

        let report = storage
            .prune_blocks_with_retained_set(&state, 8)
            .expect("prune around accepted block");

        assert!(report.blocks_pruned_total > 0);
        assert!(storage
            .get_block(&race_parent.hash)
            .expect("read race parent")
            .is_some());
        assert!(storage
            .get_block(&child.hash)
            .expect("read child")
            .is_some());
        assert!(report.memory_only_retained_hashes.is_empty());
        assert_eq!(
            report.retained_storage_hash_digest,
            report.retained_memory_hash_digest
        );

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn restart_from_snapshot_delta_after_non_zero_prune_matches_tips_and_state_root() {
        let path = temp_db_path("restart-after-non-zero-prune");
        let storage = Storage::open(&path).expect("open storage");
        let state = build_linear_chain("testnet", 9);
        persist_all_blocks(&storage, &state);
        storage
            .persist_chain_state(&state)
            .expect("persist selected snapshot state");

        let report = storage
            .prune_blocks_with_retained_set(&state, 7)
            .expect("non-zero prune");
        assert!(report.blocks_pruned_total > 0);
        drop(storage);

        let restarted = Storage::open(&path).expect("restart storage");
        let restored = restarted
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("restart from snapshot+delta");
        let restored_tip = best_tip_hash(&restored);
        assert_eq!(restored_tip, best_tip_hash(&state));
        assert_eq!(restored.dag.ordered_dag_tip, state.dag.ordered_dag_tip);
        assert_eq!(
            restored
                .dag
                .blocks
                .get(&restored_tip)
                .map(|block| block.header.state_root.clone()),
            state
                .dag
                .blocks
                .get(&best_tip_hash(&state))
                .map(|block| block.header.state_root.clone())
        );
        let restart_report = restarted
            .retained_set_report(&state, 7)
            .expect("retained report after restart");
        assert!(restart_report.memory_only_retained_hashes.is_empty());
        assert!(restart_report.storage_only_retained_hashes.is_empty());

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn offline_catch_up_rejoin_converges_after_retained_segment_recovery() {
        let path = temp_db_path("offline-rejoin-retained-segment");
        let storage = Storage::open(&path).expect("open storage");
        let offline_state = build_linear_chain("testnet", 6);
        persist_all_blocks(&storage, &offline_state);
        storage
            .persist_chain_state(&offline_state)
            .expect("persist offline snapshot state");
        let report = storage
            .prune_blocks_with_retained_set(&offline_state, 5)
            .expect("non-zero prune before offline window");
        assert!(report.blocks_pruned_total > 0);
        drop(storage);

        let mut network_state = offline_state.clone();
        let parent = best_tip_hash(&network_state);
        append_test_block(&mut network_state, vec![parent], 7);
        let parent = best_tip_hash(&network_state);
        append_test_block(&mut network_state, vec![parent], 8);

        let rejoined = Storage::open(&path).expect("restart offline node");
        for block in network_state
            .dag
            .blocks
            .values()
            .filter(|block| block.header.height > offline_state.dag.best_height)
        {
            rejoined
                .persist_block(block)
                .expect("selected-segment recovery persists catch-up block");
        }
        rejoined
            .persist_chain_state(&network_state)
            .expect("selected-tip inventory converged snapshot");
        let caught_up = rejoined
            .replay_from_validated_snapshot_and_delta(Some("testnet"))
            .expect("replay after rejoin");

        assert_eq!(best_tip_hash(&caught_up), best_tip_hash(&network_state));
        assert_eq!(
            caught_up.dag.ordered_dag_tip,
            network_state.dag.ordered_dag_tip
        );

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
