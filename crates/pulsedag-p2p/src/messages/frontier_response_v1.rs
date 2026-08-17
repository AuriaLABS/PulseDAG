use std::collections::BTreeSet;

use pulsedag_core::{
    types::Hash, BlockConsensusMetadataV1, ChainState, ProtocolActivationIdentity,
    CONSENSUS_METADATA_SCHEMA_VERSION,
};

use super::{
    resolve_selected_common_ancestor_v1, DagFrontierEntryV1, DagFrontierResponseV1,
    DagSyncContractError, SelectedChainLocatorV1, SelectedLocatorError,
    MAX_SELECTED_CHAIN_SUFFIX_HASHES, P2P_DAG_SYNC_CONTRACT_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagFrontierBuildErrorV1 {
    InvalidProtocolIdentity { detail: String },
    StateChainIdMismatch { expected: String, observed: String },
    StateGenesisMismatch { expected: Hash, observed: Hash },
    StateOrderingVersionMismatch { expected: String, observed: String },
    Locator(SelectedLocatorError),
    MissingLocalBlock { hash: Hash },
    MissingConsensusMetadata { hash: Hash, field: String },
    NonCanonicalLocalHashes { hash: Hash, field: String },
    SelectedParentNotHeaderParent { hash: Hash, selected_parent: Hash },
    MissingReferencedContext { hash: Hash },
    Contract(DagSyncContractError),
}

fn validate_canonical_hashes(
    owner_hash: &str,
    field: &str,
    values: &[Hash],
) -> Result<(), DagFrontierBuildErrorV1> {
    if values.iter().any(String::is_empty) || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(DagFrontierBuildErrorV1::NonCanonicalLocalHashes {
            hash: owner_hash.to_string(),
            field: field.to_string(),
        });
    }
    Ok(())
}

fn canonicalize_merge_set_for_wire(
    hash: &Hash,
    field: &str,
    values: Option<&[Hash]>,
) -> Result<Vec<Hash>, DagFrontierBuildErrorV1> {
    let values = values.ok_or_else(|| DagFrontierBuildErrorV1::MissingConsensusMetadata {
        hash: hash.clone(),
        field: field.to_string(),
    })?;
    if values.iter().any(String::is_empty) {
        return Err(DagFrontierBuildErrorV1::NonCanonicalLocalHashes {
            hash: hash.clone(),
            field: field.to_string(),
        });
    }
    let mut canonical = values.to_vec();
    canonical.sort();
    if canonical.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(DagFrontierBuildErrorV1::NonCanonicalLocalHashes {
            hash: hash.clone(),
            field: field.to_string(),
        });
    }
    Ok(canonical)
}

fn collect_required_reference(
    state: &ChainState,
    selected_suffix: &BTreeSet<Hash>,
    frontier: &BTreeSet<Hash>,
    required: &mut BTreeSet<Hash>,
    referenced_hash: &Hash,
) -> Result<(), DagFrontierBuildErrorV1> {
    if selected_suffix.contains(referenced_hash) || frontier.contains(referenced_hash) {
        return Ok(());
    }
    if !state.dag.blocks.contains_key(referenced_hash) {
        return Err(DagFrontierBuildErrorV1::MissingReferencedContext {
            hash: referenced_hash.clone(),
        });
    }
    required.insert(referenced_hash.clone());
    Ok(())
}

