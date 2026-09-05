from pathlib import Path

# P1: advertise and require a semantic capability that distinguishes the
# authoritative confirmation semantics from older explorer_api implementations.
release_path = Path('crates/pulsedag-rpc/src/handlers/release.rs')
release = release_path.read_text()

old = 'const SIGNED_TRANSACTION_RELAY_VERSION: &str = "signed-transaction-relay-v1";\n'
new = old + 'const AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY: &str = "authoritative_address_activity_v1";\n'
assert release.count(old) == 1
release = release.replace(old, new)

old = '        "explorer_api".into(),\n        "sync_diagnostics".into(),\n'
new = '        "explorer_api".into(),\n        AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY.into(),\n        "sync_diagnostics".into(),\n'
assert release.count(old) == 1
release = release.replace(old, new)

old = '    use super::{default_public_network_identity, operator_stage, repo_version};\n'
new = '''    use super::{\n        default_public_network_identity, operator_stage, release_capabilities, repo_version,\n        AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY,\n    };\n'''
assert release.count(old) == 1
release = release.replace(old, new)

old = '''        assert!(release.contains("\\\"/address/:address/activity\\\""));\n        assert!(release.contains("\\\"/api/v1/tx/submit\\\""));\n'''
new = '''        assert!(release.contains("\\\"/address/:address/activity\\\""));\n        assert!(release_capabilities()\n            .iter()\n            .any(|capability| capability == AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY));\n        assert!(release.contains("\\\"/api/v1/tx/submit\\\""));\n'''
assert release.count(old) == 1
release = release.replace(old, new)
release_path.write_text(release)

reconcile_path = Path('crates/pulsedag-wallet/src/bin/pulsedag-wallet-reconcile.rs')
reconcile = reconcile_path.read_text()

old = 'const EXPLORER_CAPABILITY: &str = "explorer_api";\nconst ACTIVITY_ENDPOINT: &str = "/address/:address/activity";\n'
new = 'const EXPLORER_CAPABILITY: &str = "explorer_api";\nconst AUTHORITATIVE_ACTIVITY_CAPABILITY: &str = "authoritative_address_activity_v1";\nconst ACTIVITY_ENDPOINT: &str = "/address/:address/activity";\n'
assert reconcile.count(old) == 1
reconcile = reconcile.replace(old, new)

old = '''    if !data\n        .core_endpoints\n        .iter()\n        .any(|endpoint| endpoint == ACTIVITY_ENDPOINT)\n    {\n'''
new = '''    if !data\n        .capabilities\n        .iter()\n        .any(|capability| capability == AUTHORITATIVE_ACTIVITY_CAPABILITY)\n    {\n        return Err(reconcile_error(format!(\n            "relay identity is missing required capability: {AUTHORITATIVE_ACTIVITY_CAPABILITY}"\n        )));\n    }\n    if !data\n        .core_endpoints\n        .iter()\n        .any(|endpoint| endpoint == ACTIVITY_ENDPOINT)\n    {\n'''
assert reconcile.count(old) == 1
reconcile = reconcile.replace(old, new)

# Every positive test identity must advertise both the generic explorer surface
# and the new semantic confirmation capability.
old = '                capabilities: vec![EXPLORER_CAPABILITY.to_string()],\n'
new = '''                capabilities: vec![\n                    EXPLORER_CAPABILITY.to_string(),\n                    AUTHORITATIVE_ACTIVITY_CAPABILITY.to_string(),\n                ],\n'''
assert reconcile.count(old) == 3
reconcile = reconcile.replace(old, new)

marker = '''        assert!(validate_release_identity(&network, valid).is_ok());\n\n        let wrong_network = ApiResponse {\n'''
insert = '''        assert!(validate_release_identity(&network, valid).is_ok());\n\n        let legacy_unversioned_activity = ApiResponse {\n            ok: true,\n            data: Some(ReleaseIdentityData {\n                network_profile: "testnet".to_string(),\n                chain_id: "pulsedag-testnet".to_string(),\n                capabilities: vec![EXPLORER_CAPABILITY.to_string()],\n                core_endpoints: vec![ACTIVITY_ENDPOINT.to_string()],\n            }),\n            error: None,\n        };\n        assert!(validate_release_identity(&network, legacy_unversioned_activity).is_err());\n\n        let wrong_network = ApiResponse {\n'''
assert reconcile.count(marker) == 1
reconcile = reconcile.replace(marker, insert)
reconcile_path.write_text(reconcile)

# P2: confirmed entries are terminal and no longer reserve inputs. Preserve the
# same-txid terminal check above, but once a genuinely new reservation is about
# to be appended, discard older confirmed records so this reservation journal
# cannot grow forever as transaction history.
pending_path = Path('crates/pulsedag-wallet/src/pending.rs')
pending = pending_path.read_text()

old = '''        self.entries.push(WalletPendingTransaction {\n            final_txid,\n            from,\n            selected_outpoints,\n            state: WalletPendingState::Signed,\n            rejection_code: None,\n            rejection_message: None,\n        });\n'''
new = '''        self.entries\n            .retain(|entry| entry.state != WalletPendingState::Confirmed);\n        self.entries.push(WalletPendingTransaction {\n            final_txid,\n            from,\n            selected_outpoints,\n            state: WalletPendingState::Signed,\n            rejection_code: None,\n            rejection_message: None,\n        });\n'''
assert pending.count(old) == 1
pending = pending.replace(old, new)

marker = '''    #[test]\n    fn submission_started_is_reserved_serialized_and_cannot_begin_twice() {\n'''
test = '''    #[test]\n    fn new_reservation_prunes_only_older_confirmed_records() {\n        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");\n        let confirmed_txid = final_txid("aa");\n        journal\n            .reserve_signed(&confirmed_txid, address(), &[selected("11", 0)])\n            .expect("reserve confirmed candidate");\n        journal\n            .mark_submission_started(&confirmed_txid)\n            .expect("submission started");\n        journal.mark_confirmed(&confirmed_txid).expect("confirmed");\n\n        let active_txid = final_txid("bb");\n        journal\n            .reserve_signed(&active_txid, address(), &[selected("22", 0)])\n            .expect("append new reservation");\n\n        assert!(journal.entry(&confirmed_txid).is_none());\n        assert_eq!(\n            journal.entry(&active_txid).expect("active entry").state,\n            WalletPendingState::Signed\n        );\n        assert_eq!(journal.entries.len(), 1);\n        assert_eq!(journal.reserved_outpoints().len(), 1);\n    }\n\n    #[test]\n    fn submission_started_is_reserved_serialized_and_cannot_begin_twice() {\n'''
assert pending.count(marker) == 1
pending = pending.replace(marker, test)
pending_path.write_text(pending)
