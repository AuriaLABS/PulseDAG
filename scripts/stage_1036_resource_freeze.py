from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"guard mismatch {path}: expected 1, got {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1))


# Core exports.
replace_once(
    "crates/pulsedag-core/src/lib.rs",
    "pub mod mempool_replacement_v3;\npub mod mempool_v3;",
    "pub mod mempool_replacement_v3;\npub mod mempool_resource_v1;\npub mod mempool_v3;",
)
replace_once(
    "crates/pulsedag-core/src/lib.rs",
    "pub use mempool::{\n    canonical_mempool_txids, combined_pressure_tier, mempool_pressure_bps, pressure_tier_from_bps,\n    reconcile_mempool, MempoolPressureTier, MempoolReconcileResult,\n};",
    "pub use mempool::{\n    canonical_mempool_txids, combined_pressure_tier, mempool_pressure_bps, pressure_tier_from_bps,\n    prune_expired_mempool, reconcile_mempool, MempoolExpiryResult, MempoolPressureTier,\n    MempoolReconcileResult,\n};\npub use mempool_resource_v1::{\n    assess_production_transaction_resources_v1, canonical_resource_survivors_v1,\n    mempool_resource_rejection_code_from_reason_v1, mempool_resource_rejection_detail_v1,\n    mempool_resource_rejection_reason_v1, normalize_production_mempool_resources_v1,\n    MempoolResourceAssessmentErrorV1, MempoolResourceNormalizationV1, MempoolResourcePolicyV1,\n    MEMPOOL_RESOURCE_POLICY_V1_VERSION, MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE,\n    MEMPOOL_RESOURCE_V1_MAX_AGE_BLOCKS, MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES,\n    MEMPOOL_RESOURCE_V1_MAX_ORPHANS, MEMPOOL_RESOURCE_V1_MAX_SPENT_OUTPOINTS,\n    MEMPOOL_RESOURCE_V1_MAX_TRANSACTIONS,\n};",
)

# Ordinary reconciliation: production resource normalization is the single
# activation point used by startup and every legacy block-advance path.
replace_once(
    "crates/pulsedag-core/src/mempool.rs",
    "    errors::PulseError,\n    state::ChainState,",
    "    errors::PulseError,\n    mempool_resource_v1::normalize_production_mempool_resources_v1,\n    state::ChainState,",
)
replace_once(
    "crates/pulsedag-core/src/mempool.rs",
    "pub fn reconcile_mempool(state: &mut ChainState) -> MempoolReconcileResult {\n    let tx_count = state.mempool.transactions.len();",
    "pub fn reconcile_mempool(state: &mut ChainState) -> MempoolReconcileResult {\n    let mut resource_removed =\n        normalize_production_mempool_resources_v1(state).removed_live_txids;\n    let tx_count = state.mempool.transactions.len();",
)
replace_once(
    "crates/pulsedag-core/src/mempool.rs",
    "        return MempoolReconcileResult {\n            removed_txids: Vec::new(),\n            kept_txids: Vec::new(),\n        };",
    "        state.mempool.counters.reconcile_removed_total = state\n            .mempool\n            .counters\n            .reconcile_removed_total\n            .saturating_add(resource_removed.len() as u64);\n        return MempoolReconcileResult {\n            removed_txids: resource_removed,\n            kept_txids: Vec::new(),\n        };",
)
replace_once(
    "crates/pulsedag-core/src/mempool.rs",
    "    rebuilt_mempool.counters.reconcile_removed_total = rebuilt_mempool\n        .counters\n        .reconcile_removed_total\n        .saturating_add(removed_txids.len() as u64);\n    state.mempool = rebuilt_mempool;\n\n    MempoolReconcileResult {\n        removed_txids,\n        kept_txids,\n    }",
    "    resource_removed.append(&mut removed_txids);\n    resource_removed.sort();\n    resource_removed.dedup();\n    rebuilt_mempool.counters.reconcile_removed_total = rebuilt_mempool\n        .counters\n        .reconcile_removed_total\n        .saturating_add(resource_removed.len() as u64);\n    state.mempool = rebuilt_mempool;\n\n    MempoolReconcileResult {\n        removed_txids: resource_removed,\n        kept_txids,\n    }",
)

