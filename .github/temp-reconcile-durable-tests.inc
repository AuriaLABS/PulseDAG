    fn durable_test_dir(label: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pulsedag-wallet-reconcile-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn durable_not_observed_survives_restart_and_retains_reservation() {
        let path = durable_test_dir("not-observed");
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let txid = "cc".repeat(32);

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
            let mut snapshot = store.load_or_new(&network).expect("load");
            snapshot
                .journal
                .reserve_signed(&txid, from_address(), &selected())
                .expect("reserve");
            snapshot
                .journal
                .mark_submission_started(&txid)
                .expect("started");
            snapshot
                .journal
                .mark_submission_outcome_unknown(&txid)
                .expect("unknown");
            store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save unknown");
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("reopen");
            let mut snapshot = store.load_or_new(&network).expect("reload");
            assert!(!apply_observation(
                &mut snapshot.journal,
                &txid,
                ReconcileObservation::NotObserved,
            )
            .expect("absence is a no-op"));
            let entry = snapshot.journal.entry(&txid).expect("entry");
            assert_eq!(
                entry.state,
                WalletPendingState::SubmissionOutcomeUnknown
            );
            assert!(entry.state.reserves_outpoints());
            assert_eq!(snapshot.generation, 1);
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("second reopen");
            let snapshot = store.load_or_new(&network).expect("second reload");
            let entry = snapshot.journal.entry(&txid).expect("entry");
            assert_eq!(
                entry.state,
                WalletPendingState::SubmissionOutcomeUnknown
            );
            assert!(entry.state.reserves_outpoints());
            assert_eq!(snapshot.generation, 1);
        }

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn durable_confirmation_releases_after_restart_and_cross_network_fails_closed() {
        let path = durable_test_dir("confirmed");
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let wrong_network = WalletNetworkIdentity::new("testnet", "other-chain").unwrap();
        let txid = "dd".repeat(32);

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
            let mut snapshot = store.load_or_new(&network).expect("load");
            snapshot
                .journal
                .reserve_signed(&txid, from_address(), &selected())
                .expect("reserve");
            store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save signed");
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("reopen");
            assert!(store.load_or_new(&wrong_network).is_err());
            let mut snapshot = store.load_or_new(&network).expect("reload");
            assert!(apply_observation(
                &mut snapshot.journal,
                &txid,
                ReconcileObservation::Confirmed,
            )
            .expect("confirm"));
            store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save confirmed");
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("second reopen");
            let mut snapshot = store.load_or_new(&network).expect("second reload");
            let entry = snapshot.journal.entry(&txid).expect("entry");
            assert_eq!(entry.state, WalletPendingState::Confirmed);
            assert!(!entry.state.reserves_outpoints());
            assert_eq!(snapshot.generation, 2);
            assert!(!apply_observation(
                &mut snapshot.journal,
                &txid,
                ReconcileObservation::Confirmed,
            )
            .expect("repeat confirmation"));
        }

        let _ = std::fs::remove_dir_all(path);
    }

