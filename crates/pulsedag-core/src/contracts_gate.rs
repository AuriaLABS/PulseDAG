//! Compile-time programmability gate for the v2.4.0 Task31 candidate (#1131).
//!
//! Contract/covenant modules remain in the tree as inactive foundation.
//! Executable apply is compiled only when the `executable-contracts` Cargo
//! feature is enabled. Default Task31 / node builds must leave that feature off.
//!
//! This compile/runtime gate is a necessary execution precondition only. It is
//! not consensus authorization. A future activation must additionally satisfy
//! the versioned, identity-bound, protocol-upgrade authorization contract in
//! `programmability_activation_v1`; the current launch profile remains inactive.

#[path = "programmability_activation_v1.rs"]
pub mod programmability_activation_v1;

/// True when this crate was built with `--features executable-contracts`.
#[cfg(feature = "executable-contracts")]
pub const EXECUTABLE_CONTRACTS_COMPILED: bool = true;

/// True when this crate was built with `--features executable-contracts`.
#[cfg(not(feature = "executable-contracts"))]
pub const EXECUTABLE_CONTRACTS_COMPILED: bool = false;

/// Human-readable compile identity for logs and `/status`.
pub fn contracts_compile_identity() -> &'static str {
    if EXECUTABLE_CONTRACTS_COMPILED {
        "executable-contracts-compiled"
    } else {
        "inactive-task31"
    }
}

/// Whether this binary is allowed to treat `contracts.config.enabled = true`
/// as an executable admission precondition. This does not grant consensus
/// authorization by itself.
pub fn contracts_compile_time_executable() -> bool {
    EXECUTABLE_CONTRACTS_COMPILED
}

/// Combined runtime + compile precondition. Runtime `enabled` is ignored unless
/// the compile feature is present. A `true` result is still insufficient for
/// consensus activation without `ProgrammabilityActivationContractV1`.
pub fn contracts_may_execute(runtime_enabled: bool) -> bool {
    EXECUTABLE_CONTRACTS_COMPILED && runtime_enabled
}

/// Reject executable apply when the Task31 compile gate is closed.
///
/// Passing this precondition does not authorize programmability at consensus;
/// callers must also satisfy the separate activation contract before any future
/// execution wiring can be considered.
pub fn reject_inactive_contract_apply(runtime_enabled: bool) -> Result<(), &'static str> {
    if contracts_may_execute(runtime_enabled) {
        Ok(())
    } else {
        Err("contract apply is inactive: compile gate closed or contracts_enabled=false")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_gate_identity_matches_feature() {
        assert_eq!(
            contracts_compile_time_executable(),
            EXECUTABLE_CONTRACTS_COMPILED
        );
        assert_eq!(contracts_may_execute(true), EXECUTABLE_CONTRACTS_COMPILED);
        assert!(!contracts_may_execute(false));
        if EXECUTABLE_CONTRACTS_COMPILED {
            assert_eq!(
                contracts_compile_identity(),
                "executable-contracts-compiled"
            );
            assert!(reject_inactive_contract_apply(true).is_ok());
        } else {
            assert_eq!(contracts_compile_identity(), "inactive-task31");
            assert!(reject_inactive_contract_apply(true).is_err());
        }
    }
}
