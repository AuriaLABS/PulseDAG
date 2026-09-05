from pathlib import Path


def patch_core_confirmation_set() -> None:
    path = Path("crates/pulsedag-core/src/validation.rs")
    text = path.read_text()

    anchor = "\n/// Return true only when this canonical transaction id is present in the\n"
    helper = r'''
fn ordered_replay_skipped_transactions(state: &ChainState) -> BTreeSet<(String, String)> {
    state
        .dag
        .ordered_dag_conflict_diagnostics
        .iter()
        .filter_map(|entry| {
            let (_, tail) = entry.split_once(" block=")?;
            let (block_hash, tail) = tail.split_once(" tx=")?;
            let (txid, _) = tail.split_once(" skipped_conflict")?;
            if block_hash.is_empty() || txid.is_empty() {
                None
            } else {
                Some((block_hash.to_string(), txid.to_string()))
            }
        })
        .collect()
}

/// Build the canonical set of transaction ids that were actually applied to
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
    assert text.count(anchor) == 1
    text = text.replace(anchor, "\n" + helper + anchor, 1)
    path.write_text(text)


def patch_rpc_activity() -> None:
    path = Path("crates/pulsedag-rpc/src/handlers/address.rs")
    text = path.read_text()

    old_import = "    validation::transaction_is_confirmed,\n"
    new_import = "    validation::authoritative_confirmed_transaction_ids,\n"
    assert text.count(old_import) == 1
    text = text.replace(old_import, new_import, 1)

    retained_anchor = "    let retained_outputs = retained_transaction_outputs(&chain);\n\n"
    retained_replacement = (
        "    let retained_outputs = retained_transaction_outputs(&chain);\n"
        "    let authoritative_confirmed_txids = authoritative_confirmed_transaction_ids(&chain);\n\n"
    )
    assert text.count(retained_anchor) == 1
    text = text.replace(retained_anchor, retained_replacement, 1)

    old_check = "                if !transaction_is_confirmed(&tx.txid, &chain) {\n"
    new_check = "                if !authoritative_confirmed_txids.contains(&tx.txid) {\n"
    assert text.count(old_check) == 1
    text = text.replace(old_check, new_check, 1)
    path.write_text(text)


def patch_pending_rejection_bounds() -> None:
    path = Path("crates/pulsedag-wallet/src/pending.rs")
    text = path.read_text()

    constants_anchor = (
        'pub const WALLET_PENDING_JOURNAL_FORMAT: &str = "pulsedag-wallet-pending-journal";\n'
        "pub const WALLET_PENDING_JOURNAL_VERSION: u32 = 1;\n"
    )
    constants_replacement = constants_anchor + (
        "const WALLET_PENDING_REJECTION_CODE_MAX_BYTES: usize = 128;\n"
        "const WALLET_PENDING_REJECTION_MESSAGE_MAX_BYTES: usize = 2048;\n"
    )
    assert text.count(constants_anchor) == 1
    text = text.replace(constants_anchor, constants_replacement, 1)

    old_validation = (
        '                validate_text("rejection_code", self.rejection_code.as_deref())?;\n'
        '                validate_text("rejection_message", self.rejection_message.as_deref())?;\n'
    )
    new_validation = (
        '                validate_text(\n'
        '                    "rejection_code",\n'
        '                    self.rejection_code.as_deref(),\n'
        '                    WALLET_PENDING_REJECTION_CODE_MAX_BYTES,\n'
        '                )?;\n'
        '                validate_text(\n'
        '                    "rejection_message",\n'
        '                    self.rejection_message.as_deref(),\n'
        '                    WALLET_PENDING_REJECTION_MESSAGE_MAX_BYTES,\n'
        '                )?;\n'
    )
    assert text.count(old_validation) == 1
    text = text.replace(old_validation, new_validation, 1)

    old_mark = (
        "        let code = code.into();\n"
        "        let message = message.into();\n"
        '        validate_text("rejection_code", Some(&code))?;\n'
        '        validate_text("rejection_message", Some(&message))?;\n'
    )
    new_mark = (
        "        let code = bound_rejection_text(\n"
        '            "rejection_code",\n'
        "            code.into(),\n"
        "            WALLET_PENDING_REJECTION_CODE_MAX_BYTES,\n"
        "        )?;\n"
        "        let message = bound_rejection_text(\n"
        '            "rejection_message",\n'
        "            message.into(),\n"
        "            WALLET_PENDING_REJECTION_MESSAGE_MAX_BYTES,\n"
        "        )?;\n"
    )
    assert text.count(old_mark) == 1
    text = text.replace(old_mark, new_mark, 1)

    old_text_validator = r'''fn validate_text(field: &'static str, value: Option<&str>) -> Result<(), WalletPendingError> {
    let value = value.ok_or(WalletPendingError::InvalidField {
        field,
        reason: "must be present",
    })?;
    if value.is_empty() || value.trim() != value {
        return Err(WalletPendingError::InvalidField {
            field,
            reason: "must be non-empty without leading or trailing whitespace",
        });
    }
    Ok(())
}
'''
    new_text_validator = r'''fn validate_text_shape(field: &'static str, value: Option<&str>) -> Result<(), WalletPendingError> {
    let value = value.ok_or(WalletPendingError::InvalidField {
        field,
        reason: "must be present",
    })?;
    if value.is_empty() || value.trim() != value {
        return Err(WalletPendingError::InvalidField {
            field,
            reason: "must be non-empty without leading or trailing whitespace",
        });
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), WalletPendingError> {
    validate_text_shape(field, value)?;
    if value.is_some_and(|text| text.len() > max_bytes) {
        return Err(WalletPendingError::InvalidField {
            field,
            reason: "exceeds persisted byte limit",
        });
    }
    Ok(())
}

fn bound_rejection_text(
    field: &'static str,
    mut value: String,
    max_bytes: usize,
) -> Result<String, WalletPendingError> {
    validate_text_shape(field, Some(&value))?;
    if value.len() > max_bytes {
        let mut end = max_bytes;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
        let trimmed_len = value.trim_end().len();
        value.truncate(trimmed_len);
    }
    validate_text(field, Some(&value), max_bytes)?;
    Ok(value)
}
'''
    assert text.count(old_text_validator) == 1
    text = text.replace(old_text_validator, new_text_validator, 1)

    tests = r'''

    #[test]
    fn relay_rejection_metadata_is_bounded_before_persistence() {
        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");
        let selected = [selected("11", 0)];
        let txid = final_txid("aa");
        journal
            .reserve_signed(&txid, address(), &selected)
            .expect("reserve");
        journal
            .mark_submission_started(&txid)
            .expect("submission started");

        let long_code = format!(
            "CODE-{}",
            "X".repeat(WALLET_PENDING_REJECTION_CODE_MAX_BYTES * 4)
        );
        let long_message = format!(
            "relay rejected {}",
            "界".repeat(WALLET_PENDING_REJECTION_MESSAGE_MAX_BYTES)
        );
        journal
            .mark_relay_rejected(&txid, long_code.clone(), long_message.clone())
            .expect("bounded rejection observation");

        let entry = journal.entry(&txid).expect("entry");
        let code = entry.rejection_code.as_deref().expect("code");
        let message = entry.rejection_message.as_deref().expect("message");
        assert!(code.len() <= WALLET_PENDING_REJECTION_CODE_MAX_BYTES);
        assert!(message.len() <= WALLET_PENDING_REJECTION_MESSAGE_MAX_BYTES);
        assert!(long_code.starts_with(code));
        assert!(long_message.starts_with(message));
        assert_eq!(entry.state, WalletPendingState::RelayRejected);
        assert_eq!(journal.reserved_outpoints().len(), 1);
        journal.validate().expect("bounded journal validates");

        let mut oversized = journal.clone();
        oversized
            .entries
            .iter_mut()
            .find(|entry| entry.final_txid == txid)
            .expect("oversized entry")
            .rejection_message = Some("Z".repeat(WALLET_PENDING_REJECTION_MESSAGE_MAX_BYTES + 1));
        assert!(matches!(
            oversized.validate(),
            Err(WalletPendingError::InvalidField {
                field: "rejection_message",
                reason: "exceeds persisted byte limit"
            })
        ));
    }
'''
    end = text.rfind("\n}")
    assert end > 0
    text = text[:end] + tests + text[end:]
    path.write_text(text)


patch_core_confirmation_set()
patch_rpc_activity()
patch_pending_rejection_bounds()
