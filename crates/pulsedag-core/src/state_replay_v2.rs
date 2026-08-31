use serde::{Deserialize, Serialize};

use crate::{
    apply::apply_transaction,
    errors::PulseError,
    genesis::init_chain_state,
    genesis_v2::init_chain_state_v2,
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2, GHOSTDAG_V1_ORDERING_VERSION},
    protocol::{BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2},
    state::{ChainState, UtxoState},
    tx::{TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2},
    types::Hash,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateReplayV2Diagnostics {
    pub applied_transactions: usize,
    pub skipped_conflicting_transactions: usize,
    pub conflict_diagnostics: Vec<String>,
    pub state_root: String,
    pub ordered_dag_tip: Option<Hash>,
    pub ordered_dag_digest: String,
}

#[derive(Debug, Clone)]
pub struct StateReplayV2 {
    pub utxo: UtxoState,
    pub ordered_dag: OrderedDagV2,
    pub diagnostics: StateReplayV2Diagnostics,
}

fn replay_base_state_for_genesis(state: &ChainState) -> Result<ChainState, PulseError> {
    let genesis = state
        .dag
        .blocks
        .get(&state.dag.genesis_hash)
        .ok_or_else(|| {
            PulseError::NonDeterministicState("v2 replay genesis block missing".into())
        })?;
    let first_tx = genesis.transactions.first().ok_or_else(|| {
        PulseError::NonDeterministicState("v2 replay genesis transaction missing".into())
    })?;
    if genesis
        .transactions
        .iter()
        .any(|tx| tx.version != first_tx.version)
    {
        return Err(PulseError::NonDeterministicState(
            "mixed transaction versions inside genesis are not supported".into(),
        ));
    }

    match (genesis.header.version, first_tx.version) {
        (BLOCK_HEADER_VERSION_V1, TRANSACTION_VERSION_V1) => {
            Ok(init_chain_state(state.chain_id.clone()))
        }
        (BLOCK_HEADER_VERSION_V2, TRANSACTION_VERSION_V2) => {
            let rebuilt = init_chain_state_v2(state.chain_id.clone())?;
            if rebuilt.dag.genesis_hash != state.dag.genesis_hash {
                return Err(PulseError::NonDeterministicState(format!(
                    "chain-bound v2 genesis mismatch: expected {}, rebuilt {}",
                    state.dag.genesis_hash, rebuilt.dag.genesis_hash
                )));
            }
            Ok(rebuilt)
        }
        (header_version, transaction_version) => Err(PulseError::NonDeterministicState(format!(
            "unsupported mixed genesis protocol versions: header={header_version} transaction={transaction_version}"
        ))),
    }
}

fn compact_pruned_checkpoint_floor(state: &ChainState) -> Option<u64> {
    if state.dag.blocks.is_empty() || state.dag.blocks.contains_key(&state.dag.genesis_hash) {
        return None;
    }
    let retained_floor = state
        .dag
        .blocks
        .values()
        .map(|block| block.header.height)
        .min()?;
    (retained_floor > 0).then_some(retained_floor)
}

fn conflict_diagnostic_block_hash(entry: &str) -> Option<&str> {
    entry
        .split_whitespace()
        .find_map(|part| part.strip_prefix("block="))
}

