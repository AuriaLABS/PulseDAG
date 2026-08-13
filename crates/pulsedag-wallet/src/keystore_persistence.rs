use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use fs4::{FileExt, TryLockError};
use rand::{rngs::OsRng, RngCore};

use crate::{WalletKeystoreEnvelope, WalletKeystoreFormatError};

pub const KEYSTORE_FILE_MAX_BYTES: u64 = 64 * 1024;
const TEMP_ATTEMPTS: usize = 32;

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
    RandomnessUnavailable,
    TooLarge { limit: u64, actual: u64 },
    Format(WalletKeystoreFormatError),
    Json(serde_json::Error),
    Io(&'static str, io::Error),
}

impl fmt::Display for WalletKeystorePersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(reason) => write!(f, "invalid wallet keystore path: {reason}"),
            Self::UnsafePath(reason) => write!(f, "unsafe wallet keystore path: {reason}"),
            Self::Locked => f.write_str("wallet keystore is locked by another process"),
            Self::AlreadyExists => f.write_str("wallet keystore already exists; overwrite refused"),
            Self::RandomnessUnavailable => f.write_str("operating-system randomness is unavailable"),
            Self::TooLarge { limit, actual } => {
                write!(f, "wallet keystore too large ({actual} bytes > {limit} bytes)")
            }
            Self::Format(error) => write!(f, "invalid wallet keystore structure: {error}"),
            Self::Json(_) => f.write_str("wallet keystore JSON is invalid"),
            Self::Io(operation, _) => write!(f, "wallet keystore I/O failed during {operation}"),
        }
    }
}

impl Error for WalletKeystorePersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(_, error) => Some(error),
            _ => None,
        }
    }
}

impl From<WalletKeystoreFormatError> for WalletKeystorePersistenceError {
    fn from(value: WalletKeystoreFormatError) -> Self {
        Self::Format(value)
    }
}

/// Holds an exclusive OS advisory lock for one keystore path until dropped.
pub struct WalletKeystoreFile {
    path: PathBuf,
    parent: PathBuf,
    _lock: File,
}

