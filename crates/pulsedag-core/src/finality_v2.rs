use serde::{Deserialize, Serialize};

use crate::{
    ordering_v2::{derive_ordered_dag_v2, OrderingV2Error},
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    types::Hash,
};

/// v2.4.0 starts fail-closed: historical pruning remains disabled until the
/// adversarial/replay program in Task 30 produces explicit evidence supporting
/// a stronger checkpoint/finality policy.
pub const GHOSTDAG_V1_FINALITY_POLICY_VERSION: &str = "ghostdag-v1-no-prune-before-task30-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum FinalityV2Error {
    Ordering {
        detail: String,
    },
    MissingGenesisBlock {
        hash: Hash,
    },
    SelectedChainDoesNotStartAtGenesis {
        expected: Hash,
        observed: Option<Hash>,
    },
}

impl From<OrderingV2Error> for FinalityV2Error {
    fn from(error: OrderingV2Error) -> Self {
        Self::Ordering {
            detail: format!("{error:?}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FinalityBoundaryV1 {
    pub policy_version: String,
    pub protocol_identity: ProtocolActivationIdentity,
    pub boundary_hash: Hash,
    pub boundary_height: u64,
    pub selected_tip: Option<Hash>,
    pub ordered_dag_digest: String,
    pub pruning_enabled: bool,
    pub prunable_blocks: Vec<Hash>,
    pub required_context_blocks: Vec<Hash>,
    pub reason: String,
}

/// Derive the initial v2.4.0 finality/pruning boundary.
///
/// This deliberately does not pretend that a confirmation count is absolute
/// PoW finality. Until Task 30 validates an explicit stronger policy, the only
/// history-discard boundary is genesis and *no accepted block is prunable*.
/// Before returning even that conservative boundary, the complete reserved
/// v2.4 ordering context must validate. A compact state missing parents,
/// classification, selected-chain metadata, or blue-work therefore fails
/// closed through `derive_ordered_dag_v2` rather than inventing a boundary.
pub fn derive_finality_boundary_v1(
    state: &ChainState,
) -> Result<FinalityBoundaryV1, FinalityV2Error> {
    let genesis_hash = state.dag.genesis_hash.clone();
    let genesis = state.dag.blocks.get(&genesis_hash).ok_or_else(|| {
        FinalityV2Error::MissingGenesisBlock {
            hash: genesis_hash.clone(),
        }
    })?;

    if state.dag.selected_chain.first() != Some(&genesis_hash) {
        return Err(FinalityV2Error::SelectedChainDoesNotStartAtGenesis {
            expected: genesis_hash,
            observed: state.dag.selected_chain.first().cloned(),
        });
    }

    let ordered = derive_ordered_dag_v2(state)?;
    let mut required_context_blocks = state.dag.blocks.keys().cloned().collect::<Vec<_>>();
    required_context_blocks.sort();

    Ok(FinalityBoundaryV1 {
        policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
        protocol_identity: ProtocolActivationIdentity::legacy_from_state(state),
        boundary_hash: state.dag.genesis_hash.clone(),
        boundary_height: genesis.header.height,
        selected_tip: state.dag.selected_chain.last().cloned(),
        ordered_dag_digest: ordered.digest,
        pruning_enabled: false,
        prunable_blocks: Vec::new(),
        required_context_blocks,
        reason: "historical pruning is disabled until Task 30 validates and freezes a stronger v2.4 finality/checkpoint policy".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{Block, BlockHeader},
    };

    fn block(hash: &str, parents: Vec<&str>, height: u64, blue_score: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: parents.into_iter().map(str::to_string).collect(),
                timestamp: height.saturating_add(10),
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("m-{hash}"),
                state_root: format!("s-{hash}"),
                blue_score,
                height,
            },
            transactions: vec![],
        }
    }

    fn complete_diamond(reverse_insert: bool) -> ChainState {
        let mut state = init_chain_state("finality-v2-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        let a = block("a", vec![&genesis], 1, 10);
        let b = block("b", vec![&genesis], 1, 9);
        let c = block("c", vec!["a", "b"], 2, 20);
        let entries = if reverse_insert {
            vec![c.clone(), b.clone(), a.clone()]
        } else {
            vec![a.clone(), b.clone(), c.clone()]
        };
        for candidate in entries {
            state.dag.blue_work.insert(
                candidate.hash.clone(),
                candidate.header.blue_score as u128 * 10,
            );
            state.dag.blocks.insert(candidate.hash.clone(), candidate);
        }

        state
            .dag
            .selected_parents
            .insert("a".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("b".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("c".into(), Some("a".into()));
        state.dag.selected_chain = vec![genesis.clone(), "a".into(), "c".into()];
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis, vec![]);
        state.dag.merge_set_blues.insert("a".into(), vec![]);
        state.dag.merge_set_reds.insert("a".into(), vec![]);
        state
            .dag
            .merge_set_blues
            .insert("c".into(), vec!["b".into()]);
        state.dag.merge_set_reds.insert("c".into(), vec![]);
        state
    }

    #[test]
    fn initial_policy_is_genesis_boundary_and_prunes_nothing() {
        let state = complete_diamond(false);
        let boundary = derive_finality_boundary_v1(&state).unwrap();

        assert_eq!(boundary.boundary_hash, state.dag.genesis_hash);
        assert_eq!(boundary.boundary_height, 0);
        assert!(!boundary.pruning_enabled);
        assert!(boundary.prunable_blocks.is_empty());
        assert_eq!(
            boundary.required_context_blocks.len(),
            state.dag.blocks.len()
        );
    }

    #[test]
    fn boundary_is_independent_of_block_insertion_order() {
        let forward = derive_finality_boundary_v1(&complete_diamond(false)).unwrap();
        let reverse = derive_finality_boundary_v1(&complete_diamond(true)).unwrap();

        assert_eq!(forward, reverse);
    }

    #[test]
    fn incomplete_compact_context_fails_closed() {
        let mut state = complete_diamond(false);
        state.dag.merge_set_blues.get_mut("c").unwrap().clear();

        assert!(matches!(
            derive_finality_boundary_v1(&state),
            Err(FinalityV2Error::Ordering { .. })
        ));
    }

    #[test]
    fn selected_chain_must_start_at_genesis() {
        let mut state = complete_diamond(false);
        state.dag.selected_chain.remove(0);

        assert!(matches!(
            derive_finality_boundary_v1(&state),
            Err(FinalityV2Error::SelectedChainDoesNotStartAtGenesis { .. })
        ));
    }
}