fn verify_compact_pruned_authoritative_checkpoint_v2(
    state: &ChainState,
    retained_floor: u64,
) -> Result<StateReplayV2Diagnostics, PulseError> {
    if state.dag.ordering_version != GHOSTDAG_V1_ORDERING_VERSION {
        return Err(PulseError::NonDeterministicState(format!(
            "v2.4 compact checkpoint ordering version {} does not match {}",
            state.dag.ordering_version, GHOSTDAG_V1_ORDERING_VERSION
        )));
    }

    for block in state.dag.blocks.values() {
        for parent in &block.header.parents {
            if state.dag.blocks.contains_key(parent) {
                continue;
            }
            if block.header.height != retained_floor {
                return Err(PulseError::NonDeterministicState(format!(
                    "v2.4 compact checkpoint block {} at height {} references missing parent {} above retained floor {}",
                    block.hash, block.header.height, parent, retained_floor
                )));
            }
        }
    }

    let ordered_dag = derive_ordered_dag_v2(state).map_err(|err| {
        PulseError::NonDeterministicState(format!(
            "v2.4 compact checkpoint authoritative ordering unavailable: {err:?}"
        ))
    })?;
    if state.dag.ordered_dag != ordered_dag.blocks {
        return Err(PulseError::NonDeterministicState(
            "v2.4 compact checkpoint ordered DAG does not match retained authoritative recomputation"
                .to_string(),
        ));
    }

    let ordered_dag_tip = ordered_dag.blocks.last().cloned();
    if state.dag.ordered_dag_tip != ordered_dag_tip {
        return Err(PulseError::NonDeterministicState(
            "v2.4 compact checkpoint ordered DAG tip does not match retained authoritative recomputation"
                .to_string(),
        ));
    }

    let observed_state_root = state.utxo.compute_state_root()?;
    if state.dag.ordered_dag_state_root.as_deref() != Some(observed_state_root.as_str()) {
        return Err(PulseError::NonDeterministicState(
            "v2.4 compact checkpoint recorded state root does not match persisted UTXO checkpoint"
                .to_string(),
        ));
    }

    for entry in &state.dag.ordered_dag_conflict_diagnostics {
        let Some(block_hash) = conflict_diagnostic_block_hash(entry) else {
            return Err(PulseError::NonDeterministicState(
                "v2.4 compact checkpoint conflict diagnostic is missing block identity"
                    .to_string(),
            ));
        };
        if !state.dag.blocks.contains_key(block_hash) {
            return Err(PulseError::NonDeterministicState(format!(
                "v2.4 compact checkpoint conflict diagnostic references pruned or unknown block {block_hash}"
            )));
        }
    }

    Ok(StateReplayV2Diagnostics {
        applied_transactions: 0,
        skipped_conflicting_transactions: state.dag.ordered_dag_conflict_diagnostics.len(),
        conflict_diagnostics: state.dag.ordered_dag_conflict_diagnostics.clone(),
        state_root: observed_state_root,
        ordered_dag_tip,
        ordered_dag_digest: ordered_dag.digest,
    })
}

/// Replay the reserved v2.4.0 authoritative DAG order with transaction-level
/// atomicity.
///
/// A transaction that becomes conflicting because a preceding DAG transaction
/// already consumed one of its inputs is skipped as one atomic unit. The
/// working state is cloned before each transaction and published only after the
/// whole transaction succeeds, so an error on a later input cannot leak an
/// earlier input removal into the rebuilt UTXO set.
///
/// This calculator is non-activating. Live `legacy` and `ghostdag_dev` replay
/// continue to use their existing paths until the v2.4 activation gate is
/// wired after the full Task 26 contract is validated.
pub fn rebuild_authoritative_state_v2(state: &ChainState) -> Result<StateReplayV2, PulseError> {
    let ordered_dag = derive_ordered_dag_v2(state).map_err(|err| {
        PulseError::Internal(format!(
            "v2.4 authoritative ordering unavailable for state replay: {err:?}"
        ))
    })?;

    let mut rebuilt = replay_base_state_for_genesis(state)?;
    rebuilt.dag.consensus_mode = state.dag.consensus_mode;
    rebuilt.dag.selected_parent_policy = state.dag.selected_parent_policy;

    let mut applied_transactions = 0usize;
    let mut skipped_conflicting_transactions = 0usize;
    let mut conflict_diagnostics = Vec::new();

    for (ordered_pos, hash) in ordered_dag.blocks.iter().enumerate() {
        if hash == &state.dag.genesis_hash {
            continue;
        }
        let block = state.dag.blocks.get(hash).ok_or_else(|| {
            PulseError::Internal(format!(
                "v2.4 authoritative order references missing block {hash}"
            ))
        })?;

        for tx in &block.transactions {
            let mut candidate = rebuilt.clone();
            match apply_transaction(tx, &mut candidate, block.header.height) {
                Ok(()) => {
                    rebuilt = candidate;
                    applied_transactions = applied_transactions.saturating_add(1);
                }
                Err(PulseError::UtxoNotFound | PulseError::DuplicateUtxoOutpoint(_)) => {
                    skipped_conflicting_transactions =
                        skipped_conflicting_transactions.saturating_add(1);
                    conflict_diagnostics.push(format!(
                        "ordered_pos={ordered_pos} block={} tx={} skipped_conflict_atomic",
                        block.hash, tx.txid
                    ));
                }
                Err(err) => return Err(err),
            }
        }
    }

    let state_root = rebuilt.utxo.compute_state_root()?;
    let diagnostics = StateReplayV2Diagnostics {
        applied_transactions,
        skipped_conflicting_transactions,
        conflict_diagnostics,
        state_root,
        ordered_dag_tip: ordered_dag.blocks.last().cloned(),
        ordered_dag_digest: ordered_dag.digest.clone(),
    };

    Ok(StateReplayV2 {
        utxo: rebuilt.utxo,
        ordered_dag,
        diagnostics,
    })
}