impl fmt::Debug for WalletKeystoreFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletKeystoreFile")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl WalletKeystoreFile {
    pub fn try_acquire(path: impl AsRef<Path>) -> Result<Self, WalletKeystorePersistenceError> {
        let path = path.as_ref();
        let name = path.file_name().filter(|name| !name.is_empty()).ok_or(
            WalletKeystorePersistenceError::InvalidPath("a file name is required"),
        )?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if !fs::metadata(parent).map_err(|e| ioerr("inspect parent", e))?.is_dir() {
            return Err(WalletKeystorePersistenceError::InvalidPath(
                "parent must be an existing directory",
            ));
        }
        reject_symlink(path, "keystore target must not be a symbolic link")?;

        let lock_path = control_path(parent, name, ".lock");
        reject_symlink(&lock_path, "lock file must not be a symbolic link")?;
        let lock = open_private(&lock_path, false).map_err(|e| ioerr("open lock file", e))?;
        enforce_private_permissions(&lock).map_err(|e| ioerr("secure lock file", e))?;
        match FileExt::try_lock(&lock) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(WalletKeystorePersistenceError::Locked),
            Err(TryLockError::Error(e)) => return Err(ioerr("acquire lock", e)),
        }
        Ok(Self {
            path: path.to_path_buf(),
            parent: parent.to_path_buf(),
            _lock: lock,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<WalletKeystoreEnvelope, WalletKeystorePersistenceError> {
        reject_symlink(&self.path, "keystore target must not be a symbolic link")?;
        let mut file = File::open(&self.path).map_err(|e| ioerr("open keystore", e))?;
        let metadata = file.metadata().map_err(|e| ioerr("inspect keystore", e))?;
        if !metadata.is_file() {
            return Err(WalletKeystorePersistenceError::UnsafePath(
                "keystore target must be a regular file",
            ));
        }
        ensure_size(metadata.len())?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        Read::by_ref(&mut file)
            .take(KEYSTORE_FILE_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ioerr("read keystore", e))?;
        ensure_size(bytes.len() as u64)?;
        let envelope = serde_json::from_slice::<WalletKeystoreEnvelope>(&bytes)
            .map_err(WalletKeystorePersistenceError::Json)?;
        envelope.validate_structure()?;
        Ok(envelope)
    }

    /// Atomically publishes a brand-new keystore. Existing targets are never
    /// replaced; replacement/password-rotation semantics are a separate API.
    pub fn create_new(
        &self,
        envelope: &WalletKeystoreEnvelope,
    ) -> Result<WalletKeystorePersistenceReport, WalletKeystorePersistenceError> {
        envelope.validate_structure()?;
        if self.target_exists()? {
            return Err(WalletKeystorePersistenceError::AlreadyExists);
        }
        let mut payload = serde_json::to_vec_pretty(envelope)
            .map_err(WalletKeystorePersistenceError::Json)?;
        payload.push(b'\n');
        ensure_size(payload.len() as u64)?;

        let (temp_path, mut temp) = self.create_temp()?;
        let mut cleanup = TempCleanup::new(temp_path.clone());
        temp.write_all(&payload)
            .map_err(|e| ioerr("write temporary keystore", e))?;
        temp.sync_all()
            .map_err(|e| ioerr("sync temporary keystore", e))?;
        let permissions =
            enforce_private_permissions(&temp).map_err(|e| ioerr("secure keystore", e))?;
        temp.sync_all()
            .map_err(|e| ioerr("sync keystore permissions", e))?;
        drop(temp);

        if self.target_exists()? {
            return Err(WalletKeystorePersistenceError::AlreadyExists);
        }
        fs::rename(&temp_path, &self.path).map_err(|e| ioerr("publish keystore", e))?;
        cleanup.disarm();
        let directory_sync =
            sync_parent(&self.parent).map_err(|e| ioerr("sync keystore directory", e))?;
        Ok(WalletKeystorePersistenceReport {
            file_permissions: permissions,
            parent_directory_sync: directory_sync,
        })
    }

    fn target_exists(&self) -> Result<bool, WalletKeystorePersistenceError> {
        match fs::symlink_metadata(&self.path) {
            Ok(meta) if meta.file_type().is_symlink() => Err(
                WalletKeystorePersistenceError::UnsafePath(
                    "keystore target must not be a symbolic link",
                ),
            ),
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(ioerr("inspect keystore target", e)),
        }
    }

    fn create_temp(&self) -> Result<(PathBuf, File), WalletKeystorePersistenceError> {
        let name = self.path.file_name().ok_or(
            WalletKeystorePersistenceError::InvalidPath("a file name is required"),
        )?;
        for _ in 0..TEMP_ATTEMPTS {
            let mut random = [0_u8; 8];
            OsRng
                .try_fill_bytes(&mut random)
                .map_err(|_| WalletKeystorePersistenceError::RandomnessUnavailable)?;
            let temp_path = control_path(
                &self.parent,
                name,
                &format!(".tmp-{}", hex::encode(random)),
            );
            match open_private(&temp_path, true) {
                Ok(file) => return Ok((temp_path, file)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(ioerr("create temporary keystore", e)),
            }
        }
        Err(ioerr(
            "allocate temporary keystore name",
            io::Error::new(io::ErrorKind::AlreadyExists, "temporary name collisions"),
        ))
    }
}

fn ensure_size(actual: u64) -> Result<(), WalletKeystorePersistenceError> {
    if actual > KEYSTORE_FILE_MAX_BYTES {
        return Err(WalletKeystorePersistenceError::TooLarge {
            limit: KEYSTORE_FILE_MAX_BYTES,
            actual,
        });
    }
    Ok(())
}

fn control_path(parent: &Path, name: &OsStr, suffix: &str) -> PathBuf {
    let mut control = OsString::from(".");
    control.push(name);
    control.push(suffix);
    parent.join(control)
}

fn reject_symlink(path: &Path, reason: &'static str) -> Result<(), WalletKeystorePersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(WalletKeystorePersistenceError::UnsafePath(reason))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ioerr("inspect filesystem path", e)),
    }
}

fn ioerr(operation: &'static str, error: io::Error) -> WalletKeystorePersistenceError {
    WalletKeystorePersistenceError::Io(operation, error)
}

fn open_private(path: &Path, create_new: bool) -> io::Result<File> {
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
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)?;
    if file.metadata()?.mode() & 0o777 != 0o600 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "keystore permissions are not 0600",
        ));
    }
    Ok(WalletKeystorePermissionStatus::EnforcedOwnerReadWrite)
}

