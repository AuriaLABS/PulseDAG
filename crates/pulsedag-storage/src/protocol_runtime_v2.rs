use std::collections::BTreeSet;

use pulsedag_core::{
    errors::PulseError, verify_authoritative_state_snapshot_v2, ActivatedV2P2pRuntime, Block,
    ChainState, ProtocolActivationIdentity, ProtocolActivationRecordV1,
    ProtocolRestoreIdentityGate, ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS,
    ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS, GHOSTDAG_V1_ORDERING_VERSION,
};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

use super::{protocol_identity::PROTOCOL_ACTIVATION_STORAGE_KEY, Storage, ACCEPTED_BLOCKS_CF};

pub const ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION: u32 = 1;
pub const ACTIVATED_V2_P2P_RUNTIME_STORAGE_KEY: &[u8] = b"activated_v2_p2p_runtime_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivatedV2P2pRuntimeRecordV1 {
    pub format_version: u32,
    pub activation_record: ProtocolActivationRecordV1,
    pub chain_state_generation: u64,
    pub runtime: ActivatedV2P2pRuntime,
}

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn require_canonical_activated_v2_identity(
    expected: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    expected.validate().map_err(storage_error)?;
    let canonical = ProtocolActivationIdentity::activated_v2(
        expected.chain_id.clone(),
        expected.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    if &canonical != expected {
        return Err(storage_error(
            "activated-v2 P2P runtime persistence requires the canonical ghostdag_v1 protocol identity",
        ));
    }
    Ok(())
}

fn verify_activated_v2_state(
    state: &ChainState,
    expected: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    require_canonical_activated_v2_identity(expected)?;
    if state.chain_id != expected.chain_id {
        return Err(storage_error(format!(
            "activated-v2 runtime state chain_id={} does not match expected {}",
            state.chain_id, expected.chain_id
        )));
    }
    if state.dag.genesis_hash != expected.genesis_hash {
        return Err(storage_error(format!(
            "activated-v2 runtime state genesis={} does not match expected {}",
            state.dag.genesis_hash, expected.genesis_hash
        )));
    }
    verify_authoritative_state_snapshot_v2(state).map_err(|error| {
        storage_error(format!(
            "activated-v2 runtime state is not an authoritative v2 snapshot: {error:?}"
        ))
    })?;
    Ok(())
}

fn verify_runtime_against_state(
    runtime: &ActivatedV2P2pRuntime,
    state: &ChainState,
) -> Result<(), PulseError> {
    if runtime.pending_len() > ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS {
        return Err(storage_error(format!(
            "activated-v2 pending runtime count {} exceeds capacity {}",
            runtime.pending_len(),
            ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS
        )));
    }
    if runtime.staging().len() > ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS {
        return Err(storage_error(format!(
            "activated-v2 staging runtime count {} exceeds capacity {}",
            runtime.staging().len(),
            ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS
        )));
    }

    let accepted = state.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
    let staged = runtime
        .staging()
        .hashes()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let pending = runtime
        .pending_hashes()
        .into_iter()
        .collect::<BTreeSet<_>>();

    if let Some(hash) = staged.intersection(&accepted).next() {
        return Err(storage_error(format!(
            "activated-v2 staged runtime hash {hash} is already authoritative"
        )));
    }
    if let Some(hash) = pending.intersection(&accepted).next() {
        return Err(storage_error(format!(
            "activated-v2 pending runtime hash {hash} is already authoritative"
        )));
    }
    if let Some(hash) = staged.intersection(&pending).next() {
        return Err(storage_error(format!(
            "activated-v2 runtime hash {hash} appears in both staging and pending queues"
        )));
    }

    for hash in &staged {
        let block = runtime.staging().get(hash).ok_or_else(|| {
            storage_error(format!(
                "activated-v2 staging hash {hash} disappeared during restore validation"
            ))
        })?;
        if block.hash != *hash {
            return Err(storage_error(format!(
                "activated-v2 staged runtime key {hash} does not match embedded block hash {}",
                block.hash
            )));
        }
    }
    Ok(())
}

impl ActivatedV2P2pRuntimeRecordV1 {
    fn from_runtime(
        expected: &ProtocolActivationIdentity,
        state: &ChainState,
        runtime: &ActivatedV2P2pRuntime,
    ) -> Result<Self, PulseError> {
        verify_activated_v2_state(state, expected)?;
        verify_runtime_against_state(runtime, state)?;
        let activation_record =
            ProtocolActivationRecordV1::from_identity(expected.clone()).map_err(storage_error)?;
        Ok(Self {
            format_version: ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION,
            activation_record,
            chain_state_generation: state.chain_state_generation,
            runtime: runtime.clone(),
        })
    }

    pub fn verify_expected(
        &self,
        expected: &ProtocolActivationIdentity,
        state: &ChainState,
    ) -> Result<(), PulseError> {
        if self.format_version != ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION {
            return Err(storage_error(format!(
                "unsupported activated-v2 P2P runtime record format {}; expected {}",
                self.format_version, ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION
            )));
        }
        require_canonical_activated_v2_identity(expected)?;
        self.activation_record
            .verify_expected(expected)
            .map_err(storage_error)?;
        verify_activated_v2_state(state, expected)?;
        if self.chain_state_generation != state.chain_state_generation {
            return Err(storage_error(format!(
                "activated-v2 runtime generation {} does not match chain snapshot generation {}",
                self.chain_state_generation, state.chain_state_generation
            )));
        }
        verify_runtime_against_state(&self.runtime, state)
    }
}

impl Storage {
    pub fn activated_v2_p2p_runtime_record(
        &self,
    ) -> Result<Option<ActivatedV2P2pRuntimeRecordV1>, PulseError> {
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let Some(bytes) = self
            .db
            .get_cf(&meta_cf, ACTIVATED_V2_P2P_RUNTIME_STORAGE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
        else {
            return Ok(None);
        };
        let record: ActivatedV2P2pRuntimeRecordV1 =
            serde_json::from_slice(&bytes).map_err(|error| storage_error(error.to_string()))?;
        if record.format_version != ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION {
            return Err(storage_error(format!(
                "unsupported activated-v2 P2P runtime record format {}; expected {}",
                record.format_version, ACTIVATED_V2_P2P_RUNTIME_RECORD_FORMAT_VERSION
            )));
        }
        record
            .activation_record
            .validate_internal()
            .map_err(storage_error)?;
        Ok(Some(record))
    }

    fn stage_activated_v2_runtime_sidecars(
        &self,
        batch: &mut WriteBatch,
        meta_cf: &impl rocksdb::AsColumnFamilyRef,
        record: &ActivatedV2P2pRuntimeRecordV1,
    ) -> Result<(), PulseError> {
        batch.put_cf(
            meta_cf,
            PROTOCOL_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&record.activation_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        batch.put_cf(
            meta_cf,
            ACTIVATED_V2_P2P_RUNTIME_STORAGE_KEY,
            serde_json::to_vec(record).map_err(|error| storage_error(error.to_string()))?,
        );
        Ok(())
    }

    /// Persist an authoritative activated-v2 chain snapshot, exact activation
    /// identity and transient P2P runtime in one RocksDB batch. No accepted
    /// block row is added because this path is for staged/pending-only runtime
    /// changes whose authoritative chain generation did not advance.
    pub fn persist_activated_v2_p2p_runtime_snapshot(
        &self,
        expected: &ProtocolActivationIdentity,
        state: &ChainState,
        runtime: &ActivatedV2P2pRuntime,
    ) -> Result<(), PulseError> {
        let record = ActivatedV2P2pRuntimeRecordV1::from_runtime(expected, state, runtime)?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let mut batch = WriteBatch::default();
        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        self.stage_activated_v2_runtime_sidecars(&mut batch, &meta_cf, &record)?;
        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))
    }

    /// Atomically persist one newly accepted activated-v2 block together with
    /// its authoritative state, exact activation identity and post-commit P2P
    /// runtime snapshot.
    pub fn persist_activated_v2_p2p_block_and_runtime(
        &self,
        block: &Block,
        expected: &ProtocolActivationIdentity,
        state: &ChainState,
        runtime: &ActivatedV2P2pRuntime,
    ) -> Result<(), PulseError> {
        self.persist_activated_v2_p2p_blocks_and_runtime(
            std::slice::from_ref(block),
            expected,
            state,
            runtime,
        )
    }

    /// Atomically persist a promoted activated-v2 bundle together with its
    /// authoritative state, exact activation identity and post-commit P2P
    /// runtime snapshot.
    pub fn persist_activated_v2_p2p_blocks_and_runtime(
        &self,
        blocks: &[Block],
        expected: &ProtocolActivationIdentity,
        state: &ChainState,
        runtime: &ActivatedV2P2pRuntime,
    ) -> Result<(), PulseError> {
        let record = ActivatedV2P2pRuntimeRecordV1::from_runtime(expected, state, runtime)?;
        let blocks_cf = self
            .db
            .cf_handle(ACCEPTED_BLOCKS_CF)
            .ok_or_else(|| storage_error("missing cf accepted blocks"))?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let mut seen = BTreeSet::new();
        let mut batch = WriteBatch::default();

        for block in blocks {
            if !seen.insert(block.hash.clone()) {
                return Err(storage_error(format!(
                    "activated-v2 accepted block batch contains duplicate hash {}",
                    block.hash
                )));
            }
            let state_block = state.dag.blocks.get(&block.hash).ok_or_else(|| {
                storage_error(format!(
                    "activated-v2 accepted block {} is absent from committed chain state",
                    block.hash
                ))
            })?;
            if serde_json::to_vec(state_block).map_err(|error| storage_error(error.to_string()))?
                != serde_json::to_vec(block).map_err(|error| storage_error(error.to_string()))?
            {
                return Err(storage_error(format!(
                    "activated-v2 accepted block {} differs from committed chain state",
                    block.hash
                )));
            }
            batch.put_cf(
                &blocks_cf,
                block.hash.as_bytes(),
                serde_json::to_vec(block).map_err(|error| storage_error(error.to_string()))?,
            );
        }

        if !blocks.is_empty() {
            self.stage_accepted_storage_generation_advance(&mut batch, &meta_cf)?;
        }
        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        self.stage_activated_v2_runtime_sidecars(&mut batch, &meta_cf, &record)?;
        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))
    }

    /// Restore only from an exact activated-v2 durable identity. Missing
    /// sidecars, legacy schema compatibility, generation mismatch, corrupt
    /// runtime payloads or non-authoritative chain snapshots all fail closed.
    pub fn load_activated_v2_p2p_runtime_snapshot(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(ChainState, ActivatedV2P2pRuntime), PulseError> {
        require_canonical_activated_v2_identity(expected)?;
        let gate = self.verify_persisted_protocol_identity(expected)?;
        if gate != ProtocolRestoreIdentityGate::VerifiedRecordV1 {
            return Err(storage_error(
                "activated-v2 P2P runtime restore requires an explicit verified activation record",
            ));
        }
        let state = self
            .load_chain_state()?
            .ok_or_else(|| storage_error("activated-v2 chain snapshot missing"))?;
        let record = self
            .activated_v2_p2p_runtime_record()?
            .ok_or_else(|| storage_error("activated-v2 P2P runtime sidecar missing"))?;
        record.verify_expected(expected, &state)?;
        Ok((state, record.runtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, materialize_authoritative_state_v2, ProtocolActivationIdentity,
    };

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-p2p-runtime-v2-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn activated_state() -> (ChainState, ProtocolActivationIdentity) {
        let mut state = init_chain_state("task28-storage-p2p-runtime-v2".to_string());
        let genesis = state.dag.genesis_hash.clone();
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis, vec![]);
        let state = materialize_authoritative_state_v2(&state).unwrap();
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        (state, identity)
    }

    #[test]
    fn activated_v2_runtime_snapshot_round_trips_with_exact_identity() {
        let path = temp_db_path("round-trip");
        let storage = Storage::open(&path).unwrap();
        let (state, identity) = activated_state();
        let runtime = ActivatedV2P2pRuntime::default();

        storage
            .persist_activated_v2_p2p_runtime_snapshot(&identity, &state, &runtime)
            .unwrap();
        let (restored_state, restored_runtime) = storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .unwrap();

        assert_eq!(restored_state.chain_id, state.chain_id);
        assert_eq!(
            restored_state.chain_state_generation,
            state.chain_state_generation
        );
        assert!(restored_runtime.pending_is_empty());
        assert!(restored_runtime.staging().is_empty());
        assert_eq!(
            storage
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            identity
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn activated_v2_restore_rejects_missing_explicit_identity_record() {
        let path = temp_db_path("missing-identity");
        let storage = Storage::open(&path).unwrap();
        let (state, identity) = activated_state();
        storage.persist_chain_state(&state).unwrap();

        assert!(storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn activated_v2_restore_rejects_runtime_generation_mismatch() {
        let path = temp_db_path("generation-mismatch");
        let storage = Storage::open(&path).unwrap();
        let (state, identity) = activated_state();
        let runtime = ActivatedV2P2pRuntime::default();
        storage
            .persist_activated_v2_p2p_runtime_snapshot(&identity, &state, &runtime)
            .unwrap();

        let meta_cf = storage.db.cf_handle("meta").unwrap();
        let mut record = storage.activated_v2_p2p_runtime_record().unwrap().unwrap();
        record.chain_state_generation = record.chain_state_generation.saturating_add(1);
        storage
            .db
            .put_cf(
                &meta_cf,
                ACTIVATED_V2_P2P_RUNTIME_STORAGE_KEY,
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();

        assert!(storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn activated_v2_runtime_rejects_hash_already_in_authoritative_state() {
        let path = temp_db_path("overlap");
        let storage = Storage::open(&path).unwrap();
        let (state, identity) = activated_state();
        let genesis = state.dag.genesis_hash.clone();
        let block = state.dag.blocks.get(&genesis).unwrap().clone();

        let mut value = serde_json::to_value(ActivatedV2P2pRuntime::default()).unwrap();
        value["staging"]["blocks"]
            .as_object_mut()
            .unwrap()
            .insert(genesis, serde_json::to_value(block).unwrap());
        let runtime: ActivatedV2P2pRuntime = serde_json::from_value(value).unwrap();

        assert!(storage
            .persist_activated_v2_p2p_runtime_snapshot(&identity, &state, &runtime)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn legacy_identity_cannot_write_activated_v2_runtime_sidecar() {
        let path = temp_db_path("legacy-rejected");
        let storage = Storage::open(&path).unwrap();
        let (state, _) = activated_state();
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);

        assert!(storage
            .persist_activated_v2_p2p_runtime_snapshot(
                &legacy,
                &state,
                &ActivatedV2P2pRuntime::default(),
            )
            .is_err());
        assert!(storage.activated_v2_p2p_runtime_record().unwrap().is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