/// Materialize a canonical, self-consistent v2.4 snapshot state without
/// mutating the caller's live runtime state.
///
/// The authoritative UTXO is rebuilt from the frozen total DAG order, then the
/// snapshot-only ordering/state-root fields are populated from that same replay.
/// Runtime consensus mode and other operational state remain unchanged.
pub fn materialize_authoritative_state_v2(state: &ChainState) -> Result<ChainState, PulseError> {
    let replay = rebuild_authoritative_state_v2(state)?;
    let mut materialized = state.clone();
    materialized.utxo = replay.utxo.clone();
    materialized.dag.ordered_dag = replay.ordered_dag.blocks.clone();
    materialized.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    materialized.dag.ordered_dag_tip = replay.diagnostics.ordered_dag_tip.clone();
    materialized.dag.ordered_dag_state_root = Some(replay.diagnostics.state_root.clone());
    materialized.dag.ordered_dag_conflict_diagnostics =
        replay.diagnostics.conflict_diagnostics.clone();
    Ok(materialized)
}

/// Verify that a persisted/restored v2.4 snapshot is already materialized from
/// the same authoritative ordering and transactional replay it claims.
///
/// Full snapshots are replayed from genesis. A compact-pruned checkpoint cannot
/// replay history that was intentionally deleted, so it is instead validated
/// against the retained authoritative DAG, the persisted checkpoint UTXO root,
/// and the strict rule that omitted parents may occur only at the retained
/// floor. Storage metadata remains responsible for binding that retained floor
/// to the original genesis and omitted-parent set.
pub fn verify_authoritative_state_snapshot_v2(
    state: &ChainState,
) -> Result<StateReplayV2Diagnostics, PulseError> {
    if let Some(retained_floor) = compact_pruned_checkpoint_floor(state) {
        return verify_compact_pruned_authoritative_checkpoint_v2(state, retained_floor);
    }

    let replay = rebuild_authoritative_state_v2(state)?;
    let observed_state_root = state.utxo.compute_state_root()?;

    if state.dag.ordering_version != GHOSTDAG_V1_ORDERING_VERSION {
        return Err(PulseError::NonDeterministicState(format!(
            "v2.4 snapshot ordering version {} does not match {}",
            state.dag.ordering_version, GHOSTDAG_V1_ORDERING_VERSION
        )));
    }
    if state.dag.ordered_dag != replay.ordered_dag.blocks {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot ordered DAG does not match authoritative recomputation".to_string(),
        ));
    }
    if state.dag.ordered_dag_tip != replay.diagnostics.ordered_dag_tip {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot ordered DAG tip does not match authoritative recomputation".to_string(),
        ));
    }
    if state.dag.ordered_dag_state_root.as_deref() != Some(replay.diagnostics.state_root.as_str()) {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot recorded state root does not match authoritative recomputation"
                .to_string(),
        ));
    }
    if observed_state_root != replay.diagnostics.state_root {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot UTXO state root does not match authoritative recomputation".to_string(),
        ));
    }
    if state.dag.ordered_dag_conflict_diagnostics != replay.diagnostics.conflict_diagnostics {
        return Err(PulseError::NonDeterministicState(
            "v2.4 snapshot conflict diagnostics do not match authoritative recomputation"
                .to_string(),
        ));
    }

    Ok(replay.diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        replay::compact_snapshot_to_retained_blocks,
        types::{Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput},
    };

    fn transaction(
        txid: &str,
        inputs: Vec<OutPoint>,
        outputs: Vec<(&str, u64)>,
        fee: u64,
    ) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: 1,
            inputs: inputs
                .into_iter()
                .map(|previous_output| TxInput {
                    previous_output,
                    public_key: "00".repeat(32),
                    signature: "00".repeat(64),
                })
                .collect(),
            outputs: outputs
                .into_iter()
                .map(|(address, amount)| TxOutput {
                    address: address.to_string(),
                    amount,
                })
                .collect(),
            fee,
            nonce: 0,
        }
    }

    fn block(hash: &str, parents: Vec<&str>, height: u64, txs: Vec<Transaction>) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: parents.into_iter().map(str::to_string).collect(),
                timestamp: height.saturating_add(100),
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("m-{hash}"),
                state_root: format!("s-{hash}"),
                blue_score: height,
                height,
            },
            transactions: txs,
        }
    }

    fn conflict_state(reverse_loser_inputs: bool) -> ChainState {
        let mut state = init_chain_state("state-replay-v2-test".to_string());
        let genesis = state.dag.genesis_hash.clone();

        let funding = transaction("funding", vec![], vec![("alice", 30), ("bob", 20)], 0);
        let funding_a = OutPoint {
            txid: funding.txid.clone(),
            index: 0,
        };
        let funding_b = OutPoint {
            txid: funding.txid.clone(),
            index: 1,
        };
        let winner = transaction("winner", vec![funding_b.clone()], vec![("winner", 20)], 0);
        let mut loser_inputs = vec![funding_a, funding_b];
        if reverse_loser_inputs {
            loser_inputs.reverse();
        }
        let loser = transaction("loser", loser_inputs, vec![("loser", 50)], 0);

        let fund = block("fund", vec![&genesis], 1, vec![funding]);
        let winner_block = block("winner-block", vec!["fund"], 2, vec![winner]);
        let loser_block = block("loser-block", vec!["fund"], 2, vec![loser]);
        let merge = block("merge", vec!["winner-block", "loser-block"], 3, vec![]);

        for (candidate, work) in [
            (fund, 100u128),
            (winner_block, 300u128),
            (loser_block, 200u128),
            (merge, 400u128),
        ] {
            state.dag.blue_work.insert(candidate.hash.clone(), work);
            state.dag.blocks.insert(candidate.hash.clone(), candidate);
        }

        state
            .dag
            .selected_parents
            .insert("fund".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("winner-block".into(), Some("fund".into()));
        state
            .dag
            .selected_parents
            .insert("loser-block".into(), Some("fund".into()));
        state
            .dag
            .selected_parents
            .insert("merge".into(), Some("winner-block".into()));
        state.dag.selected_chain = vec![
            genesis.clone(),
            "fund".into(),
            "winner-block".into(),
            "merge".into(),
        ];

        for anchor in [&genesis, "fund", "winner-block", "merge"] {
            state.dag.merge_set_blues.insert(anchor.to_string(), vec![]);
            state.dag.merge_set_reds.insert(anchor.to_string(), vec![]);
        }
        state
            .dag
            .merge_set_blues
            .insert("merge".into(), vec!["loser-block".into()]);

        state
    }

    fn compact_pruned_checkpoint_state() -> ChainState {
        let full = materialize_authoritative_state_v2(&conflict_state(false)).unwrap();
        let retained = full
            .dag
            .blocks
            .values()
            .filter(|block| block.header.height >= 2)
            .cloned()
            .collect::<Vec<_>>();
        compact_snapshot_to_retained_blocks(full, &retained).unwrap()
    }

    #[test]
    fn clean_chain_bound_v2_genesis_replays_from_v2_utxo() {
        let state =
            crate::genesis_v2::init_chain_state_v2("pulsedag-private-v2.4.0".to_string()).unwrap();
        let replay = rebuild_authoritative_state_v2(&state).unwrap();
        let expected_root = state.utxo.compute_state_root().unwrap();
        let genesis_txid = state.dag.blocks[&state.dag.genesis_hash].transactions[0]
            .txid
            .clone();

        assert_eq!(replay.diagnostics.state_root, expected_root);
        assert!(replay
            .utxo
            .utxos
            .keys()
            .any(|outpoint| outpoint.txid == genesis_txid));
        verify_authoritative_state_snapshot_v2(&state).unwrap();
    }

    #[test]
    fn compact_pruned_checkpoint_verifies_without_requiring_deleted_genesis() {
        let compact = compact_pruned_checkpoint_state();
        assert!(!compact.dag.blocks.contains_key(&compact.dag.genesis_hash));
        assert!(rebuild_authoritative_state_v2(&compact).is_err());

        let diagnostics = verify_authoritative_state_snapshot_v2(&compact).unwrap();
        assert_eq!(
            diagnostics.state_root,
            compact.utxo.compute_state_root().unwrap()
        );
        assert_eq!(diagnostics.ordered_dag_tip, compact.dag.ordered_dag_tip);
    }

    #[test]
    fn compact_pruned_checkpoint_rejects_missing_parent_above_retained_floor() {
        let mut compact = compact_pruned_checkpoint_state();
        compact.dag.blocks.get_mut("merge").unwrap().header.parents = vec!["missing".into()];

        let error = verify_authoritative_state_snapshot_v2(&compact)
            .expect_err("missing parent above checkpoint floor must fail closed");
        assert!(error.to_string().contains("missing parent"));
    }

    #[test]
    fn compact_pruned_checkpoint_rejects_stale_state_root_commitment() {
        let mut compact = compact_pruned_checkpoint_state();
        compact.dag.ordered_dag_state_root = Some("00".repeat(32));

        let error = verify_authoritative_state_snapshot_v2(&compact)
            .expect_err("stale checkpoint state root must fail closed");
        assert!(error.to_string().contains("recorded state root"));
    }

    #[test]
    fn mixed_genesis_protocol_versions_fail_closed() {
        let mut state =
            crate::genesis_v2::init_chain_state_v2("pulsedag-private-v2.4.0".to_string()).unwrap();
        let genesis = state.dag.genesis_hash.clone();
        state.dag.blocks.get_mut(&genesis).unwrap().header.version = BLOCK_HEADER_VERSION_V1;

        let error = rebuild_authoritative_state_v2(&state)
            .expect_err("mixed genesis protocol versions must fail closed");
        assert!(error
            .to_string()
            .contains("mixed genesis protocol versions"));
    }

    #[test]
    fn conflicting_transaction_is_atomic_when_later_input_is_missing() {
        let rebuilt = rebuild_authoritative_state_v2(&conflict_state(false)).unwrap();
        let unspent = OutPoint {
            txid: "funding".into(),
            index: 0,
        };

        assert!(rebuilt.utxo.utxos.contains_key(&unspent));
        assert_eq!(rebuilt.diagnostics.applied_transactions, 2);
        assert_eq!(rebuilt.diagnostics.skipped_conflicting_transactions, 1);
        assert_eq!(rebuilt.diagnostics.conflict_diagnostics.len(), 1);
        assert!(rebuilt.diagnostics.conflict_diagnostics[0].contains("skipped_conflict_atomic"));
    }

    #[test]
    fn conflicting_input_order_cannot_change_rebuilt_state_root() {
        let forward = rebuild_authoritative_state_v2(&conflict_state(false)).unwrap();
        let reverse = rebuild_authoritative_state_v2(&conflict_state(true)).unwrap();
        let canonical_utxos = |replay: &StateReplayV2| {
            let mut entries = replay
                .utxo
                .utxos
                .values()
                .map(|utxo| {
                    (
                        utxo.outpoint.clone(),
                        utxo.address.clone(),
                        utxo.amount,
                        utxo.coinbase,
                        utxo.height,
                    )
                })
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            entries
        };

        assert_eq!(forward.ordered_dag, reverse.ordered_dag);
        assert_eq!(
            forward.diagnostics.state_root,
            reverse.diagnostics.state_root
        );
        assert_eq!(canonical_utxos(&forward), canonical_utxos(&reverse));
        assert_eq!(forward.utxo.address_index, reverse.utxo.address_index);
    }
}
