use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::{types::Hash, ProtocolActivationIdentity};

use super::{
    DagSyncContractError, SelectedChainLocatorV1, MAX_SELECTED_CHAIN_LOCATOR_HASHES,
    P2P_DAG_SYNC_CONTRACT_VERSION,
};

/// Number of newest selected-chain hashes advertised one-by-one before the
/// locator switches to exponential backoff.
pub const SELECTED_LOCATOR_LINEAR_WINDOW: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedLocatorError {
    EmptySelectedChain,
    EmptySelectedChainHash { index: usize },
    DuplicateSelectedChainHash { hash: Hash },
    InvalidProtocolIdentity { detail: String },
    ProtocolIdentityMismatch,
    Contract(DagSyncContractError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCommonAncestorV1 {
    pub hash: Hash,
    /// Zero-based position in the local selected chain, which is ordered from
    /// oldest/genesis toward selected tip.
    pub selected_chain_index: usize,
}

fn validate_selected_chain(selected_chain: &[Hash]) -> Result<(), SelectedLocatorError> {
    if selected_chain.is_empty() {
        return Err(SelectedLocatorError::EmptySelectedChain);
    }

    let mut seen = BTreeSet::new();
    for (index, hash) in selected_chain.iter().enumerate() {
        if hash.is_empty() {
            return Err(SelectedLocatorError::EmptySelectedChainHash { index });
        }
        if !seen.insert(hash.clone()) {
            return Err(SelectedLocatorError::DuplicateSelectedChainHash { hash: hash.clone() });
        }
    }
    Ok(())
}

/// Build the deterministic selected-chain locator used by Task 27.
///
/// `selected_chain` is ordered oldest/genesis -> selected tip. The locator is
/// emitted in the opposite direction: selected tip -> older anchors. The ten
/// newest entries are contiguous; after that the distance doubles until the
/// oldest retained selected-chain hash is included. This gives deterministic
/// recent-fork precision without making locator size grow linearly with chain
/// length.
pub fn build_selected_chain_locator_v1(
    protocol_identity: ProtocolActivationIdentity,
    selected_chain: &[Hash],
) -> Result<SelectedChainLocatorV1, SelectedLocatorError> {
    protocol_identity
        .validate()
        .map_err(|detail| SelectedLocatorError::InvalidProtocolIdentity { detail })?;
    validate_selected_chain(selected_chain)?;

    let mut locator = Vec::new();
    let mut index = selected_chain.len() - 1;
    let mut step = 1usize;

    loop {
        locator.push(selected_chain[index].clone());
        if index == 0 {
            break;
        }

        if locator.len() < SELECTED_LOCATOR_LINEAR_WINDOW {
            index -= 1;
        } else {
            step = step.saturating_mul(2).max(2);
            index = index.saturating_sub(step);
        }
    }

    if locator.last() != selected_chain.first() {
        locator.push(selected_chain[0].clone());
    }

    if locator.len() > MAX_SELECTED_CHAIN_LOCATOR_HASHES {
        return Err(SelectedLocatorError::Contract(
            DagSyncContractError::LocatorTooLarge {
                observed: locator.len(),
                maximum: MAX_SELECTED_CHAIN_LOCATOR_HASHES,
            },
        ));
    }

    let result = SelectedChainLocatorV1 {
        contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
        protocol_identity,
        selected_tip: selected_chain
            .last()
            .expect("validated selected chain is non-empty")
            .clone(),
        locator,
    };
    result
        .validate_shape()
        .map_err(SelectedLocatorError::Contract)?;
    Ok(result)
}

/// Resolve the newest locator anchor that exists in the local selected chain.
///
/// The peer locator is already ordered newest -> oldest, so the first match is
/// the deterministic common ancestor candidate. `None` is an explicit result:
/// it means the supplied locator cannot bridge the retained local selected
/// history and the caller must use the pruning/mixed-version recovery policy
/// rather than inventing ancestry.
pub fn resolve_selected_common_ancestor_v1(
    expected_protocol_identity: &ProtocolActivationIdentity,
    locator: &SelectedChainLocatorV1,
    local_selected_chain: &[Hash],
) -> Result<Option<SelectedCommonAncestorV1>, SelectedLocatorError> {
    expected_protocol_identity
        .validate()
        .map_err(|detail| SelectedLocatorError::InvalidProtocolIdentity { detail })?;
    locator
        .validate_shape()
        .map_err(SelectedLocatorError::Contract)?;
    if &locator.protocol_identity != expected_protocol_identity {
        return Err(SelectedLocatorError::ProtocolIdentityMismatch);
    }
    validate_selected_chain(local_selected_chain)?;

    let local_indexes = local_selected_chain
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash.clone(), index))
        .collect::<BTreeMap<_, _>>();

    Ok(locator.locator.iter().find_map(|hash| {
        local_indexes
            .get(hash)
            .copied()
            .map(|selected_chain_index| SelectedCommonAncestorV1 {
                hash: hash.clone(),
                selected_chain_index,
            })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::GHOSTDAG_V1_ORDERING_VERSION;

    fn identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            chain_id.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn chain(count: usize) -> Vec<Hash> {
        (0..count)
            .map(|index| format!("block-{index:05}"))
            .collect()
    }

    #[test]
    fn short_selected_chain_locator_is_complete_and_reversed() {
        let selected = chain(6);
        let locator = build_selected_chain_locator_v1(identity("testnet"), &selected).unwrap();

        assert_eq!(locator.selected_tip, "block-00005");
        assert_eq!(
            locator.locator,
            vec![
                "block-00005",
                "block-00004",
                "block-00003",
                "block-00002",
                "block-00001",
                "block-00000",
            ]
        );
    }

    #[test]
    fn long_locator_is_deterministic_bounded_and_contains_oldest_anchor() {
        let selected = chain(100_000);
        let first = build_selected_chain_locator_v1(identity("testnet"), &selected).unwrap();
        let second = build_selected_chain_locator_v1(identity("testnet"), &selected).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.locator.first(), Some(&"block-99999".to_string()));
        assert_eq!(first.locator.last(), Some(&"block-00000".to_string()));
        assert!(first.locator.len() <= MAX_SELECTED_CHAIN_LOCATOR_HASHES);
        assert_eq!(
            first
                .locator
                .iter()
                .take(SELECTED_LOCATOR_LINEAR_WINDOW)
                .cloned()
                .collect::<Vec<_>>(),
            (99_990..=99_999)
                .rev()
                .map(|index| format!("block-{index:05}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn nearest_locator_match_is_selected_as_common_ancestor() {
        let local = chain(100);
        let remote = chain(106);
        let locator = build_selected_chain_locator_v1(identity("testnet"), &remote).unwrap();
        let common = resolve_selected_common_ancestor_v1(&identity("testnet"), &locator, &local)
            .unwrap()
            .expect("chains share retained history");

        assert_eq!(common.hash, "block-00099");
        assert_eq!(common.selected_chain_index, 99);
    }

    #[test]
    fn no_retained_locator_overlap_returns_none_instead_of_inventing_ancestry() {
        let local = vec!["local-a".to_string(), "local-b".to_string()];
        let remote = vec!["remote-a".to_string(), "remote-b".to_string()];
        let locator = build_selected_chain_locator_v1(identity("testnet"), &remote).unwrap();

        assert_eq!(
            resolve_selected_common_ancestor_v1(&identity("testnet"), &locator, &local).unwrap(),
            None
        );
    }

    #[test]
    fn mismatched_protocol_identity_fails_closed_before_ancestor_selection() {
        let selected = chain(5);
        let locator = build_selected_chain_locator_v1(identity("testnet"), &selected).unwrap();

        assert_eq!(
            resolve_selected_common_ancestor_v1(&identity("other-chain"), &locator, &selected),
            Err(SelectedLocatorError::ProtocolIdentityMismatch)
        );
    }

    #[test]
    fn malformed_local_selected_chain_is_rejected() {
        let remote = chain(4);
        let locator = build_selected_chain_locator_v1(identity("testnet"), &remote).unwrap();
        let local = vec!["a".to_string(), "a".to_string()];

        assert!(matches!(
            resolve_selected_common_ancestor_v1(&identity("testnet"), &locator, &local),
            Err(SelectedLocatorError::DuplicateSelectedChainHash { .. })
        ));
    }
}