fn collect_block_context(
    state: &ChainState,
    hash: &Hash,
    selected_suffix: &BTreeSet<Hash>,
    frontier: &BTreeSet<Hash>,
    required: &mut BTreeSet<Hash>,
) -> Result<(), DagFrontierBuildErrorV1> {
    let block = state
        .dag
        .blocks
        .get(hash)
        .ok_or_else(|| DagFrontierBuildErrorV1::MissingLocalBlock { hash: hash.clone() })?;
    validate_canonical_hashes(hash, "parents", &block.header.parents)?;
    for parent in &block.header.parents {
        collect_required_reference(state, selected_suffix, frontier, required, parent)?;
    }

    let selected_parent = state.dag.selected_parents.get(hash).ok_or_else(|| {
        DagFrontierBuildErrorV1::MissingConsensusMetadata {
            hash: hash.clone(),
            field: "selected_parent".to_string(),
        }
    })?;
    if let Some(selected_parent) = selected_parent {
        if !block
            .header
            .parents
            .iter()
            .any(|parent| parent == selected_parent)
        {
            return Err(DagFrontierBuildErrorV1::SelectedParentNotHeaderParent {
                hash: hash.clone(),
                selected_parent: selected_parent.clone(),
            });
        }
        collect_required_reference(state, selected_suffix, frontier, required, selected_parent)?;
    }

    let merge_set_blues = canonicalize_merge_set_for_wire(
        hash,
        "merge_set_blues",
        state.dag.merge_set_blues.get(hash).map(Vec::as_slice),
    )?;
    let merge_set_reds = canonicalize_merge_set_for_wire(
        hash,
        "merge_set_reds",
        state.dag.merge_set_reds.get(hash).map(Vec::as_slice),
    )?;
    for member in merge_set_blues.iter().chain(&merge_set_reds) {
        collect_required_reference(state, selected_suffix, frontier, required, member)?;
    }
    Ok(())
}

fn frontier_entry(
    state: &ChainState,
    hash: &Hash,
) -> Result<DagFrontierEntryV1, DagFrontierBuildErrorV1> {
    let block = state
        .dag
        .blocks
        .get(hash)
        .ok_or_else(|| DagFrontierBuildErrorV1::MissingLocalBlock { hash: hash.clone() })?;
    validate_canonical_hashes(hash, "parents", &block.header.parents)?;

    let selected_parent = state
        .dag
        .selected_parents
        .get(hash)
        .cloned()
        .ok_or_else(|| DagFrontierBuildErrorV1::MissingConsensusMetadata {
            hash: hash.clone(),
            field: "selected_parent".to_string(),
        })?;
    if let Some(ref selected_parent) = selected_parent {
        if !block
            .header
            .parents
            .iter()
            .any(|parent| parent == selected_parent)
        {
            return Err(DagFrontierBuildErrorV1::SelectedParentNotHeaderParent {
                hash: hash.clone(),
                selected_parent: selected_parent.clone(),
            });
        }
    }

    let blue_work = state.dag.blue_work.get(hash).ok_or_else(|| {
        DagFrontierBuildErrorV1::MissingConsensusMetadata {
            hash: hash.clone(),
            field: "blue_work".to_string(),
        }
    })?;
    let merge_set_blues = canonicalize_merge_set_for_wire(
        hash,
        "merge_set_blues",
        state.dag.merge_set_blues.get(hash).map(Vec::as_slice),
    )?;
    let merge_set_reds = canonicalize_merge_set_for_wire(
        hash,
        "merge_set_reds",
        state.dag.merge_set_reds.get(hash).map(Vec::as_slice),
    )?;

    Ok(DagFrontierEntryV1 {
        hash: hash.clone(),
        parents: block.header.parents.clone(),
        consensus: BlockConsensusMetadataV1 {
            selected_parent,
            blue_score: block.header.blue_score,
            blue_work_decimal: blue_work.to_string(),
            merge_set_blues,
            merge_set_reds,
        },
    })
}

