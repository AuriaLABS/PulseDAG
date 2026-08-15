use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    protocol::ProtocolActivationIdentity,
    replay::{merge_set_digest, ordered_dag_digest, selection_digest},
    state::{ChainState, ConsensusMode, SelectedParentPolicy},
    types::Hash,
};

pub const CONSENSUS_METADATA_SCHEMA_VERSION: u32 = 1;
const CONSENSUS_METADATA_DIGEST_DOMAIN: &[u8] = b"PulseDAG:consensus-metadata:v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlueScoreSemantics {
    /// Historical block-header v1 semantics. Existing header values are never
    /// reinterpreted when a newer binary loads legacy state.
    LegacyHeaderV1,
    /// Current diagnostic-only GHOSTDAG development semantics.
    GhostdagDevV1,
    /// Reserved release semantics. This value is representable in persisted
    /// metadata but is not selectable by the current runtime consensus mode.
    GhostdagV1,
}

impl From<ConsensusMode> for BlueScoreSemantics {
    fn from(mode: ConsensusMode) -> Self {
        match mode {
            ConsensusMode::Legacy => Self::LegacyHeaderV1,
            ConsensusMode::GhostdagDev => Self::GhostdagDevV1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockConsensusMetadataV1 {
    pub selected_parent: Option<Hash>,
    pub blue_score: u64,
    /// Decimal string by contract. This keeps the persisted representation
    /// deterministic over the complete u128 range and avoids JSON number-width
    /// ambiguity.
    pub blue_work_decimal: String,
    pub merge_set_blues: Vec<Hash>,
    pub merge_set_reds: Vec<Hash>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsensusMetadataSnapshotV1 {
    pub schema_version: u32,
    pub protocol_identity: ProtocolActivationIdentity,
    pub blue_score_semantics: BlueScoreSemantics,
    pub selected_parent_policy: SelectedParentPolicy,
    pub selected_tip: Option<Hash>,
    pub selected_chain: Vec<Hash>,
    pub merge_set_k: u64,
    /// BTreeMap is intentional: canonical serialization must not depend on
    /// HashMap iteration order.
    pub blocks: BTreeMap<Hash, BlockConsensusMetadataV1>,
    pub ordering_version: String,
    pub ordered_dag: Vec<Hash>,
    pub selection_digest: String,
    pub merge_set_digest: String,
    pub ordered_dag_digest: String,
}

impl ConsensusMetadataSnapshotV1 {
    pub fn from_state(state: &ChainState) -> Self {
        let mut blocks = BTreeMap::new();
        for (hash, block) in &state.dag.blocks {
            let mut blues = state
                .dag
                .merge_set_blues
                .get(hash)
                .cloned()
                .unwrap_or_default();
            let mut reds = state
                .dag
                .merge_set_reds
                .get(hash)
                .cloned()
                .unwrap_or_default();
            blues.sort();
            reds.sort();

            let selected_parent = state.dag.selected_parents.get(hash).cloned().flatten();
            let blue_work = state
                .dag
                .blue_work
                .get(hash)
                .copied()
                .unwrap_or(block.header.blue_score as u128);

            blocks.insert(
                hash.clone(),
                BlockConsensusMetadataV1 {
                    selected_parent,
                    blue_score: block.header.blue_score,
                    blue_work_decimal: blue_work.to_string(),
                    merge_set_blues: blues,
                    merge_set_reds: reds,
                },
            );
        }

        Self {
            schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            protocol_identity: ProtocolActivationIdentity::legacy_from_state(state),
            blue_score_semantics: state.dag.consensus_mode.into(),
            selected_parent_policy: state.dag.selected_parent_policy,
            selected_tip: state.dag.selected_chain.last().cloned(),
            selected_chain: state.dag.selected_chain.clone(),
            merge_set_k: u64::try_from(state.dag.merge_set_k).unwrap_or(u64::MAX),
            blocks,
            ordering_version: state.dag.ordering_version.clone(),
            ordered_dag: state.dag.ordered_dag.clone(),
            selection_digest: selection_digest(state),
            merge_set_digest: merge_set_digest(state),
            ordered_dag_digest: ordered_dag_digest(state),
        }
    }

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.schema_version != CONSENSUS_METADATA_SCHEMA_VERSION {
            return Err(format!(
                "unsupported consensus metadata schema version {}; expected {}",
                self.schema_version, CONSENSUS_METADATA_SCHEMA_VERSION
            ));
        }
        self.protocol_identity.validate()?;
        if self.merge_set_k == 0 {
            return Err("consensus metadata merge_set_k must be non-zero".into());
        }
        if self.ordering_version.is_empty() {
            return Err("consensus metadata ordering_version must not be empty".into());
        }
        if self.selected_tip != self.selected_chain.last().cloned() {
            return Err("selected_tip does not match selected_chain tail".into());
        }

        for (hash, metadata) in &self.blocks {
            metadata.blue_work_decimal.parse::<u128>().map_err(|_| {
                format!("block {hash} has invalid decimal blue_work representation")
            })?;

            let blues = metadata
                .merge_set_blues
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if blues.len() != metadata.merge_set_blues.len() {
                return Err(format!("block {hash} has duplicate blue merge-set entries"));
            }
            let reds = metadata
                .merge_set_reds
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if reds.len() != metadata.merge_set_reds.len() {
                return Err(format!("block {hash} has duplicate red merge-set entries"));
            }
            if blues.intersection(&reds).next().is_some() {
                return Err(format!(
                    "block {hash} classifies the same merge-set member as blue and red"
                ));
            }
        }
        Ok(())
    }

    /// Fail-closed comparison primitive for storage/snapshot restore. Callers
    /// must treat any mismatch as a protocol-identity/metadata incompatibility,
    /// not as recoverable snapshot corruption eligible for legacy fallback.
    pub fn validate_against_state(&self, state: &ChainState) -> Result<(), String> {
        self.validate_shape()?;
        let expected = Self::from_state(state);
        if self != &expected {
            return Err(
                "persisted consensus metadata does not match reconstructed chain state".into(),
            );
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn digest(&self) -> Result<String, serde_json::Error> {
        let mut hasher = Sha256::new();
        hasher.update(CONSENSUS_METADATA_DIGEST_DOMAIN);
        hasher.update(self.canonical_bytes()?);
        Ok(hex::encode(hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::init_chain_state;

    #[test]
    fn legacy_metadata_snapshot_round_trips_deterministically() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let snapshot = ConsensusMetadataSnapshotV1::from_state(&state);
        snapshot.validate_shape().unwrap();

        let bytes = snapshot.canonical_bytes().unwrap();
        let decoded: ConsensusMetadataSnapshotV1 = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(snapshot, decoded);
        assert_eq!(snapshot.digest().unwrap(), decoded.digest().unwrap());
        assert_eq!(
            snapshot.blue_score_semantics,
            BlueScoreSemantics::LegacyHeaderV1
        );
        assert_eq!(
            snapshot.selected_tip,
            snapshot.selected_chain.last().cloned()
        );
    }

    #[test]
    fn ghostdag_dev_blue_score_semantics_are_explicitly_distinct() {
        let mut state = init_chain_state("pulsedag-private".to_string());
        state.dag.consensus_mode = ConsensusMode::GhostdagDev;

        let snapshot = ConsensusMetadataSnapshotV1::from_state(&state);

        assert_eq!(
            snapshot.blue_score_semantics,
            BlueScoreSemantics::GhostdagDevV1
        );
        assert_ne!(
            snapshot.blue_score_semantics,
            BlueScoreSemantics::LegacyHeaderV1
        );
        assert!(snapshot.validate_shape().is_ok());
    }

    #[test]
    fn blue_work_uses_full_width_decimal_representation() {
        let mut state = init_chain_state("pulsedag-testnet".to_string());
        let genesis = state.dag.genesis_hash.clone();
        state.dag.blue_work.insert(genesis.clone(), u128::MAX);

        let snapshot = ConsensusMetadataSnapshotV1::from_state(&state);
        let metadata = snapshot.blocks.get(&genesis).unwrap();

        assert_eq!(metadata.blue_work_decimal, u128::MAX.to_string());
        assert_eq!(
            metadata.blue_work_decimal.parse::<u128>().unwrap(),
            u128::MAX
        );
    }

    #[test]
    fn malformed_or_mismatched_metadata_fails_closed() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut malformed = ConsensusMetadataSnapshotV1::from_state(&state);
        let genesis = state.dag.genesis_hash.clone();
        malformed
            .blocks
            .get_mut(&genesis)
            .unwrap()
            .blue_work_decimal = "not-a-u128".to_string();
        assert!(malformed.validate_shape().is_err());

        let mut mismatched = ConsensusMetadataSnapshotV1::from_state(&state);
        mismatched.ordering_version.push_str("-mismatch");
        assert!(mismatched.validate_against_state(&state).is_err());
    }

    #[test]
    fn unsupported_metadata_version_fails_closed() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut snapshot = ConsensusMetadataSnapshotV1::from_state(&state);
        snapshot.schema_version = CONSENSUS_METADATA_SCHEMA_VERSION + 1;

        assert!(snapshot.validate_shape().is_err());
    }
}