# Protocol-aware reconciliation preserves its fail-closed identity-before-
# mutation invariant, then applies the identical resource normalization.
replace_once(
    "crates/pulsedag-core/src/mempool_protocol.rs",
    "    mempool::MempoolReconcileResult,\n    protocol::ProtocolActivationIdentity,",
    "    mempool::MempoolReconcileResult,\n    mempool_resource_v1::normalize_production_mempool_resources_v1,\n    protocol::ProtocolActivationIdentity,",
)
replace_once(
    "crates/pulsedag-core/src/mempool_protocol.rs",
    "    resolve_transaction_validation_path(identity, state)?;\n\n    let tx_count = state.mempool.transactions.len();",
    "    resolve_transaction_validation_path(identity, state)?;\n\n    let mut resource_removed =\n        normalize_production_mempool_resources_v1(state).removed_live_txids;\n    let tx_count = state.mempool.transactions.len();",
)
replace_once(
    "crates/pulsedag-core/src/mempool_protocol.rs",
    "        return Ok(MempoolReconcileResult {\n            removed_txids: Vec::new(),\n            kept_txids: Vec::new(),\n        });",
    "        state.mempool.counters.reconcile_removed_total = state\n            .mempool\n            .counters\n            .reconcile_removed_total\n            .saturating_add(resource_removed.len() as u64);\n        return Ok(MempoolReconcileResult {\n            removed_txids: resource_removed,\n            kept_txids: Vec::new(),\n        });",
)
replace_once(
    "crates/pulsedag-core/src/mempool_protocol.rs",
    "    rebuilt_mempool.counters.reconcile_removed_total = rebuilt_mempool\n        .counters\n        .reconcile_removed_total\n        .saturating_add(removed_txids.len() as u64);\n    state.mempool = rebuilt_mempool;\n\n    Ok(MempoolReconcileResult {\n        removed_txids,\n        kept_txids,\n    })",
    "    resource_removed.append(&mut removed_txids);\n    resource_removed.sort();\n    resource_removed.dedup();\n    rebuilt_mempool.counters.reconcile_removed_total = rebuilt_mempool\n        .counters\n        .reconcile_removed_total\n        .saturating_add(resource_removed.len() as u64);\n    state.mempool = rebuilt_mempool;\n\n    Ok(MempoolReconcileResult {\n        removed_txids: resource_removed,\n        kept_txids,\n    })",
)

# Production fresh admission rejects oversized canonical transactions before
# any live/orphan mutation. Compatibility/custom v3 policy semantics are left
# unchanged.
replace_once(
    "crates/pulsedag-core/src/mempool_admission_v3.rs",
    "    mempool_v3::{\n        fee_rate_v3, MempoolPolicyRejectionV3, MempoolPolicyV3, MEMPOOL_POLICY_V3_VERSION,\n    },",
    "    mempool_resource_v1::{\n        assess_production_transaction_resources_v1, mempool_resource_rejection_reason_v1,\n        MempoolResourceAssessmentErrorV1,\n    },\n    mempool_v3::{\n        fee_rate_v3, MempoolPolicyRejectionV3, MempoolPolicyV3, MEMPOOL_POLICY_V3_VERSION,\n    },",
)
replace_once(
    "crates/pulsedag-core/src/mempool_admission_v3.rs",
    "    if tx.fee > policy.max_transaction_fee {",
    "    if policy == MempoolPolicyV3::production_default() {\n        match assess_production_transaction_resources_v1(tx, state) {\n            Ok(_) => {}\n            Err(MempoolResourceAssessmentErrorV1::TransactionTooLarge {\n                canonical_size_bytes,\n                max_canonical_tx_bytes,\n            }) => {\n                state.mempool.counters.rejected_total =\n                    state.mempool.counters.rejected_total.saturating_add(1);\n                return Err(TxAcceptanceResult::Rejected(\n                    mempool_resource_rejection_reason_v1(\n                        canonical_size_bytes,\n                        max_canonical_tx_bytes,\n                    ),\n                ));\n            }\n            Err(MempoolResourceAssessmentErrorV1::CanonicalSize(error)) => {\n                state.mempool.counters.rejected_total =\n                    state.mempool.counters.rejected_total.saturating_add(1);\n                return Err(TxAcceptanceResult::Invalid(format!(\n                    \"mempool resource canonical size assessment failed: {error}\"\n                )));\n            }\n        }\n    }\n\n    if tx.fee > policy.max_transaction_fee {",
)

