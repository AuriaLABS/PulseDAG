//! Compile-time programmability gate for the v2.4.0 Task31 candidate (#1131).
//!
//! Contract/covenant modules remain in the tree as inactive foundation.
//! Executable apply is compiled only when the `executable-contracts` Cargo
//! feature is enabled. Default Task31 / node builds must leave that feature off.

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
        "contracts-inactive-compile-gate"
    }
}

/// Whether this binary is allowed to treat `contracts.config.enabled = true`
/// as an executable admission path.
pub fn contracts_compile_time_executable() -> bool {
    EXECUTABLE_CONTRACTS_COMPILED
}

/// Combined runtime + compile gate. Runtime `enabled` is ignored unless the
/// compile feature is present.
pub fn contracts_may_execute(runtime_enabled: bool) -> bool {
    EXECUTABLE_CONTRACTS_COMPILED && runtime_enabled
}

/// Reject executable apply when the Task31 compile gate is closed.
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
    fn default_task31_build_does_not_compile_executable_contracts() {
        assert!(!EXECUTABLE_CONTRACTS_COMPILED);
        assert!(!contracts_compile_time_executable());
        assert!(!contracts_may_execute(true));
        assert!(reject_inactive_contract_apply(true).is_err());
        assert_eq!(contracts_compile_identity(), "contracts-inactive-compile-gate");
    }
}
