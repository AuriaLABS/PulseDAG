use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use rand::{rngs::OsRng, RngCore};

use crate::{
    decrypt_private_key, encrypt_private_key, SecretString, WalletKeystoreCryptoError,
    WalletKeystoreDirectorySyncStatus, WalletKeystoreFile, WalletKeystorePermissionStatus,
    WalletKeystorePersistenceError, WalletKeystorePersistenceReport, KEYSTORE_FILE_MAX_BYTES,
};

const ROTATION_TEMP_ATTEMPTS: usize = 32;

#[derive(Debug)]
pub enum WalletKeystoreRotationError {
    Persistence(WalletKeystorePersistenceError),
    Crypto(WalletKeystoreCryptoError),
    AtomicReplacementUnsupported,
    UnsafePath(&'static str),
    RandomnessUnavailable,
    TooLarge { limit: u64, actual: u64 },
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
            Self::RandomnessUnavailable => {
                f.write_str("operating-system randomness is unavailable")
            }
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
            Self::Io { source, .. } => Some(source),
            Self::PublishedButDirectorySyncFailed(error) => Some(error),
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
/// wallet secret and all authenticated network/account identity fields.
///
/// The caller must hold the [`WalletKeystoreFile`] session for the entire
/// operation. The current password is authenticated before any replacement is
/// prepared. On supported platforms publication is an atomic same-directory
/// rename of a fully synced private temporary file; the live keystore is never
/// truncated in place.
pub fn rotate_keystore_password(
    session: &WalletKeystoreFile,
    current_password: &SecretString,
    new_password: &SecretString,
) -> Result<WalletKeystorePersistenceReport, WalletKeystoreRotationError> {
    let current = session.load()?;
    let secret_key = decrypt_private_key(&current, current_password)?;
    let replacement = encrypt_private_key(
        &current.network_profile,
        &current.chain_id,
        &current.address,
        &secret_key,
        new_password,
    )?;
    replace_existing_atomically(session.path(), &replacement)
}

#[cfg(not(unix))]
fn replace_existing_atomically(
    _path: &Path,
    _replacement: &crate::WalletKeystoreEnvelope,
) -> Result<WalletKeystorePersistenceReport, WalletKeystoreRotationError> {
    Err(WalletKeystoreRotationError::AtomicReplacementUnsupported)
}

#[cfg(unix)]
fn replace_existing_atomically(
    path: &Path,
    replacement: &crate::WalletKeystoreEnvelope,
) -> Result<WalletKeystorePersistenceReport, WalletKeystoreRotationError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    replacement
        .validate_structure()
        .map_err(WalletKeystorePersistenceError::from)?;

    let name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(WalletKeystoreRotationError::UnsafePath(
            "a keystore file name is required",
        ))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    ensure_regular_existing_target(path)?;

    let mut payload =
        serde_json::to_vec_pretty(replacement).map_err(WalletKeystoreRotationError::Json)?;
    payload.push(b'\n');
    ensure_size(payload.len() as u64)?;

    let (temp_path, mut temp_file) = create_private_temp(parent, name)?;
    let mut cleanup = TempCleanup::new(temp_path.clone());

    temp_file
        .write_all(&payload)
        .map_err(|source| io_error("write temporary replacement", source))?;
    temp_file
        .sync_all()
        .map_err(|source| io_error("sync temporary replacement", source))?;

    let mut permissions = temp_file
        .metadata()
        .map_err(|source| io_error("inspect temporary replacement", source))?
        .permissions();
    permissions.set_mode(0o600);
    temp_file
        .set_permissions(permissions)
        .map_err(|source| io_error("secure temporary replacement", source))?;
    let mode = temp_file
        .metadata()
        .map_err(|source| io_error("verify temporary replacement permissions", source))?
        .mode()
        & 0o777;
    if mode != 0o600 {
        return Err(io_error(
            "verify temporary replacement permissions",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "wallet keystore replacement permissions are not 0600",
            ),
        ));
    }
    temp_file
        .sync_all()
        .map_err(|source| io_error("sync replacement permissions", source))?;
    drop(temp_file);

    // Re-check immediately before publication. This is not claimed as a
    // sandbox against a hostile local filesystem actor; the advisory lock
    // coordinates cooperating PulseDAG wallet processes.
    ensure_regular_existing_target(path)?;
    fs::rename(&temp_path, path).map_err(|source| io_error("publish replacement", source))?;
    cleanup.disarm();

    if let Err(source) = File::open(parent).and_then(|directory| directory.sync_all()) {
        return Err(WalletKeystoreRotationError::PublishedButDirectorySyncFailed(
            source,
        ));
    }

    Ok(WalletKeystorePersistenceReport {
        file_permissions: WalletKeystorePermissionStatus::EnforcedOwnerReadWrite,
        parent_directory_sync: WalletKeystoreDirectorySyncStatus::Synced,
    })
}

#[cfg(unix)]
fn ensure_regular_existing_target(path: &Path) -> Result<(), WalletKeystoreRotationError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| io_error("inspect keystore", source))?;
    if metadata.file_type().is_symlink() {
        return Err(WalletKeystoreRotationError::UnsafePath(
            "keystore target must not be a symbolic link",
        ));
    }
    if !metadata.is_file() {
        return Err(WalletKeystoreRotationError::UnsafePath(
            "keystore target must be a regular file",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn create_private_temp(
    parent: &Path,
    name: &OsStr,
) -> Result<(PathBuf, File), WalletKeystoreRotationError> {
    use std::os::unix::fs::OpenOptionsExt;

    for _ in 0..ROTATION_TEMP_ATTEMPTS {
        let mut random = [0_u8; 8];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| WalletKeystoreRotationError::RandomnessUnavailable)?;
        let temp_path = control_path(
            parent,
            name,
            &format!(".rotate-{}", hex::encode(random)),
        );
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600);
        match options.open(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error("create temporary replacement", source)),
        }
    }

    Err(io_error(
        "allocate temporary replacement name",
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "temporary replacement name collisions",
        ),
    ))
}