#[cfg(not(unix))]
fn enforce_private_permissions(_file: &File) -> io::Result<WalletKeystorePermissionStatus> {
    Ok(WalletKeystorePermissionStatus::NotEnforcedOnThisPlatform)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<WalletKeystoreDirectorySyncStatus> {
    File::open(parent)?.sync_all()?;
    Ok(WalletKeystoreDirectorySyncStatus::Synced)
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> io::Result<WalletKeystoreDirectorySyncStatus> {
    Ok(WalletKeystoreDirectorySyncStatus::NotSupportedOnThisPlatform)
}

struct TempCleanup {
    path: PathBuf,
    armed: bool,
}

impl TempCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempCleanup {
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

    fn envelope() -> WalletKeystoreEnvelope {
        WalletKeystoreEnvelope {
            format: KEYSTORE_FORMAT.into(),
            version: KEYSTORE_VERSION,
            network_profile: "public-testnet-v2.4.0-candidate".into(),
            chain_id: "pulsedag-public-testnet-v2.4.0-candidate".into(),
            address: "pulse1persistencefixture".into(),
            kdf: WalletKdfMetadata {
                algorithm: KEYSTORE_KDF_ARGON2ID.into(),
                memory_kib: KEYSTORE_KDF_DEFAULT_MEMORY_KIB,
                iterations: KEYSTORE_KDF_DEFAULT_ITERATIONS,
                lanes: KEYSTORE_KDF_DEFAULT_LANES,
                salt_hex: "11".repeat(KEYSTORE_SALT_BYTES),
            },
            cipher: WalletCipherMetadata {
                algorithm: KEYSTORE_CIPHER_XCHACHA20_POLY1305.into(),
                nonce_hex: "22".repeat(KEYSTORE_NONCE_BYTES),
            },
            ciphertext_hex: "33".repeat(KEYSTORE_V1_CIPHERTEXT_BYTES),
        }
    }

    fn dir(label: &str) -> PathBuf {
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

    #[test]
    fn create_load_roundtrip_and_refuse_overwrite() {
        let directory = dir("roundtrip");
        let path = directory.join("wallet.json");
        let session = WalletKeystoreFile::try_acquire(&path).expect("lock");
        let expected = envelope();
        session.create_new(&expected).expect("create");
        assert_eq!(session.load().expect("load"), expected);
        assert!(matches!(
            session.create_new(&envelope()),
            Err(WalletKeystorePersistenceError::AlreadyExists)
        ));
        drop(session);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn concurrent_session_is_rejected_until_drop() {
        let directory = dir("locking");
        let path = directory.join("wallet.json");
        let first = WalletKeystoreFile::try_acquire(&path).expect("first lock");
        assert!(matches!(
            WalletKeystoreFile::try_acquire(&path),
            Err(WalletKeystorePersistenceError::Locked)
        ));
        drop(first);
        let second = WalletKeystoreFile::try_acquire(&path).expect("lock released");
        drop(second);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn malformed_and_oversized_files_fail_closed() {
        let directory = dir("invalid");
        let malformed_path = directory.join("malformed.json");
        fs::write(&malformed_path, b"{bad-json").expect("write malformed");
        let malformed = WalletKeystoreFile::try_acquire(&malformed_path).expect("lock");
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
        .expect("write oversized");
        let oversized = WalletKeystoreFile::try_acquire(&oversized_path).expect("lock");
        assert!(matches!(
            oversized.load(),
            Err(WalletKeystorePersistenceError::TooLarge { .. })
        ));
        drop(oversized);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn temp_cleanup_removes_unpublished_file() {
        let directory = dir("cleanup");
        let path = directory.join("temp");
        fs::write(&path, b"ciphertext").expect("write temp");
        { let _cleanup = TempCleanup::new(path.clone()); }
        assert!(!path.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn published_keystore_is_mode_0600_and_directory_synced() {
        use std::os::unix::fs::PermissionsExt;
        let directory = dir("permissions");
        let path = directory.join("wallet.json");
        let session = WalletKeystoreFile::try_acquire(&path).expect("lock");
        let report = session.create_new(&envelope()).expect("create");
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            report.file_permissions,
            WalletKeystorePermissionStatus::EnforcedOwnerReadWrite
        );
        assert_eq!(
            report.parent_directory_sync,
            WalletKeystoreDirectorySyncStatus::Synced
        );
        drop(session);
        let _ = fs::remove_dir_all(directory);
    }
}
