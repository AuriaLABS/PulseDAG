from pathlib import Path

p = Path('crates/pulsedag-wallet/src/bin/pulsedag-wallet.rs')
t = p.read_text()

old_helper = r'''fn ensure_sign_reservation_recovery(
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
            return Err(
                WalletPendingError::TransactionIdentityMismatch(final_txid.to_string()).into(),
            );
        }
    }
    Ok(())
}
'''

new_helper = r'''fn precheck_sign_reservation(
    journal: &WalletPendingJournal,
    from: &str,
    selected_utxos: &[pulsedag_wallet::SelectedUtxo],
) -> CliResult<Option<String>> {
    journal.validate()?;
    let selected_outpoints = selected_utxos
        .iter()
        .map(|selected| selected.outpoint.clone())
        .collect::<Vec<_>>();

    for selected in selected_utxos {
        let Some(existing) = journal.entries.iter().find(|entry| {
            entry.state.reserves_outpoints()
                && entry
                    .selected_outpoints
                    .iter()
                    .any(|reserved| reserved == &selected.outpoint)
        }) else {
            continue;
        };

        if existing.state == WalletPendingState::Signed
            && existing.from == from
            && existing.selected_outpoints == selected_outpoints
        {
            return Ok(Some(existing.final_txid.clone()));
        }

        return Err(WalletPendingError::ReservedOutpoint {
            txid: selected.outpoint.txid.clone(),
            index: selected.outpoint.index,
        }
        .into());
    }

    Ok(None)
}
'''

assert t.count(old_helper) == 1
t = t.replace(old_helper, new_helper)

old_run = r'''    let pending_store = WalletPendingJournalStore::try_acquire(&args.pending_journal)?;
    let mut snapshot = pending_store.load_or_new(&plan.network)?;
    let signed = session.sign_transaction_plan(
        &plan,
        WalletPlanSigner::DeterministicV2 {
            account: args.account,
            branch: args.branch,
            index: args.index,
        },
    )?;
    session.lock();
    let final_txid = signed.transaction.txid.clone();
    ensure_sign_reservation_recovery(
        &snapshot.journal,
        &final_txid,
        &plan.intent.from,
        &plan.selected_utxos,
    )?;
    if snapshot.journal.reserve_signed(
'''

new_run = r'''    let pending_store = WalletPendingJournalStore::try_acquire(&args.pending_journal)?;
    let mut snapshot = pending_store.load_or_new(&plan.network)?;
    let expected_recovery_txid = precheck_sign_reservation(
        &snapshot.journal,
        &plan.intent.from,
        &plan.selected_utxos,
    )?;
    let signed = session.sign_transaction_plan(
        &plan,
        WalletPlanSigner::DeterministicV2 {
            account: args.account,
            branch: args.branch,
            index: args.index,
        },
    )?;
    session.lock();
    let final_txid = signed.transaction.txid.clone();
    if let Some(expected_txid) = expected_recovery_txid {
        if final_txid != expected_txid {
            return Err(WalletPendingError::TransactionIdentityMismatch(expected_txid).into());
        }
    }
    if snapshot.journal.reserve_signed(
'''

assert t.count(old_run) == 1
t = t.replace(old_run, new_run)

old_test = r'''    #[test]
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

        assert!(
            ensure_sign_reservation_recovery(&journal, &original_txid, &from, &selected,).is_ok()
        );
        assert!(!journal
            .reserve_signed(&original_txid, from.clone(), &selected)
            .expect("exact recovery is idempotent"));

        let different_txid = "bb".repeat(32);
        let conflict =
            ensure_sign_reservation_recovery(&journal, &different_txid, &from, &selected)
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
        assert!(
            ensure_sign_reservation_recovery(&journal, &original_txid, &from, &selected,).is_err()
        );
    }
'''

new_test = r'''    #[test]
    fn tx_sign_precheck_allows_exact_signed_candidate_and_rejects_incompatible_reservation() {
        let network = WalletNetworkIdentity::new("public-testnet", "pulsedag-public-testnet")
            .expect("network");
        let from = pulsedag_core::address_from_public_key(&"ab".repeat(32));
        let other_from = pulsedag_core::address_from_public_key(&"cd".repeat(32));
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

        assert_eq!(
            precheck_sign_reservation(&journal, &from, &selected)
                .expect("compatible signed recovery candidate"),
            Some(original_txid.clone())
        );
        assert!(!journal
            .reserve_signed(&original_txid, from.clone(), &selected)
            .expect("exact recovery is idempotent"));

        let conflict = precheck_sign_reservation(&journal, &other_from, &selected)
            .expect_err("incompatible active reservation must fail before signing");
        let encoded = machine_readable_pending_error(conflict.as_ref())
            .expect("conflict remains machine-readable");
        let value: serde_json::Value = serde_json::from_str(&encoded).expect("valid JSON");
        assert_eq!(value["error"]["code"], "PENDING_UTXO_RESERVED");
        assert_eq!(value["error"]["txid"], selected[0].outpoint.txid);
        assert_eq!(value["error"]["index"], 3);

        journal
            .mark_submission_started(&original_txid)
            .expect("submission started");
        assert!(precheck_sign_reservation(&journal, &from, &selected).is_err());
    }
'''

assert t.count(old_test) == 1
t = t.replace(old_test, new_test)
p.write_text(t)
