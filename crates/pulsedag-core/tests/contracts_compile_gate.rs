use pulsedag_core::{
    contracts_compile_identity, contracts_compile_time_executable, contracts_may_execute,
    reject_inactive_contract_apply, EXECUTABLE_CONTRACTS_COMPILED,
};

#[test]
fn default_task31_binary_does_not_compile_executable_contracts() {
    assert!(!EXECUTABLE_CONTRACTS_COMPILED);
    assert!(!contracts_compile_time_executable());
    assert_eq!(contracts_compile_identity(), "inactive-task31");
}

#[test]
fn runtime_flag_cannot_enable_apply_without_compile_feature() {
    assert!(!contracts_may_execute(false));
    assert!(!contracts_may_execute(true));
    assert!(reject_inactive_contract_apply(false).is_err());
    assert!(reject_inactive_contract_apply(true).is_err());
}
