from pathlib import Path


def replace_exact(text: str, old: str, new: str, count: int = 1) -> str:
    actual = text.count(old)
    assert actual == count, f"expected {count} occurrences, found {actual}: {old!r}"
    return text.replace(old, new, count)


def patch_release() -> None:
    path = Path("crates/pulsedag-rpc/src/handlers/release.rs")
    text = path.read_text()
    text = replace_exact(
        text,
        'const AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY: &str = "authoritative_address_activity_v1";\n',
        'const LEGACY_AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY: &str =\n    "authoritative_address_activity_v1";\nconst AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY: &str = "authoritative_address_activity_v2";\n',
    )
    text = replace_exact(
        text,
        '        "explorer_api".into(),\n        AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY.into(),\n',
        '        "explorer_api".into(),\n        LEGACY_AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY.into(),\n        AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY.into(),\n',
    )
    text = replace_exact(
        text,
        '        AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY,\n',
        '        AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY,\n        LEGACY_AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY,\n',
    )
    old_assert = '''        assert!(release_capabilities()\n            .iter()\n            .any(|capability| capability == AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY));\n'''
    new_assert = '''        assert!(release_capabilities()\n            .iter()\n            .any(|capability| capability == LEGACY_AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY));\n        assert!(release_capabilities()\n            .iter()\n            .any(|capability| capability == AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY));\n        assert_eq!(\n            AUTHORITATIVE_ADDRESS_ACTIVITY_CAPABILITY,\n            "authoritative_address_activity_v2"\n        );\n'''
    text = replace_exact(text, old_assert, new_assert)
    path.write_text(text)


def patch_reconcile() -> None:
    path = Path("crates/pulsedag-wallet/src/bin/pulsedag-wallet-reconcile.rs")
    text = path.read_text()
    text = replace_exact(
        text,
        'const AUTHORITATIVE_ACTIVITY_CAPABILITY: &str = "authoritative_address_activity_v1";\n',
        'const AUTHORITATIVE_ACTIVITY_CAPABILITY: &str = "authoritative_address_activity_v2";\n',
    )
    anchor = '''        assert!(validate_release_identity(&network, legacy_unversioned_activity).is_err());\n\n        let wrong_network = ApiResponse {\n'''
    replacement = '''        assert!(validate_release_identity(&network, legacy_unversioned_activity).is_err());\n\n        let legacy_v1_activity = ApiResponse {\n            ok: true,\n            data: Some(ReleaseIdentityData {\n                network_profile: "testnet".to_string(),\n                chain_id: "pulsedag-testnet".to_string(),\n                capabilities: vec![\n                    EXPLORER_CAPABILITY.to_string(),\n                    "authoritative_address_activity_v1".to_string(),\n                ],\n                core_endpoints: vec![ACTIVITY_ENDPOINT.to_string()],\n            }),\n            error: None,\n        };\n        let error = validate_release_identity(&network, legacy_v1_activity)\n            .expect_err("v1 activity semantics must fail closed");\n        assert!(error.to_string().contains("authoritative_address_activity_v2"));\n\n        let wrong_network = ApiResponse {\n'''
    text = replace_exact(text, anchor, replacement)
    path.write_text(text)


def patch_readme() -> None:
    path = Path("crates/pulsedag-wallet/README.md")
    text = path.read_text()
    old = '''- `pulsedag-wallet-reconcile --pending-journal <dir> --txid <final-txid> --relay <origin> [--page-size <1..100>] [--max-pages <1..10>]` loads the exact pending transaction and its stored sender/network identity, releases the journal lock while doing bounded public HTTP reads, verifies `/release` network identity plus `explorer_api`, the semantic `authoritative_address_activity_v1` capability, and `/address/:address/activity`, then scans retained activity from newest to older pages within the explicit budget. A relay that exposes only the older generic explorer surface fails closed before its activity can be used as confirmation evidence. Positive exact-txid evidence may promote state to `observed_mempool` or `confirmed`; `confirmed` is the only automatic state here that releases selected-outpoint reservations. `not_observed`, retained-history exhaustion, and page-budget exhaustion are reported but do not change state or release reservations. The journal is reacquired and revalidated before any positive evidence is persisted, making retry/restart reconciliation idempotent and avoiding a journal lock across network I/O.\n'''
    new = '''- `pulsedag-wallet-reconcile --pending-journal <dir> --txid <final-txid> --relay <origin> [--page-size <1..100>] [--max-pages <1..10>]` loads the exact pending transaction and its stored sender/network identity, releases the journal lock while doing bounded public HTTP reads, verifies `/release` network identity plus `explorer_api`, the exact-occurrence semantic `authoritative_address_activity_v2` capability, and `/address/:address/activity`, then scans retained activity from newest to older pages within the explicit budget. A relay that exposes only the generic explorer surface or the older `authoritative_address_activity_v1` txid-level semantic fails closed before its activity can be used as confirmation evidence. New relays may still advertise v1 for backward-compatible read clients, but pending reconciliation requires v2 because only v2 binds confirmation to the applied `(block_hash, txid)` occurrence. Positive exact-txid evidence may promote state to `observed_mempool` or `confirmed`; `confirmed` is the only automatic state here that releases selected-outpoint reservations. `not_observed`, retained-history exhaustion, and page-budget exhaustion are reported but do not change state or release reservations. The journal is reacquired and revalidated before any positive evidence is persisted, making retry/restart reconciliation idempotent and avoiding a journal lock across network I/O.\n'''
    text = replace_exact(text, old, new)
    path.write_text(text)


