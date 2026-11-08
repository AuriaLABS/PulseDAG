//! Offline contract readiness conformance. Does NOT enable contract consensus.
//! Run identically with and without executable-contracts to prove v3.0
//! inactive network behavior is independent of compiled developer tooling.

use pulsedag_core::{
    contract_v3::{
        ProgrammabilityActivationIdentity, ProgrammabilityDomain, ResourceBudgetV1,
        RESOURCE_BUDGET_VERSION_V1,
    },
    contracts_gate::programmability_activation_v1::{
        authorize_programmability_execution_v1, protocol_activation_identity_fingerprint_bytes,
        ProgrammabilityActivationContractV1, ProgrammabilityActivationV1Error,
    },
    contracts_may_execute,
    protocol::ProtocolActivationIdentity,
    pulsescript_vm_v1::{
        compile_pulsescript_v1, execute_pulse_bytecode_v1, pulse_bytecode_fingerprint_v1,
        PulseValueV1, PulseVmV1Error,
    },
};

fn isolated_identity() -> (
    ProtocolActivationIdentity,
    ProgrammabilityActivationIdentity,
) {
    // Synthetic historical protocol identity for the dormant activation-contract
    // fixture; deliberately NOT a v3.0 network identity or activation authority.
    let base = ProtocolActivationIdentity::activated_v2(
        "offline-contract-testnet",
        "offline-frozen-genesis",
        "offline-ordered-dag",
    );
    let identity = ProgrammabilityActivationIdentity {
        version: 1,
        domain: ProgrammabilityDomain::Testnet,
        chain_id: "offline-contract-testnet".to_owned(),
        genesis_hash: [0x41; 32],
        base_protocol_fingerprint: protocol_activation_identity_fingerprint_bytes(&base).unwrap(),
    };
    (base, identity)
}

fn budget(compute_units: u64) -> ResourceBudgetV1 {
    ResourceBudgetV1 {
        version: RESOURCE_BUDGET_VERSION_V1,
        compute_units,
        read_bytes: 0,
        write_bytes: 0,
        proof_bytes: 0,
        event_bytes: 0,
    }
}

#[test]
fn inactive_contract_is_never_authorized_even_with_runtime_flag_and_trusted_bytes() {
    let (base, identity) = isolated_identity();
    let contract = ProgrammabilityActivationContractV1::inactive(&identity, &base).unwrap();

    for runtime_enabled in [false, true] {
        for trusted in [[0; 32], [0x44; 32]] {
            assert_eq!(
                authorize_programmability_execution_v1(
                    &contract,
                    &identity,
                    &base,
                    runtime_enabled,
                    trusted,
                ),
                Err(ProgrammabilityActivationV1Error::Inactive),
            );
        }
    }
    assert!(!contracts_may_execute(false));
}

#[test]
fn local_upgrade_proposal_with_no_trusted_authority_fails_closed() {
    let (base, identity) = isolated_identity();
    let proposal =
        ProgrammabilityActivationContractV1::protocol_upgrade(&identity, &base, [0x44; 32])
            .unwrap();
    assert_eq!(
        authorize_programmability_execution_v1(&proposal, &identity, &base, true, [0; 32]),
        Err(ProgrammabilityActivationV1Error::TrustedAuthorizationMissing),
    );
    assert_eq!(
        authorize_programmability_execution_v1(&proposal, &identity, &base, false, [0; 32]),
        Err(ProgrammabilityActivationV1Error::TrustedAuthorizationMissing),
    );
}

#[test]
fn offline_vm_vectors_are_identical_across_feature_builds_without_network_activation() {
    let source = "pulsescript-v1\npush_u64 2\npush_u64 3\nadd_u64\npush_u64 5\neq_u64\nhalt\n";
    let compiled = compile_pulsescript_v1(source).unwrap();
    let expected_fingerprint = "dd9c436647ddcd2af828f1c3dd60ac13ca6102ce436adc1d8e67494da409908d";
    assert_eq!(
        hex::encode(pulse_bytecode_fingerprint_v1(&compiled.bytecode).unwrap()),
        expected_fingerprint,
    );
    assert_eq!(
        hex::encode(compiled.bytecode_fingerprint),
        expected_fingerprint,
    );
    assert_eq!(
        execute_pulse_bytecode_v1(&compiled.bytecode, &budget(6))
            .unwrap()
            .result,
        PulseValueV1::Bool(true),
    );
    assert!(!contracts_may_execute(false));
}

#[test]
fn offline_vm_resource_exhaustion_is_rejected_in_both_feature_builds() {
    let compiled = compile_pulsescript_v1(
        "pulsescript-v1\npush_u64 2\npush_u64 3\nadd_u64\npush_u64 5\neq_u64\nhalt\n",
    )
    .unwrap();
    assert_eq!(
        execute_pulse_bytecode_v1(&compiled.bytecode, &budget(5)),
        Err(PulseVmV1Error::ComputeBudgetExceeded { actual: 6, max: 5 }),
    );
}

#[test]
fn native_v3_signed_replay_refuses_enabled_contract_state_in_both_feature_profiles() {
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3, genesis_v3::init_chain_state_v3,
        header_v3::BlockHeaderV3,
        native_v3_signed_prestate_trial::inspect_dormant_native_v3_explicit_order_trial,
        protocol::BLOCK_HEADER_VERSION_V3, reward_settlement_v3::build_reward_claim_transaction_v3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "TEST-ONLY-v3-inactive-contract-signed-replay";
    let clean_state = init_chain_state_v3(CHAIN.into(), 1_800_000_000).unwrap();
    assert!(!clean_state.contracts.config.enabled);
    let original_root = clean_state.utxo.compute_state_root().unwrap();
    let claim = build_reward_claim_transaction_v3("pulse1test-miner", 1, CHAIN).unwrap();
    let transactions = vec![claim];
    let header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec!["11".repeat(32)],
        timestamp_ns: 1_800_000_000_500_000_000,
        difficulty: 0x207f_ffff,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: "33".repeat(32), // Deliberately not a committed monetary root.
        blue_score: 1,
        height: 1,
    };
    // No PoW or network admission: this is a claim-only signed-replay
    // diagnostic checking that enabling contracts cannot change its gate.
    let block = build_block_envelope_v3(CHAIN, header, transactions).unwrap();
    let clean =
        inspect_dormant_native_v3_explicit_order_trial(std::slice::from_ref(&block), &clean_state)
            .unwrap();
    assert_eq!(clean.candidate_fees, 0);
    assert_eq!(clean.signed_transactions, 0);
    assert_eq!(clean.resulting_utxo_root, original_root);

    let mut unsafe_state = clean_state.clone();
    unsafe_state.contracts.config.enabled = true;
    let root_before_rejection = unsafe_state.utxo.compute_state_root().unwrap();
    let rejected = inspect_dormant_native_v3_explicit_order_trial(&[block], &unsafe_state)
        .unwrap_err()
        .to_string();
    assert!(rejected.contains("INACTIVE"), "{rejected}");
    assert_eq!(
        unsafe_state.utxo.compute_state_root().unwrap(),
        root_before_rejection
    );
    assert_eq!(
        clean_state.utxo.compute_state_root().unwrap(),
        original_root
    );
}
