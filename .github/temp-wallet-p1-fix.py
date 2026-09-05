from pathlib import Path


def patch_rpc() -> None:
    path = Path("crates/pulsedag-rpc/src/handlers/address.rs")
    text = path.read_text()

    old_import = "use pulsedag_core::types::{OutPoint, Utxo};\n"
    new_import = (
        "use pulsedag_core::{types::{OutPoint, Utxo}, "
        "validation::transaction_is_confirmed};\n"
    )
    assert text.count(old_import) == 1
    text = text.replace(old_import, new_import)

    dag_activity_branch = (
        "            if incoming > 0 || outgoing > 0 {\n"
        "                let net = incoming as i64 - outgoing as i64;\n"
    )
    assert text.count(dag_activity_branch) == 1
    confirmed_branch = (
        "            if incoming > 0 || outgoing > 0 {\n"
        "                if !transaction_is_confirmed(&tx.txid, &chain) {\n"
        "                    continue;\n"
        "                }\n"
        "                let net = incoming as i64 - outgoing as i64;\n"
    )
    text = text.replace(dag_activity_branch, confirmed_branch, 1)

    tests = r'''

    fn record_noncanonical_activity_tx(
        chain: &mut ChainState,
        tx: Transaction,
        block_hash: &str,
    ) {
        let genesis = chain.dag.genesis_hash.clone();
        let mut block = chain
            .dag
            .blocks
            .get(&genesis)
            .expect("genesis block")
            .clone();
        block.hash = block_hash.to_string();
        block.header.parents = vec![genesis];
        block.header.height = 1;
        block.header.timestamp = 1;
        block.transactions = vec![tx];
        chain.dag.blocks.insert(block_hash.to_string(), block);
    }

    fn retained_activity_tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: 1,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                address: "alice".into(),
                amount: 7,
            }],
            fee: 0,
            nonce: 9,
        }
    }

    #[tokio::test]
    async fn side_dag_activity_is_not_reported_as_confirmed() {
        let state = mk_state().await;
        {
            let mut chain = state.chain.write().await;
            chain.mempool.transactions.clear();
            let txid = "side-dag-wallet-activity";
            record_noncanonical_activity_tx(
                &mut chain,
                retained_activity_tx(txid),
                "side-dag-block",
            );
            assert!(!pulsedag_core::validation::transaction_is_confirmed(txid, &chain));
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
        assert!(data
            .activity
            .iter()
            .all(|item| item.txid != "side-dag-wallet-activity"));
    }

    #[tokio::test]
    async fn replay_conflict_loser_activity_is_not_reported_as_confirmed() {
        let state = mk_state().await;
        {
            let mut chain = state.chain.write().await;
            chain.mempool.transactions.clear();
            chain.dag.consensus_mode = pulsedag_core::state::ConsensusMode::GhostdagDev;
            let txid = "replay-loser-wallet-activity";
            let block_hash = "replay-loser-wallet-block";
            record_noncanonical_activity_tx(
                &mut chain,
                retained_activity_tx(txid),
                block_hash,
            );
            chain.dag.ordered_dag.push(block_hash.to_string());
            chain.dag.ordered_dag_conflict_diagnostics.push(format!(
                "ordered_pos=1 block={block_hash} tx={txid} skipped_conflict"
            ));
            assert!(!pulsedag_core::validation::transaction_is_confirmed(txid, &chain));
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
        assert!(data
            .activity
            .iter()
            .all(|item| item.txid != "replay-loser-wallet-activity"));
    }
'''
    end = text.rfind("\n}")
    assert end > 0
    text = text[:end] + tests + text[end:]
    path.write_text(text)