# Stable permanent RPC classification for resource-size rejection.
replace_once(
    "crates/pulsedag-rpc/src/handlers/tx_protocol.rs",
    "    mempool_policy_rejection_code_from_reason_v3, mempool_policy_rejection_detail_v3,\n    tx_protocol::resolve_transaction_validation_path, AcceptSource, ChainState, MempoolPolicyV3,",
    "    mempool_policy_rejection_code_from_reason_v3, mempool_policy_rejection_detail_v3,\n    mempool_resource_rejection_code_from_reason_v1, mempool_resource_rejection_detail_v1,\n    tx_protocol::resolve_transaction_validation_path, AcceptSource, ChainState, MempoolPolicyV3,",
)
replace_once(
    "crates/pulsedag-rpc/src/handlers/tx_protocol.rs",
    "    let reason = rejection_reason(result);\n    if let Some(code) = mempool_policy_rejection_code_from_reason_v3(&reason) {",
    "    let reason = rejection_reason(result);\n    if let Some(code) = mempool_resource_rejection_code_from_reason_v1(&reason) {\n        return ApiResponse::err(code, mempool_resource_rejection_detail_v1(&reason));\n    }\n    if let Some(code) = mempool_policy_rejection_code_from_reason_v3(&reason) {",
)
rpc = Path("crates/pulsedag-rpc/src/handlers/tx_protocol.rs")
rpc.write_text(
    rpc.read_text().rstrip()
    + '''\n\n#[cfg(test)]\nmod production_resource_rejection_tests {\n    use super::*;\n\n    #[test]\n    fn oversized_resource_rejection_is_stable_and_not_transient_mempool_full() {\n        let chain = pulsedag_core::genesis::init_chain_state(\"rpc-resource-code\".to_string());\n        let transaction = pulsedag_core::types::Transaction {\n            txid: \"oversized-rpc\".to_string(),\n            version: pulsedag_core::TRANSACTION_VERSION_V1,\n            inputs: Vec::new(),\n            outputs: Vec::new(),\n            fee: 1,\n            nonce: 1,\n        };\n        let result = TxAcceptanceResult::Rejected(\n            pulsedag_core::mempool_resource_rejection_reason_v1(32_769, 32_768),\n        );\n        let response = classified_rejection(&transaction, &chain, None, &result);\n        let error = response.error.expect(\"resource rejection error\");\n        assert_eq!(error.code, \"MEMPOOL_RESOURCE_TX_TOO_LARGE\");\n        assert!(error.classification.is_none());\n    }\n}\n'''
)

# Optional admission-age sidecar corruption must not brick a valid positional
# CHAIN_STATE_KEY. Fail safe to empty age metadata and record diagnostics.
replace_once(
    "crates/pulsedag-storage/src/lib.rs",
    "pub static STARTUP_STORAGE_RECONCILIATION_FAILED_TOTAL: AtomicU64 = AtomicU64::new(0);\npub static SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL: AtomicU64 = AtomicU64::new(0);",
    "pub static STARTUP_STORAGE_RECONCILIATION_FAILED_TOTAL: AtomicU64 = AtomicU64::new(0);\npub static MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL: AtomicU64 = AtomicU64::new(0);\npub static SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL: AtomicU64 = AtomicU64::new(0);",
)
replace_once(
    "crates/pulsedag-storage/src/lib.rs",
    '''            let persisted: BTreeMap<Hash, u64> = bincode::deserialize(&sidecar).map_err(|e| {\n                PulseError::StorageError(format!(\n                    "mempool admission-height sidecar is corrupt: {e}"\n                ))\n            })?;\n            for (txid, height) in persisted {\n                if state.mempool.transactions.contains_key(&txid) {\n                    state.mempool.admission_height.insert(txid, height);\n                }\n            }\n''',
    '''            match bincode::deserialize::<BTreeMap<Hash, u64>>(&sidecar) {\n                Ok(persisted) => {\n                    for (txid, height) in persisted {\n                        if state.mempool.transactions.contains_key(&txid) {\n                            state.mempool.admission_height.insert(txid, height);\n                        }\n                    }\n                }\n                Err(_) => {\n                    // Optional policy-age metadata must not invalidate an otherwise\n                    // valid positional chain state. Empty age is the legacy fail-safe\n                    // retain behavior frozen by #1081.\n                    MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL\n                        .fetch_add(1, Ordering::Relaxed);\n                }\n            }\n''',
)

