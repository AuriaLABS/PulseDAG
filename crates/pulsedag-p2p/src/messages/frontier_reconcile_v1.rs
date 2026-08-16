use std::collections::BTreeSet;

use pulsedag_core::{types::Hash, ProtocolActivationIdentity};

use super::{DagFrontierResponseV1, DagSyncContractError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagFrontierReconcileError {
    InvalidProtocolIdentity { detail: String },
    ProtocolIdentityMismatch,
    InvalidLocalKnownHash,
    CommonAncestorUnavailable { hash: Hash },
    Contract(DagSyncContractError),
}

/// Deterministic, side-effect-free reconciliation plan for one validated remote
/// DAG frontier response.
///
/// The three missing classes preserve their protocol-defined order. The combined
/// `request_hashes` set is lexicographically sorted and unique so transport
/// batching is independent of hash-map iteration, peer timing, or arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagFrontierReconcilePlanV1 {
    pub common_ancestor: Hash,
    pub selected_tip: Hash,
    pub missing_required_context: Vec<Hash>,
    pub missing_selected_chain: Vec<Hash>,
    pub missing_frontier: Vec<Hash>,
    pub request_hashes: Vec<Hash>,
}

impl DagFrontierReconcilePlanV1 {
    pub fn is_complete(&self) -> bool {
        self.request_hashes.is_empty()
    }

    /// Selected-chain/frontier validation can begin only after every explicitly
    /// required historical/context hash is already present locally.
    pub fn required_context_ready(&self) -> bool {
        self.missing_required_context.is_empty()
    }
}

/// Build the deterministic local fetch/reconciliation plan for a remote frontier.
///
/// The common ancestor must already be locally retained because it is the anchor
/// discovered by the selected-chain locator exchange. If that anchor disappeared
/// or was never available, callers must use the pruning/recovery path instead of
/// treating the response as progress.
pub fn plan_dag_frontier_reconciliation_v1(
    expected_protocol_identity: &ProtocolActivationIdentity,
    response: &DagFrontierResponseV1,
    local_known_hashes: &BTreeSet<Hash>,
) -> Result<DagFrontierReconcilePlanV1, DagFrontierReconcileError> {
    expected_protocol_identity
        .validate()
        .map_err(|detail| DagFrontierReconcileError::InvalidProtocolIdentity { detail })?;
    response
        .validate_shape()
        .map_err(DagFrontierReconcileError::Contract)?;

    if &response.protocol_identity != expected_protocol_identity {
        return Err(DagFrontierReconcileError::ProtocolIdentityMismatch);
    }
    if local_known_hashes.iter().any(String::is_empty) {
        return Err(DagFrontierReconcileError::InvalidLocalKnownHash);
    }
    if !local_known_hashes.contains(&response.common_ancestor) {
        return Err(DagFrontierReconcileError::CommonAncestorUnavailable {
            hash: response.common_ancestor.clone(),
        });
    }

    let missing_required_context = response
        .required_context
        .iter()
        .filter(|hash| !local_known_hashes.contains(*hash))
        .cloned()
        .collect::<Vec<_>>();

    let missing_selected_chain = response
        .selected_chain_suffix
        .iter()
        .skip(1)
        .filter(|hash| !local_known_hashes.contains(*hash))
        .cloned()
        .collect::<Vec<_>>();

    let missing_frontier = response
        .frontier
        .iter()
        .filter(|entry| !local_known_hashes.contains(&entry.hash))
        .map(|entry| entry.hash.clone())
        .collect::<Vec<_>>();

    let request_hashes = missing_required_context
        .iter()
        .chain(&missing_selected_chain)
        .chain(&missing_frontier)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    Ok(DagFrontierReconcilePlanV1 {
        common_ancestor: response.common_ancestor.clone(),
        selected_tip: response.selected_tip.clone(),
        missing_required_context,
        missing_selected_chain,
        missing_frontier,
        request_hashes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{DagFrontierEntryV1, P2P_DAG_SYNC_CONTRACT_VERSION};
    use pulsedag_core::{
        BlockConsensusMetadataV1, ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_ORDERING_VERSION,
    };

    fn identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            chain_id.to_string(),
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

    fn response() -> DagFrontierResponseV1 {
        DagFrontierResponseV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity("testnet"),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            ordering_version: GHOSTDAG_V1_ORDERING_VERSION.to_string(),
            common_ancestor: "a".to_string(),
            selected_tip: "c".to_string(),
            selected_chain_suffix: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            required_context: vec!["ctx-a".to_string(), "ctx-b".to_string()],
            frontier: vec![
                DagFrontierEntryV1 {
                    hash: "d".to_string(),
                    parents: vec!["c".to_string()],
                    consensus: metadata(Some("c"), "100"),
                },
                DagFrontierEntryV1 {
                    hash: "e".to_string(),
                    parents: vec!["ctx-a".to_string(), "d".to_string()],
                    consensus: metadata(Some("d"), "200"),
                },
            ],
        }
    }

    #[test]
    fn plan_partitions_missing_hashes_deterministically() {
        let known = ["a", "b", "ctx-a", "d"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let plan = plan_dag_frontier_reconciliation_v1(&identity("testnet"), &response(), &known)
            .expect("valid response produces plan");

        assert_eq!(plan.missing_required_context, vec!["ctx-b"]);
        assert_eq!(plan.missing_selected_chain, vec!["c"]);
        assert_eq!(plan.missing_frontier, vec!["e"]);
        assert_eq!(plan.request_hashes, vec!["c", "ctx-b", "e"]);
        assert!(!plan.is_complete());
        assert!(!plan.required_context_ready());
    }

    #[test]
    fn fully_known_response_produces_complete_plan() {
        let known = ["a", "b", "c", "ctx-a", "ctx-b", "d", "e"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let plan = plan_dag_frontier_reconciliation_v1(&identity("testnet"), &response(), &known)
            .expect("valid response produces plan");

        assert!(plan.is_complete());
        assert!(plan.required_context_ready());
        assert!(plan.missing_required_context.is_empty());
        assert!(plan.missing_selected_chain.is_empty());
        assert!(plan.missing_frontier.is_empty());
    }

    #[test]
    fn missing_common_ancestor_fails_closed() {
        let known = BTreeSet::new();
        assert_eq!(
            plan_dag_frontier_reconciliation_v1(&identity("testnet"), &response(), &known),
            Err(DagFrontierReconcileError::CommonAncestorUnavailable {
                hash: "a".to_string()
            })
        );
    }

    #[test]
    fn protocol_identity_mismatch_fails_closed() {
        let known = ["a".to_string()].into_iter().collect::<BTreeSet<_>>();
        assert_eq!(
            plan_dag_frontier_reconciliation_v1(&identity("other"), &response(), &known),
            Err(DagFrontierReconcileError::ProtocolIdentityMismatch)
        );
    }

    #[test]
    fn malformed_remote_frontier_is_rejected_before_planning() {
        let mut malformed = response();
        malformed.selected_chain_suffix = vec!["a".to_string(), "a".to_string(), "c".to_string()];
        let known = ["a".to_string()].into_iter().collect::<BTreeSet<_>>();

        assert!(matches!(
            plan_dag_frontier_reconciliation_v1(&identity("testnet"), &malformed, &known),
            Err(DagFrontierReconcileError::Contract(
                DagSyncContractError::SelectedChainDuplicate { .. }
            ))
        ));
    }
}
