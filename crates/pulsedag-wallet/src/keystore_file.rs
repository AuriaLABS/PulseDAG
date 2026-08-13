use std::{
    error::Error,
    ffi::OsString,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use fs4::{FileExt, TryLockError};
use rand::{rngs::OsRng, RngCore};

use crate::{WalletKeystoreEnvelope, WalletKeystoreFormatError};

pub const KEYSTORE_FILE_MAX_BYTES: u64 = 64 * 1024;
const TEMP_CREATE_ATTEMPTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletKeystorePermissionStatus {
    EnforcedOwnerReadWrite,
    NotEnforcedOnThisPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletKeystoreDirectorySyncStatus {
    Synced,
    NotSupportedOnThisPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalletKeystorePersistenceReport {
    pub file_permissions: WalletKeystorePermissionStatus,
    pub parent_directory_sync: WalletKeystoreDirectorySyncStatus,
}

#[derive(Debug)]
pub enum WalletKeystorePersistenceError {
    InvalidPath(&'static str),
    UnsafePath(&'static str),
    Locked,
    AlreadyExists,
    TooLarge { limit: u64, actual: u64 },
    Format(WalletKeystoreFormatError),
    Json(serde_json::Error),
    Io {
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for WalletKeystorePersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(reason) => write!(f, "invalid wallet keystore path: {reason}"),
            Self::UnsafePath(reason) => write!(f, "unsafe wallet keystore path: {reason}"),
            Self::Locked => f.write_str("wallet keystore is already locked by another process"),
            Self::AlreadyExists => f.write_str("wallet keystore already exists; overwrite is refused"),
            Self::TooLarge { limit, actual } => write!(
                f,
                "wallet keystore exceeds size limit ({actual} bytes > {limit} bytes)"
            ),
            Self::Format(error) => write!(f, "invalid wallet keystore structure: {error}"),
            Self::Json(_) => f.write_str("wallet keystore JSON is invalid"),
            Self::Io { operation, .. } => write!(f, "wallet keystore I/O failed during {operation}"),
        }
    }
}

impl Error for WalletKeystorePersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<WalletKeystoreFormatError> for WalletKeystorePersistenceError {
    fn from(value: WalletKeystoreFormatError) -> Self {
        Self::Format(value)
    }
}

/// Exclusive, process-scoped handle for one keystore path.
///
/// The adjacent lock file is intentionally persistent. Dropping this value
/// releases the OS advisory lock but does not delete the lock file, avoiding
/// unlink/recreate races between cooperating wallet processes.
pub struct WalletKeystoreFile {
    path: PathBuf,
    parent: PathBuf,
    _lock_file: File,
}

impl fmt::Debug for WalletKeystoreFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletKeystoreFile")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl WalletKeystoreFile {
    /// Acquire the exclusive lock for a keystore path without opening or
    /// decrypting the keystore itself. Fails immediately when another wallet
    /// process already holds the lock.
    pub fn try_acquire(path: impl AsRef<Path>) -> Result<Self, WalletKeystorePersistenceError> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or(WalletKeystorePersistenceError::InvalidPath(
                "a file name is required",
            ))?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        let parent_meta = fs::metadata(parent).map_err(|source| io_error("inspect parent", source))?;
        if !parent_meta.is_dir() {
            return Err(WalletKeystorePersistenceError::InvalidPath(
                "parent must be an existing directory",
            ));
        }

        reject_symlink_if_present(path, "keystore target must not be a symbolic link")?;

        let lock_path = sibling_control_path(parent, file_name, ".lock");
        reject_symlink_if_present(&lock_path, "lock file must not be a symbolic link")?;
        let lock_file = open_private_read_write_create(&lock_path, false)
            .map_err(|source| io_error("open lock file", source))?;
        enforce_private_permissions(&lock_file)
            .map_err(|source| io_error("set lock-file permissions", source))?;

        match FileExt::try_lock(&lock_file) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(WalletKeystorePersistenceError::Locked),
            Err(TryLockError::Error(source)) => {
                return Err(io_error("acquire keystore lock", source));
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            parent: parent.to_path_buf(),
            _lock_file: lock_file,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load and structurally validate the encrypted envelope while the
    /// exclusive keystore lock is held. The file is size-bounded before JSON
    /// parsing and symbolic-link targets are rejected.
    pub fn load(&self) -> Result<WalletKeystoreEnvelope, WalletKeystorePersistenceError> {
        reject_symlink_if_present(&self.path, "keystore target must not be a symbolic link")?;
        let mut file = File::open(&self.path).map_err(|source| io_error("open keystore", source))?;
        let metadata = file
            .metadata()
            .map_err(|source| io_error("inspect keystore", source))?;
        if !metadata.is_file() {
            return Err(WalletKeystorePersistenceError::UnsafePath(
                "keystore target must be a regular file",
            ));
        }
        if metadata.len() > KEYSTORE_FILE_MAX_BYTES {
            return Err(WalletKeystorePersistenceError::TooLarge {
                limit: KEYSTORE_FILE_MAX_BYTES,
                actual: metadata.len(),
            });
        }

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.by_ref()
            .take(KEYSTORE_FILE_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| io_error("read keystore", source))?;
        if bytes.len() as u64 > KEYSTORE_FILE_MAX_BYTES {
            return Err(WalletKeystorePersistenceError::TooLarge {
                limit: KEYSTORE_FILE_MAX_BYTES,
                actual: bytes.len() as u64,
            });
        }

        let envelope: WalletKeystoreEnvelope = serde_json::from_slice(&bytes)
            .map_err(WalletKeystorePersistenceError::Json)?;
        envelope.validate_structure()?;
        Ok(envelope)
    }

    /// Create a new keystore atomically. Existing targets are never replaced.
    ///
    /// The encrypted JSON is written to a private temporary file in the same
    /// directory, fully synced, renamed into place, and the parent directory is
    /// synced on Unix. A crash before rename leaves only an ignorable temp file;
    /// a crash after the completed rename leaves the fully synced envelope.
    pub fn create_new(
        &self,
        envelope: &WalletKeystoreEnvelope,
    ) -> Result<WalletKeystorePersistenceReport, WalletKeystorePersistenceError> {
        envelope.validate_structure()?;
        if self.path_exists_without_following()? {
            return Err(WalletKeystorePersistenceError::AlreadyExists);
        }

        let mut payload = serde_json::to_vec_pretty(envelope)
            .map_err(WalletKeystorePersistenceError::Json)?;
        payload.push(b'\n');
        if payload.len() as u64 > KEYSTORE_FILE_MAX_BYTES {
            return Err(WalletKeystorePersistenceError::TooLarge {
                limit: KEYSTORE_FILE_MAX_BYTES,
                actual: payload.len() as u64,
            });
        }

        let (temp_path, mut temp_file) = self.create_private_temp()?;
        let mut temp_guard = TempPathGuard::new(temp_path.clone());
        temp_file
            .write_all(&payload)
            .map_err(|source| io_error("write temporary keystore", source))?;
        temp_file
            .sync_all()
            .map_err(|source| io_error("sync temporary keystore", source))?;
        let permission_status = enforce_private_permissions(&temp_file)
            .map_err(|source| io_error("set keystore permissions", source))?;
        temp_file
            .sync_all()
            .map_err(|source| io_error("sync keystore permissions", source))?;
        drop(temp_file);

        if self.path_exists_without_following()? {
            return Err(WalletKeystorePersistenceError::AlreadyExists);
        }
        fs::rename(&temp_path, &self.path)
            .map_err(|source| io_error("publish keystore", source))?;
        temp_guard.disarm();

        let directory_status = sync_parent_directory(&self.parent)
            .map_err(|source| io_error("sync keystore directory", source))?;

        Ok(WalletKeystorePersistenceReport {
            file_permissions: permission_status,
            parent_directory_sync: directory_status,
        })
    }

    fn path_exists_without_following(&self) -> Result<bool, WalletKeystorePersistenceError> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(WalletKeystorePersistenceError::UnsafePath(
                        "keystore target must not be a symbolic link",
                    ));
                }
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(io_error("inspect keystore target", source)),
        }
    }

    fn create_private_temp(&self) -> Result<(PathBuf, File), WalletKeystorePersistenceError> {
        let file_name = self
            .path
            .file_name()
            .ok_or(WalletKeystorePersistenceError::InvalidPath(
                "a file name is required",
            ))?;

        for _ in 0..TEMP_CREATE_ATTEMPTS {
            let mut random = [0_u8; 8];
            OsRng
                .try_fill_bytes(&mut random)
                .map_err(|source| io_error("obtain temp-file randomness", source))?;
            let suffix = format!(".tmp-{}", hex::encode(random));
            let temp_path = sibling_control_path(&self.parent, file_name, &suffix);
            reject_symlink_if_present(&temp_path, "temporary path must not be a symbolic link")?;
            match open_private_read_write_create(&temp_path, true) {
                Ok(file) => return Ok((temp_path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(io_error("create temporary keystore", source)),
            }
        }

        Err(WalletKeystorePersistenceError::Io {
            operation: "allocate temporary keystore name",
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary keystore name collision limit reached",
            ),
        })
    }
}

fn sibling_control_path(parent: &Path, file_name: &std::ffi::OsStr, suffix: &str) -> PathBuf {
    let mut control_name = OsString::from(".");
    control_name.push(file_name);
    control_name.push(suffix);
    parent.join(control_name)
}

fn reject_symlink_if_present(
    path: &Path,
    reason: &'static str,
) -> Result<(), WalletKeystorePersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(WalletKeystorePersistenceError::UnsafePath(reason))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error("inspect filesystem path", source)),
    }
}