def patch_wallet() -> None:
    path = Path("crates/pulsedag-wallet/src/bin/pulsedag-wallet.rs")
    text = path.read_text()

    anchor = "fn run_tx_sign(args: TxSignArgs, password: &SecretString) -> CliResult<TxSignOutput> {\n"
    helper = r'''fn ensure_sign_reservation_recovery(
    journal: &WalletPendingJournal,
    final_txid: &str,
    from: &str,
    selected_utxos: &[pulsedag_wallet::SelectedUtxo],
) -> CliResult<()> {
    journal.validate()?;
    let selected_outpoints = selected_utxos
        .iter()
        .map(|selected| selected.outpoint.clone())
        .collect::<Vec<_>>();

    for selected in selected_utxos {
        if journal.entries.iter().any(|entry| {
            entry.final_txid != final_txid
                && entry.state.reserves_outpoints()
                && entry
                    .selected_outpoints
                    .iter()
                    .any(|reserved| reserved == &selected.outpoint)
        }) {
            return Err(WalletPendingError::ReservedOutpoint {
                txid: selected.outpoint.txid.clone(),
                index: selected.outpoint.index,
            }
            .into());
        }
    }

    if let Some(existing) = journal.entry(final_txid) {
        if existing.state != WalletPendingState::Signed {
            return Err(invalid_input(format!(
                "pending transaction cannot be recovered from state: {}",
                existing.state.as_str()
            ))
            .into());
        }
        if existing.from != from || existing.selected_outpoints != selected_outpoints {
            return Err(WalletPendingError::TransactionIdentityMismatch(final_txid.to_string()).into());
        }
    }
    Ok(())
}

'''
    assert text.count(anchor) == 1
    text = text.replace(anchor, helper + anchor)

    old_precheck = (
        "    let mut snapshot = pending_store.load_or_new(&plan.network)?;\n"
        "    snapshot\n"
        "        .journal\n"
        "        .ensure_selected_unreserved(&plan.selected_utxos)?;\n"
        "    let signed = session.sign_transaction_plan(\n"
    )
    new_precheck = (
        "    let mut snapshot = pending_store.load_or_new(&plan.network)?;\n"
        "    let signed = session.sign_transaction_plan(\n"
    )
    assert text.count(old_precheck) == 1
    text = text.replace(old_precheck, new_precheck)

    old_after_sign = (
        "    session.lock();\n"
        "    let final_txid = signed.transaction.txid.clone();\n"
        "    if snapshot.journal.reserve_signed(\n"
    )
    new_after_sign = (
        "    session.lock();\n"
        "    let final_txid = signed.transaction.txid.clone();\n"
        "    ensure_sign_reservation_recovery(\n"
        "        &snapshot.journal,\n"
        "        &final_txid,\n"
        "        &plan.intent.from,\n"
        "        &plan.selected_utxos,\n"
        "    )?;\n"
        "    if snapshot.journal.reserve_signed(\n"
    )
    assert text.count(old_after_sign) == 1
    text = text.replace(old_after_sign, new_after_sign)

    tests = r'''

    #[test]
    fn tx_sign_recovery_allows_exact_signed_record_and_rejects_other_active_tx() {
        let network = WalletNetworkIdentity::new("public-testnet", "pulsedag-public-testnet")
            .expect("network");
        let from = pulsedag_core::address_from_public_key(&"ab".repeat(32));
        let selected = [pulsedag_wallet::SelectedUtxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 3,
            },
            amount: 100,
        }];
        let original_txid = "aa".repeat(32);
        let mut journal = WalletPendingJournal::new(network).expect("journal");
        assert!(journal
            .reserve_signed(&original_txid, from.clone(), &selected)
            .expect("first reservation"));

        assert!(ensure_sign_reservation_recovery(
            &journal,
            &original_txid,
            &from,
            &selected,
        )
        .is_ok());
        assert!(!journal
            .reserve_signed(&original_txid, from.clone(), &selected)
            .expect("exact recovery is idempotent"));

        let different_txid = "bb".repeat(32);
        let conflict = ensure_sign_reservation_recovery(
            &journal,
            &different_txid,
            &from,
            &selected,
        )
        .expect_err("different tx must conflict");
        let encoded = machine_readable_pending_error(conflict.as_ref())
            .expect("conflict remains machine-readable");
        let value: serde_json::Value = serde_json::from_str(&encoded).expect("valid JSON");
        assert_eq!(value["error"]["code"], "PENDING_UTXO_RESERVED");
        assert_eq!(value["error"]["txid"], selected[0].outpoint.txid);
        assert_eq!(value["error"]["index"], 3);

        journal
            .mark_submission_started(&original_txid)
            .expect("submission started");
        assert!(ensure_sign_reservation_recovery(
            &journal,
            &original_txid,
            &from,
            &selected,
        )
        .is_err());
    }
'''
    end = text.rfind("\n}")
    assert end > 0
    text = text[:end] + tests + text[end:]
    path.write_text(text)


patch_rpc()
patch_wallet()