def patch_docs() -> None:
    path = Path("docs/WALLET_PENDING_RECONCILIATION.md")
    text = path.read_text()
    text = replace_exact(
        text,
        '- requires `explorer_api`, the semantic `authoritative_address_activity_v1` capability, and the canonical `/address/:address/activity` endpoint;\n- fails closed before using activity evidence if a relay exposes only the older generic `explorer_api` surface without `authoritative_address_activity_v1`;\n',
        '- requires `explorer_api`, the exact-occurrence semantic `authoritative_address_activity_v2` capability, and the canonical `/address/:address/activity` endpoint;\n- fails closed before using activity evidence if a relay exposes only generic `explorer_api` or the older txid-level `authoritative_address_activity_v1` capability without `authoritative_address_activity_v2`;\n',
    )
    old_para = '''The public address-activity surface only labels a retained DAG transaction occurrence `confirmed` when that exact `(block_hash, txid)` occurrence is in the authoritative selected/ordered state and was actually applied. For read paths that classify many retained transactions, core builds the authoritative applied-occurrence set once per request via `authoritative_confirmed_transaction_occurrences`, rather than rescanning the selected/ordered chain for every matching retained transaction. Binding confirmation to the block hash as well as the txid prevents a replay-skipped duplicate txid in a parallel retained block from inheriting confirmation from the applied occurrence. The bulk occurrence set preserves the authoritative semantics of `transaction_is_confirmed`, including exclusion of side-DAG-only transactions and ordered-replay conflict losers, while identifying the exact canonical occurrence reported by address activity. The explicit `authoritative_address_activity_v1` capability binds reconcile to this semantic contract during mixed-version rollout; the generic explorer capability alone is insufficient release evidence.\n'''
    new_para = '''The public address-activity surface only labels a retained DAG transaction occurrence `confirmed` when that exact `(block_hash, txid)` occurrence is in the authoritative selected/ordered state and was actually applied. For read paths that classify many retained transactions, core builds the authoritative applied-occurrence set once per request via `authoritative_confirmed_transaction_occurrences`, rather than rescanning the selected/ordered chain for every matching retained transaction. Binding confirmation to the block hash as well as the txid prevents a replay-skipped duplicate txid in a parallel retained block from inheriting confirmation from the applied occurrence. The bulk occurrence set preserves the authoritative semantics of `transaction_is_confirmed`, including exclusion of side-DAG-only transactions and ordered-replay conflict losers, while identifying the exact canonical occurrence reported by address activity. `authoritative_address_activity_v1` predates exact-occurrence binding and is therefore insufficient release evidence for this wallet flow. The new `authoritative_address_activity_v2` capability explicitly binds reconcile to the `(block_hash, txid)` semantic during mixed-version rollout; a v1-only relay fails closed. A v2 relay may also advertise v1 for backward-compatible read clients, but reconcile requires v2.\n'''
    text = replace_exact(text, old_para, new_para)
    text = replace_exact(
        text,
        'rejection of legacy unversioned activity as authoritative confirmation evidence, bounded UTF-8-safe relay rejection metadata',
        'rejection of generic-only and v1-only legacy activity as authoritative confirmation evidence, bounded UTF-8-safe relay rejection metadata',
    )
    path.write_text(text)


patch_release()
patch_reconcile()
patch_readme()
patch_docs()