# Executable transport headroom proof: output-heavy and input-heavy valid-format
# v1/v2/v3 shapes are grown to the 32 KiB canonical boundary and their complete
# NewTransaction JSON carrier must still fit the existing 64 KiB P2P ceiling.
p2p_test = r'''
    #[test]
    fn production_canonical_tx_ceiling_fits_frozen_p2p_tx_wire_ceiling_v1_v2_v3() {
        use pulsedag_core::{
            canonical_transaction_size_for_mempool_v3, encode_hybrid_public_key_v1,
            encode_hybrid_signature_v1, types::{OutPoint, TxInput, TxOutput},
            ED25519_PUBLIC_KEY_BYTES, ED25519_SIGNATURE_BYTES, ML_DSA_65_PUBLIC_KEY_BYTES,
            ML_DSA_65_SIGNATURE_BYTES, MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES,
            TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2, TRANSACTION_VERSION_V3,
        };

        fn assert_wire_headroom(tx: &Transaction, chain_id: &str) {
            let canonical = canonical_transaction_size_for_mempool_v3(tx, chain_id).unwrap();
            assert!(canonical <= MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES);
            assert!(canonical >= 30 * 1024, "shape did not exercise near-boundary sizing");
            let wire = serde_json::to_vec(&NetworkMessage::NewTransaction {
                chain_id: chain_id.to_string(),
                transaction: tx.clone(),
            })
            .unwrap();
            assert!(
                wire.len() <= MAX_TX_MESSAGE_BYTES,
                "version {} canonical={} wire={} exceeds p2p ceiling={}",
                tx.version,
                canonical,
                wire.len(),
                MAX_TX_MESSAGE_BYTES
            );
        }

        fn near_limit_outputs(version: u32, chain_id: &str) -> Transaction {
            let mut tx = Transaction {
                txid: "ab".repeat(32),
                version,
                inputs: Vec::new(),
                outputs: Vec::new(),
                fee: 10,
                nonce: 7,
            };
            loop {
                tx.outputs.push(TxOutput {
                    address: "pulse1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    amount: 1,
                });
                if canonical_transaction_size_for_mempool_v3(&tx, chain_id).unwrap()
                    > MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES
                {
                    tx.outputs.pop();
                    break;
                }
            }
            tx
        }

        fn standard_input(index: u32) -> TxInput {
            TxInput {
                previous_output: OutPoint {
                    txid: format!("{:064x}", u64::from(index) + 1),
                    index,
                },
                public_key: "11".repeat(32),
                signature: "22".repeat(64),
            }
        }

        fn hybrid_input(index: u32) -> TxInput {
            TxInput {
                previous_output: OutPoint {
                    txid: format!("{:064x}", u64::from(index) + 1),
                    index,
                },
                public_key: encode_hybrid_public_key_v1(
                    &[0x11; ED25519_PUBLIC_KEY_BYTES],
                    &vec![0x22; ML_DSA_65_PUBLIC_KEY_BYTES],
                )
                .unwrap(),
                signature: encode_hybrid_signature_v1(
                    &[0x33; ED25519_SIGNATURE_BYTES],
                    &vec![0x44; ML_DSA_65_SIGNATURE_BYTES],
                )
                .unwrap(),
            }
        }

        fn near_limit_inputs(version: u32, chain_id: &str) -> Transaction {
            let mut tx = Transaction {
                txid: "cd".repeat(32),
                version,
                inputs: Vec::new(),
                outputs: Vec::new(),
                fee: 10,
                nonce: 9,
            };
            loop {
                let index = tx.inputs.len() as u32;
                tx.inputs.push(if version == TRANSACTION_VERSION_V3 {
                    hybrid_input(index)
                } else {
                    standard_input(index)
                });
                if canonical_transaction_size_for_mempool_v3(&tx, chain_id).unwrap()
                    > MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES
                {
                    tx.inputs.pop();
                    break;
                }
            }
            tx
        }

        let chain_id = "pulsedag-resource-wire-headroom";
        for version in [
            TRANSACTION_VERSION_V1,
            TRANSACTION_VERSION_V2,
            TRANSACTION_VERSION_V3,
        ] {
            assert_wire_headroom(&near_limit_outputs(version, chain_id), chain_id);
            assert_wire_headroom(&near_limit_inputs(version, chain_id), chain_id);
        }
    }

'''
replace_once(
    "crates/pulsedag-p2p/src/lib.rs",
    "    #[test]\n    fn oversized_inbound_tx_message_is_rejected() {",
    p2p_test + "    #[test]\n    fn oversized_inbound_tx_message_is_rejected() {",
)

