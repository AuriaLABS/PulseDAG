use pulsedag_core::{genesis::init_chain_state, state_digest};

#[test]
fn integrated_state_digest_v2_commits_contract_runtime_state() {
    let state = init_chain_state("pulsedag-testnet".to_string());
    let baseline = state_digest(&state).unwrap();
    assert_eq!(baseline, state_digest(&state.clone()).unwrap());

    let mut enabled = state.clone();
    enabled.contracts.config.enabled = !enabled.contracts.config.enabled;
    assert_ne!(baseline, state_digest(&enabled).unwrap());

    let mut vm_version = state.clone();
    vm_version.contracts.config.vm_version.push_str("-mutated");
    assert_ne!(baseline, state_digest(&vm_version).unwrap());

    let mut max_gas_per_tx = state.clone();
    max_gas_per_tx.contracts.config.max_gas_per_tx += 1;
    assert_ne!(baseline, state_digest(&max_gas_per_tx).unwrap());

    let mut max_contract_size_bytes = state.clone();
    max_contract_size_bytes
        .contracts
        .config
        .max_contract_size_bytes += 1;
    assert_ne!(baseline, state_digest(&max_contract_size_bytes).unwrap());

    let mut max_storage_key_bytes = state.clone();
    max_storage_key_bytes.contracts.config.max_storage_key_bytes += 1;
    assert_ne!(baseline, state_digest(&max_storage_key_bytes).unwrap());

    let mut max_storage_value_bytes = state.clone();
    max_storage_value_bytes
        .contracts
        .config
        .max_storage_value_bytes += 1;
    assert_ne!(baseline, state_digest(&max_storage_value_bytes).unwrap());

    let mut contract_count = state.clone();
    contract_count.contracts.contract_count += 1;
    assert_ne!(baseline, state_digest(&contract_count).unwrap());

    let mut storage_slots = state.clone();
    storage_slots.contracts.storage_slots += 1;
    assert_ne!(baseline, state_digest(&storage_slots).unwrap());

    let mut receipt_count = state.clone();
    receipt_count.contracts.receipt_count += 1;
    assert_ne!(baseline, state_digest(&receipt_count).unwrap());

    let mut receipt_id_a = state.clone();
    receipt_id_a.contracts.last_receipt_id = Some("receipt-1".to_string());
    let receipt_id_a_digest = state_digest(&receipt_id_a).unwrap();
    assert_ne!(baseline, receipt_id_a_digest);

    let mut receipt_id_b = state.clone();
    receipt_id_b.contracts.last_receipt_id = Some("receipt-2".to_string());
    assert_ne!(receipt_id_a_digest, state_digest(&receipt_id_b).unwrap());
}