/// Build the canonical local Task 27 frontier response for one validated remote locator.
///
/// `None` is an explicit pruning/retained-history outcome: no local selected-chain
/// anchor overlaps the peer locator. The caller must use its pruning-aware recovery
/// policy instead of inventing an ancestor.
pub fn build_dag_frontier_response_v1(
    expected_protocol_identity: &ProtocolActivationIdentity,
    remote_locator: &SelectedChainLocatorV1,
    state: &ChainState,
) -> Result<Option<DagFrontierResponseV1>, DagFrontierBuildErrorV1> {
    expected_protocol_identity
        .validate()
        .map_err(|detail| DagFrontierBuildErrorV1::InvalidProtocolIdentity { detail })?;
    if state.chain_id != expected_protocol_identity.chain_id {
        return Err(DagFrontierBuildErrorV1::StateChainIdMismatch {
            expected: expected_protocol_identity.chain_id.clone(),
            observed: state.chain_id.clone(),
        });
    }
    if state.dag.genesis_hash != expected_protocol_identity.genesis_hash {
        return Err(DagFrontierBuildErrorV1::StateGenesisMismatch {
            expected: expected_protocol_identity.genesis_hash.clone(),
            observed: state.dag.genesis_hash.clone(),
        });
    }
    if state.dag.ordering_version != expected_protocol_identity.dag_ordering_version {
        return Err(DagFrontierBuildErrorV1::StateOrderingVersionMismatch {
            expected: expected_protocol_identity.dag_ordering_version.clone(),
            observed: state.dag.ordering_version.clone(),
        });
    }

    let common = resolve_selected_common_ancestor_v1(
        expected_protocol_identity,
        remote_locator,
        &state.dag.selected_chain,
    )
    .map_err(DagFrontierBuildErrorV1::Locator)?;
    let Some(common) = common else {
        return Ok(None);
    };

    let full_selected_chain_suffix = &state.dag.selected_chain[common.selected_chain_index..];
    let response_is_chunked = full_selected_chain_suffix.len() > MAX_SELECTED_CHAIN_SUFFIX_HASHES;
    let selected_chain_suffix = full_selected_chain_suffix
        .iter()
        .take(MAX_SELECTED_CHAIN_SUFFIX_HASHES)
        .cloned()
        .collect::<Vec<_>>();
    for hash in &selected_chain_suffix {
        if !state.dag.blocks.contains_key(hash) {
            return Err(DagFrontierBuildErrorV1::MissingLocalBlock { hash: hash.clone() });
        }
    }
    let selected_tip = selected_chain_suffix.last().cloned().ok_or_else(|| {
        DagFrontierBuildErrorV1::MissingLocalBlock {
            hash: common.hash.clone(),
        }
    })?;
    let selected_suffix_hashes = selected_chain_suffix
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let frontier_hashes = if response_is_chunked {
        BTreeSet::new()
    } else {
        state.dag.tips.iter().cloned().collect::<BTreeSet<_>>()
    };
    let frontier = frontier_hashes
        .iter()
        .map(|hash| frontier_entry(state, hash))
        .collect::<Result<Vec<_>, _>>()?;

    let mut required_context = BTreeSet::new();
    for hash in selected_chain_suffix.iter().skip(1) {
        collect_block_context(
            state,
            hash,
            &selected_suffix_hashes,
            &frontier_hashes,
            &mut required_context,
        )?;
    }
    for hash in &frontier_hashes {
        collect_block_context(
            state,
            hash,
            &selected_suffix_hashes,
            &frontier_hashes,
            &mut required_context,
        )?;
    }

    let response = DagFrontierResponseV1 {
        contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
        protocol_identity: expected_protocol_identity.clone(),
        consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
        ordering_version: expected_protocol_identity.dag_ordering_version.clone(),
        common_ancestor: common.hash,
        selected_tip,
        selected_chain_suffix,
        required_context: required_context.into_iter().collect(),
        frontier,
    };
    response
        .validate_shape()
        .map_err(DagFrontierBuildErrorV1::Contract)?;
    Ok(Some(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state,
        types::{Block, BlockHeader},
        GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task27-frontier-builder";

    fn block(hash: &str, parents: &[&str], height: u64, blue_score: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 2,
                parents: parents.iter().map(|parent| (*parent).to_string()).collect(),
                timestamp: height,
                difficulty: 1,
                nonce: height,
                merkle_root: format!("mr-{hash}"),
                state_root: format!("sr-{hash}"),
                blue_score,
                height,
            },
            transactions: Vec::new(),
        }
    }

    fn fixture() -> (
        ChainState,
        ProtocolActivationIdentity,
        SelectedChainLocatorV1,
    ) {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let genesis = state.dag.genesis_hash.clone();
        state.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();

        for candidate in [
            block("b", &[&genesis], 1, 1),
            block("c", &["b"], 2, 2),
            block("ctx-a", &[&genesis], 1, 1),
            block("ctx-z", &[&genesis], 1, 1),
            block("d", &["b", "ctx-a", "ctx-z"], 2, 2),
        ] {
            state.dag.blocks.insert(candidate.hash.clone(), candidate);
        }
        state.dag.selected_chain = vec![genesis.clone(), "b".into(), "c".into()];
        state.dag.tips.clear();
        state.dag.tips.insert("d".into());
        state.dag.tips.insert("c".into());
        state
            .dag
            .selected_parents
            .insert("b".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("c".into(), Some("b".into()));
        state
            .dag
            .selected_parents
            .insert("ctx-a".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("ctx-z".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("d".into(), Some("b".into()));
        for hash in [
            genesis.clone(),
            "b".to_string(),
            "c".to_string(),
            "ctx-a".to_string(),
            "ctx-z".to_string(),
            "d".to_string(),
        ] {
            state.dag.merge_set_blues.entry(hash.clone()).or_default();
            state.dag.merge_set_reds.entry(hash).or_default();
        }
        state.dag.blue_work.insert("b".into(), 10);
        state.dag.blue_work.insert("c".into(), 20);
        state.dag.blue_work.insert("ctx-a".into(), 5);
        state.dag.blue_work.insert("ctx-z".into(), 6);
        state.dag.blue_work.insert("d".into(), 19);
        state
            .dag
            .merge_set_blues
            .insert("d".into(), vec!["ctx-a".into(), "ctx-z".into()]);

        let identity = ProtocolActivationIdentity::activated_v2(
            CHAIN_ID.to_string(),
            genesis.clone(),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        );
        let locator = SelectedChainLocatorV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity.clone(),
            selected_tip: "remote-tip".into(),
            locator: vec!["remote-tip".into(), "b".into(), genesis],
        };
        (state, identity, locator)
    }

    #[test]
    fn builds_canonical_frontier_from_nearest_common_ancestor() {
        let (state, identity, locator) = fixture();
        let response = build_dag_frontier_response_v1(&identity, &locator, &state)
            .unwrap()
            .expect("retained common ancestor");

        assert_eq!(response.common_ancestor, "b");
        assert_eq!(response.selected_tip, "c");
        assert_eq!(response.selected_chain_suffix, vec!["b", "c"]);
        assert_eq!(response.required_context, vec!["ctx-a", "ctx-z"]);
        assert_eq!(
            response
                .frontier
                .iter()
                .map(|entry| entry.hash.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "d"]
        );
        let d = response
            .frontier
            .iter()
            .find(|entry| entry.hash == "d")
            .unwrap();
        assert_eq!(d.consensus.selected_parent.as_deref(), Some("b"));
        assert_eq!(d.consensus.blue_work_decimal, "19");
        assert_eq!(d.consensus.merge_set_blues, vec!["ctx-a", "ctx-z"]);
        assert_eq!(response.validate_shape(), Ok(()));
    }

    #[test]
    fn response_is_deterministic_across_tip_insertion_order() {
        let (state, identity, locator) = fixture();
        let first = build_dag_frontier_response_v1(&identity, &locator, &state).unwrap();
        let mut reordered = state.clone();
        reordered.dag.tips.clear();
        reordered.dag.tips.insert("c".into());
        reordered.dag.tips.insert("d".into());
        let second = build_dag_frontier_response_v1(&identity, &locator, &reordered).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn merge_set_order_is_canonicalized_for_wire_without_reinterpreting_membership() {
        let (mut state, identity, locator) = fixture();
        state
            .dag
            .merge_set_blues
            .insert("d".into(), vec!["ctx-z".into(), "ctx-a".into()]);
        let response = build_dag_frontier_response_v1(&identity, &locator, &state)
            .unwrap()
            .expect("retained common ancestor");
        let d = response
            .frontier
            .iter()
            .find(|entry| entry.hash == "d")
            .unwrap();
        assert_eq!(d.consensus.merge_set_blues, vec!["ctx-a", "ctx-z"]);
    }

    #[test]
    fn missing_merge_set_metadata_fails_closed() {
        let (mut state, identity, locator) = fixture();
        state.dag.merge_set_reds.remove("d");
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::MissingConsensusMetadata {
                hash: "d".into(),
                field: "merge_set_reds".into(),
            })
        );
    }

    #[test]
    fn long_selected_chain_is_chunked_and_next_locator_makes_progress() {
        let (mut state, identity, locator) = fixture();
        let mut previous = "c".to_string();
        let start_height = 3_u64;
        for index in 0..MAX_SELECTED_CHAIN_SUFFIX_HASHES {
            let hash = format!("long-{index:05}");
            let height = start_height.saturating_add(index as u64);
            state
                .dag
                .blocks
                .insert(hash.clone(), block(&hash, &[&previous], height, height));
            state
                .dag
                .selected_parents
                .insert(hash.clone(), Some(previous.clone()));
            state.dag.blue_work.insert(hash.clone(), height as u128);
            state.dag.merge_set_blues.insert(hash.clone(), Vec::new());
            state.dag.merge_set_reds.insert(hash.clone(), Vec::new());
            state.dag.selected_chain.push(hash.clone());
            previous = hash;
        }
        state.dag.tips.clear();
        state.dag.tips.insert(previous.clone());

        let first = build_dag_frontier_response_v1(&identity, &locator, &state)
            .unwrap()
            .expect("first retained chunk");
        assert_eq!(
            first.selected_chain_suffix.len(),
            MAX_SELECTED_CHAIN_SUFFIX_HASHES
        );
        assert!(first.frontier.is_empty());
        assert_ne!(first.selected_tip, previous);
        assert_eq!(first.validate_shape(), Ok(()));

        let next_locator = SelectedChainLocatorV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity.clone(),
            selected_tip: first.selected_tip.clone(),
            locator: vec![first.selected_tip.clone()],
        };
        let second = build_dag_frontier_response_v1(&identity, &next_locator, &state)
            .unwrap()
            .expect("second retained chunk");
        assert_eq!(
            second.selected_chain_suffix.first(),
            Some(&first.selected_tip)
        );
        assert_eq!(second.selected_tip, previous);
        assert!(!second.frontier.is_empty());
        assert_eq!(second.validate_shape(), Ok(()));
    }

    #[test]
    fn selected_chain_hash_missing_from_blocks_fails_closed() {
        let (mut state, identity, locator) = fixture();
        state.dag.blocks.remove("c");
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::MissingLocalBlock { hash: "c".into() })
        );
    }

    #[test]
    fn no_retained_locator_overlap_returns_explicit_none() {
        let (state, identity, mut locator) = fixture();
        locator.selected_tip = "remote-only-tip".into();
        locator.locator = vec!["remote-only-tip".into(), "remote-only-old".into()];
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state).unwrap(),
            None
        );
    }

    #[test]
    fn state_ordering_identity_mismatch_fails_closed() {
        let (mut state, identity, locator) = fixture();
        state.dag.ordering_version = "other-ordering".into();
        assert!(matches!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::StateOrderingVersionMismatch { .. })
        ));
    }

    #[test]
    fn missing_frontier_blue_work_fails_closed() {
        let (mut state, identity, locator) = fixture();
        state.dag.blue_work.remove("d");
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::MissingConsensusMetadata {
                hash: "d".into(),
                field: "blue_work".into(),
            })
        );
    }

    #[test]
    fn missing_required_context_block_fails_closed() {
        let (mut state, identity, locator) = fixture();
        state.dag.blocks.remove("ctx-z");
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::MissingReferencedContext {
                hash: "ctx-z".into(),
            })
        );
    }

    #[test]
    fn noncanonical_local_parent_order_is_not_silently_reinterpreted() {
        let (mut state, identity, locator) = fixture();
        state.dag.blocks.get_mut("d").unwrap().header.parents =
            vec!["ctx-z".into(), "b".into(), "ctx-a".into()];
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::NonCanonicalLocalHashes {
                hash: "d".into(),
                field: "parents".into(),
            })
        );
    }

    #[test]
    fn selected_parent_must_be_an_actual_header_parent() {
        let (mut state, identity, locator) = fixture();
        state
            .dag
            .selected_parents
            .insert("d".into(), Some("c".into()));
        assert_eq!(
            build_dag_frontier_response_v1(&identity, &locator, &state),
            Err(DagFrontierBuildErrorV1::SelectedParentNotHeaderParent {
                hash: "d".into(),
                selected_parent: "c".into(),
            })
        );
    }
}
