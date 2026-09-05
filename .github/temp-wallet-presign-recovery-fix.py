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

marker = r'''    #[test]
    fn machine_readable_pending_reserved_error_is_stable() {
'''
assert t.count(marker) == 1

tests = r'''    #[test]
    fn tx_sign_precheck_rejects_different_active_reservation_before_signing() {
        let network = WalletNetworkIdentity::new("public-testnet", "pulsedag-public-testnet")
            .expect("network");
        let mut journal = WalletPendingJournal::new(network).expect("journal");
        let selected = pulsedag_wallet::SelectedUtxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            amount: 10,
            address: "pdag1source".to_string(),
        };
        journal
            .reserve_signed(
                "22".repeat(32),
                "pdag1source",
                std::slice::from_ref(&selected),
            )
            .expect("reserve different tx");

        let error = precheck_sign_reservation(
            &journal,
            "pdag1source",
            std::slice::from_ref(&selected),
        )
        .expect_err("different active reservation must fail before signing");
        let pending = error
            .downcast_ref::<WalletPendingError>()
            .expect("pending error");
        assert!(matches!(
            pending,
            WalletPendingError::ReservedOutpoint { txid, index }
                if txid == &"11".repeat(32) && *index == 0
        ));
    }

    #[test]
    fn tx_sign_precheck_allows_only_exact_signed_recovery_candidate() {
        let network = WalletNetworkIdentity::new("public-testnet", "pulsedag-public-testnet")
            .expect("network");
        let mut journal = WalletPendingJournal::new(network).expect("journal");
        let selected = pulsedag_wallet::SelectedUtxo {
            outpoint: OutPoint {
                txid: "33".repeat(32),
                index: 1,
            },
            amount: 20,
            address: "pdag1source".to_string(),
        };
        let final_txid = "44".repeat(32);
        journal
            .reserve_signed(
                final_txid.clone(),
                "pdag1source",
                std::slice::from_ref(&selected),
            )
            .expect("reserve recovery tx");

        assert_eq!(
            precheck_sign_reservation(
                &journal,
                "pdag1source",
                std::slice::from_ref(&selected),
            )
            .expect("exact signed recovery candidate"),
            Some(final_txid)
        );

        journal
            .mark_submission_started(&"44".repeat(32))
            .expect("advance state");
        assert!(precheck_sign_reservation(
            &journal,
            "pdag1source",
            std::slice::from_ref(&selected),
        )
        .is_err());
    }

'''

t = t.replace(marker, tests + marker)
p.write_text(t)
