use std::{error::Error, fmt, io};

use crate::{
    decrypt_private_key, decrypt_wallet_seed,
    keystore_crypto::{encrypt_private_key_with_kdf_costs, KeystoreKdfCosts},
    keystore_seed::{encrypt_wallet_seed_with_kdf_costs, SeedKeystoreKdfCosts},
    SecretString, WalletKeystoreCryptoError, WalletKeystoreFile, WalletKeystoreFormatError,
    WalletKeystorePersistenceError, WalletKeystorePersistenceReport, KEYSTORE_SEED_VERSION,
    KEYSTORE_VERSION,
};

#[derive(Debug)]
pub enum WalletKeystoreRotationError {
    Persistence(WalletKeystorePersistenceError),
    Crypto(WalletKeystoreCryptoError),
    AtomicReplacementUnsupported,
    UnsafePath(&'static str),
    RandomnessUnavailable,
    TooLarge {
        limit: u64,
        actual: u64,
    },
    Json(serde_json::Error),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    PublishedButDirectorySyncFailed(io::Error),
}

impl fmt::Display for WalletKeystoreRotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(error) => write!(f, "wallet keystore persistence failed: {error}"),
            Self::Crypto(error) => write!(f, "wallet keystore rotation crypto failed: {error}"),
            Self::AtomicReplacementUnsupported => f.write_str(
                "atomic wallet keystore replacement is not supported on this platform",
            ),
            Self::UnsafePath(reason) => write!(f, "unsafe wallet keystore path: {reason}"),
            Self::RandomnessUnavailable => f.write_str("operating-system randomness is unavailable"),
            Self::TooLarge { limit, actual } => write!(
                f,
                "wallet keystore replacement is too large ({actual} bytes > {limit} bytes)"
            ),
            Self::Json(_) => f.write_str("wallet keystore replacement JSON encoding failed"),
            Self::Io { operation, .. } => {
                write!(f, "wallet keystore rotation I/O failed during {operation}")
            }
            Self::PublishedButDirectorySyncFailed(_) => f.write_str(
                "wallet keystore replacement was published but parent-directory durability sync failed",
            ),
        }
    }
}

impl Error for WalletKeystoreRotationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Persistence(error) => Some(error),
            Self::Crypto(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io { source, .. } | Self::PublishedButDirectorySyncFailed(source) => Some(source),
            _ => None,
        }
    }
}

impl From<WalletKeystorePersistenceError> for WalletKeystoreRotationError {
    fn from(value: WalletKeystorePersistenceError) -> Self {
        Self::Persistence(value)
    }
}

impl From<WalletKeystoreCryptoError> for WalletKeystoreRotationError {
    fn from(value: WalletKeystoreCryptoError) -> Self {
        Self::Crypto(value)
    }
}

/// Re-encrypt an existing keystore with a new password while preserving the
/// secret kind, authenticated identity, and authenticated Argon2 cost policy.
/// Salt, nonce, and ciphertext are regenerated with OS randomness.
pub fn rotate_keystore_password(
    session: &WalletKeystoreFile,
    current_password: &SecretString,
    new_password: &SecretString,
) -> Result<WalletKeystorePersistenceReport, WalletKeystoreRotationError> {
    let current = session.load()?;
    let replacement = match current.version {
        KEYSTORE_VERSION => {
            let secret_key = decrypt_private_key(&current, current_password)?;
            encrypt_private_key_with_kdf_costs(
                &current.network_profile,
                &current.chain_id,
                &current.address,
                &secret_key,
                new_password,
                KeystoreKdfCosts::new(
                    current.kdf.memory_kib,
                    current.kdf.iterations,
                    current.kdf.lanes,
                ),
            )?
        }
        KEYSTORE_SEED_VERSION => {
            let seed = decrypt_wallet_seed(&current, current_password)?;
            encrypt_wallet_seed_with_kdf_costs(
                &current.network_profile,
                &current.chain_id,
                &current.address,
                &seed,
                new_password,
                SeedKeystoreKdfCosts::new(
                    current.kdf.memory_kib,
                    current.kdf.iterations,
                    current.kdf.lanes,
                ),
            )?
        }
        version => {
            return Err(WalletKeystoreCryptoError::Format(
                WalletKeystoreFormatError::UnsupportedVersion(version),
            )
            .into())
        }
    };
    fs_backend::replace_existing_atomically(session.path(), &replacement)
}

#[path = "keystore_rotation_fs.rs"]
mod fs_backend;

#[cfg(all(test, unix))]
#[path = "keystore_rotation_tests_compact.rs"]
mod tests;
