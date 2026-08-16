use std::collections::BTreeSet;

use pulsedag_core::{
    types::Hash, BlockConsensusMetadataV1, ProtocolActivationIdentity,
    CONSENSUS_METADATA_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

pub const P2P_DAG_SYNC_CONTRACT_VERSION: u32 = 1;
pub const MAX_SELECTED_CHAIN_LOCATOR_HASHES: usize = 256;
pub const MAX_SELECTED_CHAIN_SUFFIX_HASHES: usize = 4_096;
pub const MAX_DAG_FRONTIER_ENTRIES: usize = 4_096;
pub const MAX_DAG_FRONTIER_REQUIRED_CONTEXT: usize = 4_096;
pub const MAX_DAG_FRONTIER_PARENTS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectedChainLocatorV1 {
    pub contract_version: u32,
    pub protocol_identity: ProtocolActivationIdentity,
    pub selected_tip: Hash,
    /// Ordered from selected tip toward older selected-chain anchors.
    pub locator: Vec<Hash>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DagFrontierEntryV1 {
    pub hash: Hash,
    /// Canonical parent hash list. The sender must sort it lexicographically.
    pub parents: Vec<Hash>,
    pub consensus: BlockConsensusMetadataV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DagFrontierResponseV1 {
    pub contract_version: u32,
    pub protocol_identity: ProtocolActivationIdentity,
    pub consensus_metadata_schema_version: u32,
    pub ordering_version: String,
    pub common_ancestor: Hash,
    pub selected_tip: Hash,
    /// Ordered selected-chain segment from common_ancestor through selected_tip.
    pub selected_chain_suffix: Vec<Hash>,
    /// Hashes that the receiver must already possess before it can validate the
    /// frontier. This list is canonical and lexicographically sorted.
    pub required_context: Vec<Hash>,
    /// Canonical frontier entries sorted lexicographically by block hash.
    pub frontier: Vec<DagFrontierEntryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum DagSyncContractError {
    ContractVersionMismatch {
        observed: u32,
    },
    InvalidProtocolIdentity {
        detail: String,
    },
    EmptySelectedTip,
    LocatorEmpty,
    LocatorTooLarge {
        observed: usize,
        maximum: usize,
    },
    LocatorTipMismatch,
    LocatorDuplicate {
        hash: Hash,
    },
    EmptyHash {
        field: String,
    },
    ConsensusMetadataSchemaMismatch {
        observed: u32,
    },
    OrderingVersionMismatch {
        identity: String,
        response: String,
    },
    SelectedChainSuffixEmpty,
    SelectedChainSuffixTooLarge {
        observed: usize,
        maximum: usize,
    },
    SelectedChainCommonAncestorMismatch,
    SelectedChainTipMismatch,
    SelectedChainDuplicate {
        hash: Hash,
    },
    RequiredContextTooLarge {
        observed: usize,
        maximum: usize,
    },
    RequiredContextNotCanonical,
    FrontierTooLarge {
        observed: usize,
        maximum: usize,
    },
    FrontierNotCanonical,
    FrontierDuplicate {
        hash: Hash,
    },
    TooManyParents {
        hash: Hash,
        observed: usize,
        maximum: usize,
    },
    ParentsNotCanonical {
        hash: Hash,
    },
    InvalidBlueWork {
        hash: Hash,
    },
    MergeSetBluesNotCanonical {
        hash: Hash,
    },
    MergeSetRedsNotCanonical {
        hash: Hash,
    },
    MergeSetColorOverlap {
        hash: Hash,
        member: Hash,
    },
    MissingRequiredParentContext {
        hash: Hash,
        parent: Hash,
    },
}

fn is_strictly_sorted_unique(values: &[Hash]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn first_duplicate(values: &[Hash]) -> Option<Hash> {
    let mut seen = BTreeSet::new();
    values.iter().find_map(|value| {
        if seen.insert(value.clone()) {
            None
        } else {
            Some(value.clone())
        }
    })
}

impl SelectedChainLocatorV1 {
    pub fn validate_shape(&self) -> Result<(), DagSyncContractError> {
        if self.contract_version != P2P_DAG_SYNC_CONTRACT_VERSION {
            return Err(DagSyncContractError::ContractVersionMismatch {
                observed: self.contract_version,
            });
        }
        self.protocol_identity
            .validate()
            .map_err(|detail| DagSyncContractError::InvalidProtocolIdentity { detail })?;
        if self.selected_tip.is_empty() {
            return Err(DagSyncContractError::EmptySelectedTip);
        }
        if self.locator.is_empty() {
            return Err(DagSyncContractError::LocatorEmpty);
        }
        if self.locator.len() > MAX_SELECTED_CHAIN_LOCATOR_HASHES {
            return Err(DagSyncContractError::LocatorTooLarge {
                observed: self.locator.len(),
                maximum: MAX_SELECTED_CHAIN_LOCATOR_HASHES,
            });
        }
        if self.locator.first() != Some(&self.selected_tip) {
            return Err(DagSyncContractError::LocatorTipMismatch);
        }
        if let Some(hash) = self.locator.iter().find(|hash| hash.is_empty()) {
            return Err(DagSyncContractError::EmptyHash {
                field: format!("locator:{hash}"),
            });
        }
        if let Some(hash) = first_duplicate(&self.locator) {
            return Err(DagSyncContractError::LocatorDuplicate { hash });
        }
        Ok(())
    }
}

impl DagFrontierEntryV1 {
    fn validate_shape(&self) -> Result<(), DagSyncContractError> {
        if self.hash.is_empty() {
            return Err(DagSyncContractError::EmptyHash {
                field: "frontier_hash".to_string(),
            });
        }
        if self.parents.len() > MAX_DAG_FRONTIER_PARENTS {
            return Err(DagSyncContractError::TooManyParents {
                hash: self.hash.clone(),
                observed: self.parents.len(),
                maximum: MAX_DAG_FRONTIER_PARENTS,
            });
        }
        if self.parents.iter().any(String::is_empty) {
            return Err(DagSyncContractError::EmptyHash {
                field: format!("parents:{}", self.hash),
            });
        }
        if !is_strictly_sorted_unique(&self.parents) && self.parents.len() > 1 {
            return Err(DagSyncContractError::ParentsNotCanonical {
                hash: self.hash.clone(),
            });
        }
        if self.consensus.blue_work_decimal.parse::<u128>().is_err() {
            return Err(DagSyncContractError::InvalidBlueWork {
                hash: self.hash.clone(),
            });
        }
        if !is_strictly_sorted_unique(&self.consensus.merge_set_blues)
            && self.consensus.merge_set_blues.len() > 1
        {
            return Err(DagSyncContractError::MergeSetBluesNotCanonical {
                hash: self.hash.clone(),
            });
        }
        if !is_strictly_sorted_unique(&self.consensus.merge_set_reds)
            && self.consensus.merge_set_reds.len() > 1
        {
            return Err(DagSyncContractError::MergeSetRedsNotCanonical {
                hash: self.hash.clone(),
            });
        }
        let blues = self
            .consensus
            .merge_set_blues
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if let Some(member) = self
            .consensus
            .merge_set_reds
            .iter()
            .find(|member| blues.contains(*member))
        {
            return Err(DagSyncContractError::MergeSetColorOverlap {
                hash: self.hash.clone(),
                member: member.clone(),
            });
        }
        Ok(())
    }
}

impl DagFrontierResponseV1 {
    pub fn validate_shape(&self) -> Result<(), DagSyncContractError> {
        if self.contract_version != P2P_DAG_SYNC_CONTRACT_VERSION {
            return Err(DagSyncContractError::ContractVersionMismatch {
                observed: self.contract_version,
            });
        }
        self.protocol_identity
            .validate()
            .map_err(|detail| DagSyncContractError::InvalidProtocolIdentity { detail })?;
        if self.consensus_metadata_schema_version != CONSENSUS_METADATA_SCHEMA_VERSION {
            return Err(DagSyncContractError::ConsensusMetadataSchemaMismatch {
                observed: self.consensus_metadata_schema_version,
            });
        }
        if self.ordering_version != self.protocol_identity.dag_ordering_version {
            return Err(DagSyncContractError::OrderingVersionMismatch {
                identity: self.protocol_identity.dag_ordering_version.clone(),
                response: self.ordering_version.clone(),
            });
        }
        if self.common_ancestor.is_empty() {
            return Err(DagSyncContractError::EmptyHash {
                field: "common_ancestor".to_string(),
            });
        }
        if self.selected_tip.is_empty() {
            return Err(DagSyncContractError::EmptySelectedTip);
        }
        if self.selected_chain_suffix.is_empty() {
            return Err(DagSyncContractError::SelectedChainSuffixEmpty);
        }
        if self.selected_chain_suffix.len() > MAX_SELECTED_CHAIN_SUFFIX_HASHES {
            return Err(DagSyncContractError::SelectedChainSuffixTooLarge {
                observed: self.selected_chain_suffix.len(),
                maximum: MAX_SELECTED_CHAIN_SUFFIX_HASHES,
            });
        }
        if self.selected_chain_suffix.first() != Some(&self.common_ancestor) {
            return Err(DagSyncContractError::SelectedChainCommonAncestorMismatch);
        }
        if self.selected_chain_suffix.last() != Some(&self.selected_tip) {
            return Err(DagSyncContractError::SelectedChainTipMismatch);
        }
        if let Some(hash) = first_duplicate(&self.selected_chain_suffix) {
            return Err(DagSyncContractError::SelectedChainDuplicate { hash });
        }
        if self.required_context.len() > MAX_DAG_FRONTIER_REQUIRED_CONTEXT {
            return Err(DagSyncContractError::RequiredContextTooLarge {
                observed: self.required_context.len(),
                maximum: MAX_DAG_FRONTIER_REQUIRED_CONTEXT,
            });
        }
        if (!is_strictly_sorted_unique(&self.required_context) && self.required_context.len() > 1)
            || self.required_context.iter().any(String::is_empty)
        {
            return Err(DagSyncContractError::RequiredContextNotCanonical);
        }
        if self.frontier.len() > MAX_DAG_FRONTIER_ENTRIES {
            return Err(DagSyncContractError::FrontierTooLarge {
                observed: self.frontier.len(),
                maximum: MAX_DAG_FRONTIER_ENTRIES,
            });
        }
        if self
            .frontier
            .windows(2)
            .any(|pair| pair[0].hash >= pair[1].hash)
        {
            return Err(DagSyncContractError::FrontierNotCanonical);
        }
        let mut frontier_hashes = BTreeSet::new();
        for entry in &self.frontier {
            entry.validate_shape()?;
            if !frontier_hashes.insert(entry.hash.clone()) {
                return Err(DagSyncContractError::FrontierDuplicate {
                    hash: entry.hash.clone(),
                });
            }
        }

        let selected = self
            .selected_chain_suffix
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let required = self
            .required_context
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for entry in &self.frontier {
            for parent in &entry.parents {
                if !frontier_hashes.contains(parent)
                    && !selected.contains(parent)
                    && !required.contains(parent)
                {
                    return Err(DagSyncContractError::MissingRequiredParentContext {
                        hash: entry.hash.clone(),
                        parent: parent.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION};

    fn identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet".to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn metadata(parent: Option<&str>, blue_work: &str) -> BlockConsensusMetadataV1 {
        BlockConsensusMetadataV1 {
            selected_parent: parent.map(str::to_string),
            blue_score: 1,
            blue_work_decimal: blue_work.to_string(),
            merge_set_blues: Vec::new(),
            merge_set_reds: Vec::new(),
        }
    }

    fn valid_response() -> DagFrontierResponseV1 {
        DagFrontierResponseV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity(),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            ordering_version: GHOSTDAG_V1_ORDERING_VERSION.to_string(),
            common_ancestor: "ancestor".to_string(),
            selected_tip: "tip".to_string(),
            selected_chain_suffix: vec!["ancestor".to_string(), "tip".to_string()],
            required_context: vec!["side-parent".to_string()],
            frontier: vec![
                DagFrontierEntryV1 {
                    hash: "frontier-a".to_string(),
                    parents: vec!["ancestor".to_string()],
                    consensus: metadata(Some("ancestor"), "100"),
                },
                DagFrontierEntryV1 {
                    hash: "frontier-b".to_string(),
                    parents: vec!["frontier-a".to_string(), "side-parent".to_string()],
                    consensus: metadata(Some("frontier-a"), "200"),
                },
            ],
        }
    }

    #[test]
    fn selected_chain_locator_requires_tip_first_and_unique_hashes() {
        let locator = SelectedChainLocatorV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity(),
            selected_tip: "tip".to_string(),
            locator: vec!["tip".to_string(), "ancestor".to_string()],
        };
        assert_eq!(locator.validate_shape(), Ok(()));

        let mut duplicate = locator.clone();
        duplicate.locator.push("tip".to_string());
        assert!(matches!(
            duplicate.validate_shape(),
            Err(DagSyncContractError::LocatorDuplicate { .. })
        ));
    }

    #[test]
    fn complete_frontier_shape_is_accepted() {
        assert_eq!(valid_response().validate_shape(), Ok(()));
    }

    #[test]
    fn unknown_parent_context_fails_closed() {
        let mut response = valid_response();
        response.frontier[1].parents = vec!["frontier-a".to_string(), "unknown".to_string()];
        assert!(matches!(
            response.validate_shape(),
            Err(DagSyncContractError::MissingRequiredParentContext { .. })
        ));
    }

    #[test]
    fn malformed_blue_work_fails_closed() {
        let mut response = valid_response();
        response.frontier[0].consensus.blue_work_decimal = "not-a-number".to_string();
        assert!(matches!(
            response.validate_shape(),
            Err(DagSyncContractError::InvalidBlueWork { .. })
        ));
    }

    #[test]
    fn ordering_identity_mismatch_fails_closed() {
        let mut response = valid_response();
        response.ordering_version = "different-ordering".to_string();
        assert!(matches!(
            response.validate_shape(),
            Err(DagSyncContractError::OrderingVersionMismatch { .. })
        ));
    }
}
