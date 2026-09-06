from pathlib import Path


def patch_core() -> None:
    path = Path("crates/pulsedag-core/src/validation.rs")
    text = path.read_text()

    old = r'''/// Build the canonical set of transaction ids that were actually applied to
/// authoritative state. This is the bulk counterpart to
/// `transaction_is_confirmed` for read paths that must classify many retained
/// transactions without rescanning the selected/ordered chain for each txid.
pub fn authoritative_confirmed_transaction_ids(state: &ChainState) -> BTreeSet<String> {
    let ordered_replay = state.dag.consensus_mode.ghostdag_metadata_active()
        || state.dag.ordering_version == crate::ordering_v2::GHOSTDAG_V1_ORDERING_VERSION;
    let canonical_order = if ordered_replay {
        &state.dag.ordered_dag
    } else {
        &state.dag.selected_chain
    };
    let skipped = if ordered_replay {
        ordered_replay_skipped_transactions(state)
    } else {
        BTreeSet::new()
    };

    let mut confirmed = BTreeSet::new();
    for block in canonical_order
        .iter()
        .filter_map(|hash| state.dag.blocks.get(hash))
    {
        for tx in &block.transactions {
            if ordered_replay && skipped.contains(&(block.hash.clone(), tx.txid.clone())) {
                continue;
            }
            confirmed.insert(tx.txid.clone());
        }
    }
    confirmed
}
'''
    new = r'''/// Build the canonical set of exact retained transaction occurrences that
/// were actually applied to authoritative state. This is the bulk counterpart
/// to `transaction_is_confirmed` for read paths that must classify many
/// retained transactions without rescanning the selected/ordered chain for
/// each txid. Binding the block hash prevents a replay-skipped duplicate txid
/// in another retained DAG block from inheriting confirmation from the applied
/// occurrence.
pub fn authoritative_confirmed_transaction_occurrences(
    state: &ChainState,
) -> BTreeSet<(String, String)> {
    let ordered_replay = state.dag.consensus_mode.ghostdag_metadata_active()
        || state.dag.ordering_version == crate::ordering_v2::GHOSTDAG_V1_ORDERING_VERSION;
    let canonical_order = if ordered_replay {
        &state.dag.ordered_dag
    } else {
        &state.dag.selected_chain
    };
    let skipped = if ordered_replay {
        ordered_replay_skipped_transactions(state)
    } else {
        BTreeSet::new()
    };

    let mut confirmed = BTreeSet::new();
    for block in canonical_order
        .iter()
        .filter_map(|hash| state.dag.blocks.get(hash))
    {
        for tx in &block.transactions {
            let occurrence = (block.hash.clone(), tx.txid.clone());
            if ordered_replay && skipped.contains(&occurrence) {
                continue;
            }
            confirmed.insert(occurrence);
        }
    }
    confirmed
}
'''
    assert text.count(old) == 1
    text = text.replace(old, new, 1)
    path.write_text(text)


def patch_rpc() -> None:
    path = Path("crates/pulsedag-rpc/src/handlers/address.rs")
    text = path.read_text()

    old_import = "    validation::authoritative_confirmed_transaction_ids,\n"
    new_import = "    validation::authoritative_confirmed_transaction_occurrences,\n"
    assert text.count(old_import) == 1
    text = text.replace(old_import, new_import, 1)

    old_set = "    let authoritative_confirmed_txids = authoritative_confirmed_transaction_ids(&chain);\n"
    new_set = (
        "    let authoritative_confirmed_occurrences =\n"
        "        authoritative_confirmed_transaction_occurrences(&chain);\n"
    )
    assert text.count(old_set) == 1
    text = text.replace(old_set, new_set, 1)

    old_check = "                if !authoritative_confirmed_txids.contains(&tx.txid) {\n"
    new_check = (
        "                if !authoritative_confirmed_occurrences\n"
        "                    .contains(&(block.hash.clone(), tx.txid.clone()))\n"
        "                {\n"
    )
    assert text.count(old_check) == 1
    text = text.replace(old_check, new_check, 1)

    tests = r'''

    #[tokio::test]
    async fn duplicate_retained_txid_reports_only_applied_block_occurrence() {
        let state = mk_state().await;
        let txid = "duplicate-retained-wallet-activity";
        let applied_block = "duplicate-applied-block";
        let skipped_block = "duplicate-skipped-block";
        {
            let mut chain = state.chain.write().await;
            chain.mempool.transactions.clear();
            chain.dag.consensus_mode = pulsedag_core::state::ConsensusMode::GhostdagDev;
            let tx = retained_activity_tx(txid);
            record_noncanonical_activity_tx(&mut chain, tx.clone(), applied_block);
            record_noncanonical_activity_tx(&mut chain, tx, skipped_block);
            chain.dag.ordered_dag.push(applied_block.to_string());
            chain.dag.ordered_dag.push(skipped_block.to_string());
            chain.dag.ordered_dag_conflict_diagnostics.push(format!(
                "ordered_pos=2 block={skipped_block} tx={txid} skipped_conflict"
            ));
            assert!(pulsedag_core::validation::transaction_is_confirmed(
                txid, &chain
            ));
            let occurrences =
                pulsedag_core::validation::authoritative_confirmed_transaction_occurrences(&chain);
            assert!(occurrences.contains(&(applied_block.to_string(), txid.to_string())));
            assert!(!occurrences.contains(&(skipped_block.to_string(), txid.to_string())));
        }

        let axum::Json(resp) = get_address_activity(
            State(state),
            Path("alice".to_string()),
            Query(super::AddressActivityQuery {
                limit: Some(10),
                offset: Some(0),
            }),
        )
        .await;
        let data = resp.data.expect("activity");
        let matches = data
            .activity
            .iter()
            .filter(|item| item.txid == txid)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].block_hash.as_deref(), Some(applied_block));
        assert!(matches[0].is_confirmed);
    }
'''
    end = text.rfind("\n}")
    assert end > 0
    text = text[:end] + tests + text[end:]
    path.write_text(text)


patch_core()
patch_rpc()