#[cfg(unix)]
fn ensure_size(actual: u64) -> Result<(), WalletKeystoreRotationError> {
    if actual > KEYSTORE_FILE_MAX_BYTES {
        return Err(WalletKeystoreRotationError::TooLarge {
            limit: KEYSTORE_FILE_MAX_BYTES,
            actual,
        });
    }
    Ok(())
}

#[cfg(unix)]
fn control_path(parent: &Path, name: &OsStr, suffix: &str) -> PathBuf {
    let mut control = OsString::from(".");
    control.push(name);
    control.push(suffix);
    parent.join(control)
}

fn io_error(operation: &'static str, source: io::Error) -> WalletKeystoreRotationError {
    WalletKeystoreRotationError::Io { operation, source }
}

#[cfg(unix)]
struct TempCleanup {
    path: PathBuf,
    armed: bool,
}

#[cfg(unix)]
impl TempCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(unix)]
impl Drop for TempCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use pulsedag_core::address_from_public_key;

    use crate::{
        decrypt_private_key, encrypt_private_key, WalletKeystoreCryptoError, WalletSecretKey,
        ED25519_SECRET_KEY_BYTES,
    };

    const OLD_PASSWORD: &str = "old-test-password";
    const NEW_PASSWORD: &str = "new-test-password";

    fn test_directory(label: &str) -> PathBuf {
        let mut random = [0_u8; 8];
        OsRng.fill_bytes(&mut random);
        let path = std::env::temp_dir().join(format!(
            "pulsedag-wallet-rotation-{label}-{}-{}",
            std::process::id(),
            hex::encode(random)
        ));
        fs::create_dir(&path).expect("create test directory");
        path
    }

    fn test_secret_and_address() -> (WalletSecretKey, String) {
        let bytes = [0x5au8; ED25519_SECRET_KEY_BYTES];
        let signing_key = SigningKey::from_bytes(&bytes);
        let address = address_from_public_key(&hex::encode(signing_key.verifying_key().to_bytes()));
        (WalletSecretKey::from_bytes(bytes), address)
    }

    fn create_fixture(path: &Path) -> WalletKeystoreFile {
        let (secret, address) = test_secret_and_address();
        let envelope = encrypt_private_key(
            "public-testnet-v2.4.0-candidate",
            "pulsedag-public-testnet-v2.4.0-candidate",
            &address,
            &secret,
            &SecretString::new(OLD_PASSWORD),
        )
        .expect("encrypt fixture");
        let session = WalletKeystoreFile::try_acquire(path).expect("acquire fixture lock");
        session.create_new(&envelope).expect("persist fixture");
        session
    }

    #[test]
    fn rotates_password_without_changing_wallet_identity() {
        let directory = test_directory("success");
        let path = directory.join("wallet.json");
        let session = create_fixture(&path);
        let before = session.load().expect("load original");

        let report = rotate_keystore_password(
            &session,
            &SecretString::new(OLD_PASSWORD),
            &SecretString::new(NEW_PASSWORD),
        )
        .expect("rotate password");
        assert_eq!(
            report.file_permissions,
            WalletKeystorePermissionStatus::EnforcedOwnerReadWrite
        );
        assert_eq!(
            report.parent_directory_sync,
            WalletKeystoreDirectorySyncStatus::Synced
        );

        let after = session.load().expect("load replacement");
        assert_eq!(after.network_profile, before.network_profile);
        assert_eq!(after.chain_id, before.chain_id);
        assert_eq!(after.address, before.address);
        assert_ne!(after.kdf.salt_hex, before.kdf.salt_hex);
        assert_ne!(after.cipher.nonce_hex, before.cipher.nonce_hex);
        assert_ne!(after.ciphertext_hex, before.ciphertext_hex);

        let recovered = decrypt_private_key(&after, &SecretString::new(NEW_PASSWORD))
            .expect("new password decrypts");
        assert_eq!(recovered.expose_secret(), &[0x5au8; ED25519_SECRET_KEY_BYTES]);
        assert!(matches!(
            decrypt_private_key(&after, &SecretString::new(OLD_PASSWORD)),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        ));

        drop(session);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn wrong_current_password_leaves_live_file_unchanged() {
        let directory = test_directory("wrong-current");
        let path = directory.join("wallet.json");
        let session = create_fixture(&path);
        let before = fs::read(&path).expect("read original bytes");

        assert!(matches!(
            rotate_keystore_password(
                &session,
                &SecretString::new("wrong-password"),
                &SecretString::new(NEW_PASSWORD),
            ),
            Err(WalletKeystoreRotationError::Crypto(
                WalletKeystoreCryptoError::AuthenticationFailed
            ))
        ));
        assert_eq!(fs::read(&path).expect("read after failure"), before);

        drop(session);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn empty_new_password_leaves_live_file_unchanged() {
        let directory = test_directory("empty-new");
        let path = directory.join("wallet.json");
        let session = create_fixture(&path);
        let before = fs::read(&path).expect("read original bytes");

        assert!(matches!(
            rotate_keystore_password(
                &session,
                &SecretString::new(OLD_PASSWORD),
                &SecretString::new(""),
            ),
            Err(WalletKeystoreRotationError::Crypto(
                WalletKeystoreCryptoError::EmptyPassword
            ))
        ));
        assert_eq!(fs::read(&path).expect("read after failure"), before);

        drop(session);
        let _ = fs::remove_dir_all(directory);
    }
}
