use pulsedag_core::{
    contracts_compile_identity, contracts_compile_time_executable, contracts_may_execute,
    reject_inactive_contract_apply, EXECUTABLE_CONTRACTS_COMPILED,
};

#[test]
fn compile_identity_matches_feature_gate() {
    assert_eq!(
        contracts_compile_time_executable(),
        EXECUTABLE_CONTRACTS_COMPILED
    );
    if EXECUTABLE_CONTRACTS_COMPILED {
        assert_eq!(
            contracts_compile_identity(),
            "executable-contracts-compiled"
        );
    } else {
        assert_eq!(contracts_compile_identity(), "inactive-task31");
        assert!(!contracts_may_execute(true));
        assert!(reject_inactive_contract_apply(true).is_err());
    }
}

#[test]
fn runtime_flag_cannot_enable_apply_when_compile_feature_is_off() {
    if EXECUTABLE_CONTRACTS_COMPILED {
        assert!(contracts_may_execute(true));
        assert!(!contracts_may_execute(false));
        assert!(reject_inactive_contract_apply(true).is_ok());
        assert!(reject_inactive_contract_apply(false).is_err());
    } else {
        assert!(!contracts_may_execute(false));
        assert!(!contracts_may_execute(true));
        assert!(reject_inactive_contract_apply(false).is_err());
        assert!(reject_inactive_contract_apply(true).is_err());
    }
}