fn io_error(operation: &'static str, source: io::Error) -> WalletKeystorePersistenceError {
    WalletKeystorePersistenceError::Io { operation, source }
}

fn open_private_read_write_create(path: &Path, create_new: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options.open(path)
}

#[cfg(unix)]
fn enforce_private_permissions(file: &File) -> io::Result<WalletKeystorePermissionStatus> {
    use std::os::unix::fs::{PermissionsExt, MetadataExt};

    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)?;
    let mode = file.metadata()?.mode() & 0o777;
    if mode != 0o600 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "wallet keystore permissions are not owner-read/write only",
        ));
    }
    Ok(WalletKeystorePermissionStatus::EnforcedOwnerReadWrite)
}

#[cfg(not(unix))]
fn enforce_private_permissions(_file: &File) -> io::Result<WalletKeystorePermissionStatus> {
    Ok(WalletKeystorePermissionStatus::NotEnforcedOnThisPlatform)
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<WalletKeystoreDirectorySyncStatus> {
    File::open(parent)?.sync_all()?;
    Ok(WalletKeystoreDirectorySyncStatus::Synced)
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> io::Result<WalletKeystoreDirectorySyncStatus> {
    Ok(WalletKeystoreDirectorySyncStatus::NotSupportedOnThisPlatform)
}

struct TempPathGuard {
    path: PathBuf,
    armed: bool,
}

impl TempPathGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempPathGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        WalletCipherMetadata, WalletKdfMetadata, KEYSTORE_CIPHER_XCHACHA20_POLY1305,
        KEYSTORE_FORMAT, KEYSTORE_KDF_ARGON2ID, KEYSTORE_KDF_DEFAULT_ITERATIONS,
        KEYSTORE_KDF_DEFAULT_LANES, KEYSTORE_KDF_DEFAULT_MEMORY_KIB, KEYSTORE_NONCE_BYTES,
        KEYSTORE_SALT_BYTES, KEYSTORE_V1_CIPHERTEXT_BYTES, KEYSTORE_VERSION,
    };

    fn sample_envelope() -> WalletKeystoreEnvelope {
        WalletKeystoreEnvelope {
            format: KEYSTORE_FORMAT.to_string(),
            version: KEYSTORE_VERSION,
            network_profile: "public-testnet-v2.4.0-candidate".to_string(),
            chain_id: "pulsedag-public-testnet-v2.4.0-candidate".to_string(),
            address: "pulse1persistencefixture".to_string(),
            kdf: WalletKdfMetadata {
                algorithm: KEYSTORE_KDF_ARGON2ID.to_string(),
                memory_kib: KEYSTORE_KDF_DEFAULT_MEMORY_KIB,
                iterations: KEYSTORE_KDF_DEFAULT_ITERATIONS,
                lanes: KEYSTORE_KDF_DEFAULT_LANES,
                salt_hex: "11".repeat(KEYSTORE_SALT_BYTES),
            },
            cipher: WalletCipherMetadata {
                algorithm: KEYSTORE_CIPHER_XCHACHA20_POLY1305.to_string(),
                nonce_hex: "22".repeat(KEYSTORE_NONCE_BYTES),
            },
            ciphertext_hex: "33".repeat(KEYSTORE_V1_CIPHERTEXT_BYTES),
        }
    }

    fn test_directory(label: &str) -> PathBuf {
        let mut random = [0_u8; 8];
        OsRng.fill_bytes(&mut random);
        let path = std::env::temp_dir().join(format!(
            "pulsedag-wallet-{label}-{}-{}",
            std::process::id(),
            hex::encode(random)
        ));
        fs::create_dir(&path).expect("create test directory");
        path
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn create_new_and_load_round_trip_under_lock() {
        let directory = test_directory("roundtrip");
        let path = directory.join("wallet.json");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire lock");
        let envelope = sample_envelope();
        let report = file.create_new(&envelope).expect("create keystore");
        let loaded = file.load().expect("load keystore");
        assert_eq!(loaded, envelope);

        #[cfg(unix)]
        {
            assert_eq!(
                report.file_permissions,
                WalletKeystorePermissionStatus::EnforcedOwnerReadWrite
            );
            assert_eq!(
                report.parent_directory_sync,
                WalletKeystoreDirectorySyncStatus::Synced
            );
        }

        drop(file);
        cleanup(&directory);
    }

    #[test]
    fn second_session_fails_immediately_while_lock_is_held() {
        let directory = test_directory("locking");
        let path = directory.join("wallet.json");
        let first = WalletKeystoreFile::try_acquire(&path).expect("first lock");
        assert!(matches!(
            WalletKeystoreFile::try_acquire(&path),
            Err(WalletKeystorePersistenceError::Locked)
        ));
        drop(first);
        WalletKeystoreFile::try_acquire(&path).expect("lock released on drop");
        cleanup(&directory);
    }

    #[test]
    fn create_new_refuses_to_replace_existing_keystore() {
        let directory = test_directory("no-overwrite");
        let path = directory.join("wallet.json");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire lock");
        file.create_new(&sample_envelope()).expect("first create");
        assert!(matches!(
            file.create_new(&sample_envelope()),
            Err(WalletKeystorePersistenceError::AlreadyExists)
        ));
        cleanup(&directory);
    }

    #[test]
    fn malformed_and_oversized_files_fail_closed() {
        let directory = test_directory("malformed");
        let malformed_path = directory.join("malformed.json");
        fs::write(&malformed_path, b"{not-json").expect("write malformed file");
        let malformed = WalletKeystoreFile::try_acquire(&malformed_path).expect("acquire lock");
        assert!(matches!(
            malformed.load(),
            Err(WalletKeystorePersistenceError::Json(_))
        ));
        drop(malformed);

        let oversized_path = directory.join("oversized.json");
        fs::write(
            &oversized_path,
            vec![b'x'; (KEYSTORE_FILE_MAX_BYTES + 1) as usize],
        )
        .expect("write oversized file");
        let oversized = WalletKeystoreFile::try_acquire(&oversized_path).expect("acquire lock");
        assert!(matches!(
            oversized.load(),
            Err(WalletKeystorePersistenceError::TooLarge { .. })
        ));
        drop(oversized);
        cleanup(&directory);
    }

    #[test]
    fn temp_guard_removes_unpublished_file() {
        let directory = test_directory("temp-cleanup");
        let temp_path = directory.join("orphan.tmp");
        fs::write(&temp_path, b"ciphertext-only").expect("write temp");
        {
            let _guard = TempPathGuard::new(temp_path.clone());
        }
        assert!(!temp_path.exists());
        cleanup(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn published_keystore_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("permissions");
        let path = directory.join("wallet.json");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire lock");
        file.create_new(&sample_envelope()).expect("create keystore");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        drop(file);
        cleanup(&directory);
    }
}
