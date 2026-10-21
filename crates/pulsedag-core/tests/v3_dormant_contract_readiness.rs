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

fn isolated_identity() -> (ProtocolActivationIdentity, ProgrammabilityActivationIdentity) {
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
    let expected_fingerprint =
        "dd9c436647ddcd2af828f1c3dd60ac13ca6102ce436adc1d8e67494da409908d";
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