# Focused policy document. Resource identity stays separate from the frozen fee
# identity, and 1440 is explicitly a logical-height—not wall-clock—TTL.
doc = Path("docs/MEMPOOL_POLICY_V3.md")
marker = "## Production resource, eviction and expiry contract"
if marker in doc.read_text():
    raise SystemExit("resource contract documentation already present")
doc.write_text(
    doc.read_text().rstrip()
    + r'''

## Production resource, eviction and expiry contract

Production mempool resources are versioned separately from `MempoolPolicyV3`, so freezing resource/expiry values does not change the already-frozen fee-policy identity. Resource policy v1 freezes:

- live transaction ceiling: `4096`;
- tracked spent-outpoint ceiling: `8192`;
- orphan transaction ceiling: `512`;
- canonical transaction size ceiling: `32768` bytes (`32 KiB`), measured by the same v1/v2/v3 canonical encoders used by mempool fee-rate accounting;
- live transaction maximum age: `1440` accepted-height steps.

Resource-policy fingerprint: `759a2820217e8b2d897634745b1fffad347f9f72f62348bb9effd5f1b79034cf` under `PulseDAG:mempool-resource-policy:v1`. The active fee-policy fingerprint remains `fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8`.

The `32 KiB` canonical ceiling is intentionally below the existing `64 KiB` full P2P `NewTransaction` carrier ceiling. Exact tests grow representative output-heavy and input-heavy v1/v2/v3 transaction shapes to the canonical boundary and require the complete serialized carrier to remain below the frozen transport ceiling. The P2P wire limit itself is unchanged.

Expiry uses only the persisted canonical DAG logical clock (`dag.best_height`) and the #1081 boundary `current_height >= admission_height + max_age_blocks`. `1440` heights is nominally 24 hours at the frozen 60-second target interval, but it is a height policy, not a wall-clock timer. Missing legacy age metadata, future admission heights and checked-add overflow retain fail-safe rather than inventing age.

Fresh production RPC/P2P admission rejects transactions above the canonical-size ceiling before live/orphan mutation with stable code `MEMPOOL_RESOURCE_TX_TOO_LARGE`. Restored oversized live roots are removed with all live descendants; restored oversized orphans are dropped with their orphan metadata. Production orphan promotion re-enters the same production admission wrapper, so it cannot bypass the size ceiling.

Every ordinary or protocol-aware mempool reconciliation applies the same production resource normalization after any protocol identity has been validated. Existing stricter runtime/test caps remain stricter; persisted limits above production are clamped. Expiry and over-capacity cleanup are deterministic and package-safe. The existing live incoming package scoring/replacement behavior is unchanged and RBF remains disabled.

A corrupt optional `mempool_admission_height_v1` RocksDB sidecar no longer invalidates an otherwise valid `CHAIN_STATE_KEY`: loading records a recovery counter and leaves age metadata empty, preserving the legacy fail-safe retain rule. `STORAGE_SCHEMA_VERSION` remains `1`.

This contract changes mempool relay/resource policy only. It does not change consensus transaction validity, monetary rules, signing/txid, P2P wire identity, mining consensus, or replacement/RBF semantics. #781/#794 launch authority remains separate.
'''
    + "\n"
)
