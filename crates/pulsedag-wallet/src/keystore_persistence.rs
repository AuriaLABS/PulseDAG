include!("keystore_persistence_base.inc");

impl WalletKeystoreFile {
    pub fn permission_policy_preflight() -> WalletKeystorePermissionStatus {
        #[cfg(unix)]
        {
            WalletKeystorePermissionStatus::EnforcedOwnerReadWrite
        }
        #[cfg(not(unix))]
        {
            WalletKeystorePermissionStatus::NotEnforcedOnThisPlatform
        }
    }
}

#[cfg(test)]
mod phase1_permission_policy_tests {
    use super::*;

    #[test]
    fn permission_policy_preflight_matches_platform_contract() {
        #[cfg(unix)]
        assert_eq!(
            WalletKeystoreFile::permission_policy_preflight(),
            WalletKeystorePermissionStatus::EnforcedOwnerReadWrite
        );
        #[cfg(not(unix))]
        assert_eq!(
            WalletKeystoreFile::permission_policy_preflight(),
            WalletKeystorePermissionStatus::NotEnforcedOnThisPlatform
        );
    }
}
