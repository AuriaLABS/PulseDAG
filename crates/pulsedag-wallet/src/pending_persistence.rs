use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use fs4::{FileExt, TryLockError};
use sha2::{Digest, Sha256};

use crate::{WalletNetworkIdentity, WalletPendingError, WalletPendingJournal};

pub const WALLET_PENDING_JOURNAL_MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_COMMITTED_GENERATIONS: usize = 1024;
const GENERATION_WIDTH: usize = 20;
const DIGEST_HEX_LEN: usize = 64;
const LOCK_FILE_NAME: &str = ".lock";
const SNAPSHOT_PREFIX: &str = "snapshot-";
const COMMIT_PREFIX: &str = "commit-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletPendingJournalSnapshot {
    pub generation: u64,
    pub journal: WalletPendingJournal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletPendingPersistenceReport {
    pub generation: u64,
    pub snapshot_digest_hex: String,
    pub cleanup_complete: bool,
}

#[derive(Debug)]
pub enum WalletPendingPersistenceError {
    InvalidPath(&'static str),
    UnsafePath(&'static str),
    Locked,
    TooLarge { limit: u64, actual: u64 },
    TooManyCommittedGenerations,
    InvalidCommitMarker,
    AmbiguousGeneration(u64),
    MissingCommittedSnapshot(u64),
    DigestMismatch(u64),
    StaleGeneration { expected: u64, actual: u64 },
    GenerationOverflow,
    Journal(WalletPendingError),
    Json(serde_json::Error),
    PublishedButDirectorySyncFailed(io::Error),
    Io(&'static str, io::Error),
}

impl fmt::Display for WalletPendingPersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(reason) => write!(f, "invalid wallet pending journal path: {reason}"),
            Self::UnsafePath(reason) => write!(f, "unsafe wallet pending journal path: {reason}"),
            Self::Locked => f.write_str("wallet pending journal is locked by another process"),
            Self::TooLarge { limit, actual } => write!(
                f,
                "wallet pending journal snapshot is too large ({actual} bytes > {limit} bytes)"
            ),
            Self::TooManyCommittedGenerations => {
                f.write_str("wallet pending journal has too many committed generations")
            }
            Self::InvalidCommitMarker => {
                f.write_str("wallet pending journal contains an invalid commit marker")
            }
            Self::AmbiguousGeneration(generation) => write!(
                f,
                "wallet pending journal contains multiple commits for generation {generation}"
            ),
            Self::MissingCommittedSnapshot(generation) => write!(
                f,
                "wallet pending journal committed generation {generation} is missing its snapshot"
            ),
            Self::DigestMismatch(generation) => write!(
                f,
                "wallet pending journal committed generation {generation} failed digest verification"
            ),
            Self::StaleGeneration { expected, actual } => write!(
                f,
                "wallet pending journal generation changed (expected {expected}, actual {actual})"
            ),
            Self::GenerationOverflow => f.write_str("wallet pending journal generation overflow"),
            Self::Journal(error) => write!(f, "wallet pending journal validation failed: {error}"),
            Self::Json(_) => f.write_str("wallet pending journal JSON is invalid"),
            Self::PublishedButDirectorySyncFailed(_) => f.write_str(
                "wallet pending journal generation was published but directory durability sync failed",
            ),
            Self::Io(operation, _) => {
                write!(f, "wallet pending journal I/O failed during {operation}")
            }
        }
    }
}

impl Error for WalletPendingPersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Journal(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::PublishedButDirectorySyncFailed(error) | Self::Io(_, error) => Some(error),
            _ => None,
        }
    }
}

impl From<WalletPendingError> for WalletPendingPersistenceError {
    fn from(value: WalletPendingError) -> Self {
        Self::Journal(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommitRef {
    generation: u64,
    digest_hex: String,
}

pub struct WalletPendingJournalStore {
    directory: PathBuf,
    _lock: File,
}

impl fmt::Debug for WalletPendingJournalStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletPendingJournalStore")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

impl WalletPendingJournalStore {
    pub fn try_acquire(path: impl AsRef<Path>) -> Result<Self, WalletPendingPersistenceError> {
        let directory = path.as_ref();
        if directory.as_os_str().is_empty() {
            return Err(WalletPendingPersistenceError::InvalidPath(
                "a journal directory is required",
            ));
        }
        ensure_store_directory(directory)?;

        let lock_path = directory.join(LOCK_FILE_NAME);
        reject_symlink(&lock_path, "journal lock file must not be a symbolic link")?;
        let lock = open_private_file(&lock_path, false)
            .map_err(|error| io_error("open lock file", error))?;
        enforce_private_file_permissions(&lock)
            .map_err(|error| io_error("secure lock file", error))?;
        match FileExt::try_lock(&lock) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(WalletPendingPersistenceError::Locked),
            Err(TryLockError::Error(error)) => return Err(io_error("acquire lock", error)),
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            _lock: lock,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load_latest(
        &self,
    ) -> Result<Option<WalletPendingJournalSnapshot>, WalletPendingPersistenceError> {
        let Some(reference) = self.latest_commit_ref()? else {
            return Ok(None);
        };
        let journal = self.load_committed(&reference)?;
        Ok(Some(WalletPendingJournalSnapshot {
            generation: reference.generation,
            journal,
        }))
    }

    pub fn load_or_new(
        &self,
        network: &WalletNetworkIdentity,
    ) -> Result<WalletPendingJournalSnapshot, WalletPendingPersistenceError> {
        network.validate().map_err(WalletPendingError::Plan)?;
        if let Some(snapshot) = self.load_latest()? {
            snapshot.journal.ensure_network(network)?;
            return Ok(snapshot);
        }
        Ok(WalletPendingJournalSnapshot {
            generation: 0,
            journal: WalletPendingJournal::new(network.clone())?,
        })
    }

    pub fn save_next(
        &self,
        expected_generation: u64,
        journal: &WalletPendingJournal,
    ) -> Result<WalletPendingPersistenceReport, WalletPendingPersistenceError> {
        journal.validate()?;
        let latest = self.load_latest()?;
        let actual_generation = latest.as_ref().map_or(0, |snapshot| snapshot.generation);
        if actual_generation != expected_generation {
            return Err(WalletPendingPersistenceError::StaleGeneration {
                expected: expected_generation,
                actual: actual_generation,
            });
        }
        if let Some(snapshot) = latest {
            snapshot.journal.ensure_network(&journal.network)?;
        }

        let generation = expected_generation
            .checked_add(1)
            .ok_or(WalletPendingPersistenceError::GenerationOverflow)?;
        let mut payload =
            serde_json::to_vec_pretty(journal).map_err(WalletPendingPersistenceError::Json)?;
        payload.push(b'\n');
        ensure_size(payload.len() as u64)?;
        let digest_hex = hex::encode(Sha256::digest(&payload));
        let snapshot_path = self.snapshot_path(generation, &digest_hex);
        let commit_path = self.commit_path(generation, &digest_hex);

        remove_uncommitted_target(&snapshot_path)?;
        if fs::symlink_metadata(&commit_path).is_ok() {
            return Err(WalletPendingPersistenceError::AmbiguousGeneration(
                generation,
            ));
        }

        let mut snapshot = open_private_file(&snapshot_path, true)
            .map_err(|error| io_error("create snapshot", error))?;
        snapshot
            .write_all(&payload)
            .map_err(|error| io_error("write snapshot", error))?;
        snapshot
            .sync_all()
            .map_err(|error| io_error("sync snapshot", error))?;
        enforce_private_file_permissions(&snapshot)
            .map_err(|error| io_error("secure snapshot", error))?;
        snapshot
            .sync_all()
            .map_err(|error| io_error("sync snapshot permissions", error))?;
        drop(snapshot);

        let marker = open_private_file(&commit_path, true)
            .map_err(|error| io_error("create commit marker", error))?;
        marker
            .sync_all()
            .map_err(|error| io_error("sync commit marker", error))?;
        enforce_private_file_permissions(&marker)
            .map_err(|error| io_error("secure commit marker", error))?;
        marker
            .sync_all()
            .map_err(|error| io_error("sync commit marker permissions", error))?;
        drop(marker);

        if let Err(error) = sync_directory(&self.directory) {
            return Err(WalletPendingPersistenceError::PublishedButDirectorySyncFailed(error));
        }

        let cleanup_complete = self.cleanup_old_generations(generation);
        Ok(WalletPendingPersistenceReport {
            generation,
            snapshot_digest_hex: digest_hex,
            cleanup_complete,
        })
    }

    fn latest_commit_ref(&self) -> Result<Option<CommitRef>, WalletPendingPersistenceError> {
        let mut committed = Vec::new();
        for entry in fs::read_dir(&self.directory)
            .map_err(|error| io_error("scan journal directory", error))?
        {
            let entry = entry.map_err(|error| io_error("read journal directory entry", error))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(reference) = parse_commit_name(&name)? else {
                continue;
            };
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| io_error("inspect commit marker", error))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != 0 {
                return Err(WalletPendingPersistenceError::InvalidCommitMarker);
            }
            committed.push(reference);
            if committed.len() > MAX_COMMITTED_GENERATIONS {
                return Err(WalletPendingPersistenceError::TooManyCommittedGenerations);
            }
        }
        committed.sort_by_key(|reference| reference.generation);
        for pair in committed.windows(2) {
            if pair[0].generation == pair[1].generation {
                return Err(WalletPendingPersistenceError::AmbiguousGeneration(
                    pair[0].generation,
                ));
            }
        }
        Ok(committed.pop())
    }

    fn load_committed(
        &self,
        reference: &CommitRef,
    ) -> Result<WalletPendingJournal, WalletPendingPersistenceError> {
        let path = self.snapshot_path(reference.generation, &reference.digest_hex);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(WalletPendingPersistenceError::MissingCommittedSnapshot(
                    reference.generation,
                ))
            }
            Err(error) => return Err(io_error("inspect committed snapshot", error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(WalletPendingPersistenceError::UnsafePath(
                "committed snapshot must be a regular file",
            ));
        }
        ensure_size(metadata.len())?;
        let mut file =
            File::open(&path).map_err(|error| io_error("open committed snapshot", error))?;
        let mut payload = Vec::with_capacity(metadata.len() as usize);
        Read::by_ref(&mut file)
            .take(WALLET_PENDING_JOURNAL_MAX_BYTES + 1)
            .read_to_end(&mut payload)
            .map_err(|error| io_error("read committed snapshot", error))?;
        ensure_size(payload.len() as u64)?;
        if hex::encode(Sha256::digest(&payload)) != reference.digest_hex {
            return Err(WalletPendingPersistenceError::DigestMismatch(
                reference.generation,
            ));
        }
        let journal = serde_json::from_slice::<WalletPendingJournal>(&payload)
            .map_err(WalletPendingPersistenceError::Json)?;
        journal.validate()?;
        Ok(journal)
    }

    fn snapshot_path(&self, generation: u64, digest_hex: &str) -> PathBuf {
        self.directory.join(format!(
            "{SNAPSHOT_PREFIX}{generation:0GENERATION_WIDTH$}-{digest_hex}.json"
        ))
    }

    fn commit_path(&self, generation: u64, digest_hex: &str) -> PathBuf {
        self.directory.join(format!(
            "{COMMIT_PREFIX}{generation:0GENERATION_WIDTH$}-{digest_hex}"
        ))
    }

    fn cleanup_old_generations(&self, current_generation: u64) -> bool {
        let minimum_to_keep = current_generation.saturating_sub(1);
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return false;
        };
        let mut complete = true;
        for entry in entries {
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let generation = parse_generation_from_snapshot_name(&name)
                .or_else(|| parse_generation_from_commit_name(&name));
            if generation.is_some_and(|generation| generation < minimum_to_keep)
                && fs::remove_file(entry.path()).is_err()
            {
                complete = false;
            }
        }
        complete
    }
}

fn parse_commit_name(name: &str) -> Result<Option<CommitRef>, WalletPendingPersistenceError> {
    if !name.starts_with(COMMIT_PREFIX) {
        return Ok(None);
    }
    let remainder = &name[COMMIT_PREFIX.len()..];
    let Some((generation, digest_hex)) = remainder.split_once('-') else {
        return Err(WalletPendingPersistenceError::InvalidCommitMarker);
    };
    let generation =
        parse_generation(generation).ok_or(WalletPendingPersistenceError::InvalidCommitMarker)?;
    validate_digest(digest_hex).ok_or(WalletPendingPersistenceError::InvalidCommitMarker)?;
    Ok(Some(CommitRef {
        generation,
        digest_hex: digest_hex.to_string(),
    }))
}

fn parse_generation_from_commit_name(name: &str) -> Option<u64> {
    let remainder = name.strip_prefix(COMMIT_PREFIX)?;
    let (generation, digest_hex) = remainder.split_once('-')?;
    validate_digest(digest_hex)?;
    parse_generation(generation)
}

fn parse_generation_from_snapshot_name(name: &str) -> Option<u64> {
    let remainder = name.strip_prefix(SNAPSHOT_PREFIX)?;
    let remainder = remainder.strip_suffix(".json")?;
    let (generation, digest_hex) = remainder.split_once('-')?;
    validate_digest(digest_hex)?;
    parse_generation(generation)
}

fn parse_generation(value: &str) -> Option<u64> {
    if value.len() != GENERATION_WIDTH || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn validate_digest(value: &str) -> Option<()> {
    if value.len() != DIGEST_HEX_LEN || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let decoded = hex::decode(value).ok()?;
    if decoded.len() != 32 || hex::encode(decoded) != value {
        return None;
    }
    Some(())
}

fn ensure_store_directory(path: &Path) -> Result<(), WalletPendingPersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(WalletPendingPersistenceError::UnsafePath(
                "journal directory must not be a symbolic link",
            ))
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(WalletPendingPersistenceError::InvalidPath(
                "journal path must be a directory",
            ))
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error("create journal directory", error)),
            }
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                sync_directory(parent).map_err(|error| io_error("sync journal parent", error))?;
            }
        }
        Err(error) => return Err(io_error("inspect journal directory", error)),
    }
    enforce_private_directory_permissions(path)
        .map_err(|error| io_error("secure journal directory", error))?;
    Ok(())
}

fn remove_uncommitted_target(path: &Path) -> Result<(), WalletPendingPersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(WalletPendingPersistenceError::UnsafePath(
                "uncommitted snapshot target must be a regular file",
            ))
        }
        Ok(_) => fs::remove_file(path).map_err(|error| io_error("remove orphan snapshot", error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect orphan snapshot", error)),
    }
}

fn reject_symlink(path: &Path, reason: &'static str) -> Result<(), WalletPendingPersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(WalletPendingPersistenceError::UnsafePath(reason))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect filesystem path", error)),
    }
}

fn ensure_size(actual: u64) -> Result<(), WalletPendingPersistenceError> {
    if actual > WALLET_PENDING_JOURNAL_MAX_BYTES {
        return Err(WalletPendingPersistenceError::TooLarge {
            limit: WALLET_PENDING_JOURNAL_MAX_BYTES,
            actual,
        });
    }
    Ok(())
}

fn io_error(operation: &'static str, error: io::Error) -> WalletPendingPersistenceError {
    WalletPendingPersistenceError::Io(operation, error)
}

fn open_private_file(path: &Path, create_new: bool) -> io::Result<File> {
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
fn enforce_private_file_permissions(file: &File) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)?;
    if file.metadata()?.mode() & 0o777 != 0o600 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "pending journal file permissions are not 0600",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_private_file_permissions(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn enforce_private_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    if fs::metadata(path)?.permissions().mode() & 0o777 != 0o700 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "pending journal directory permissions are not 0700",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_private_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "pending journal directory sync is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use pulsedag_core::{address_from_public_key, types::OutPoint};
    use rand::{rngs::OsRng, RngCore};

    use super::*;
    use crate::SelectedUtxo;

    fn network(chain_id: &str) -> WalletNetworkIdentity {
        WalletNetworkIdentity::new("public-testnet", chain_id).expect("network")
    }

    fn address() -> String {
        address_from_public_key(&"ab".repeat(32))
    }

    fn selected() -> SelectedUtxo {
        SelectedUtxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            amount: 100,
        }
    }

    fn directory(label: &str) -> PathBuf {
        let mut random = [0_u8; 8];
        OsRng.fill_bytes(&mut random);
        std::env::temp_dir().join(format!(
            "pulsedag-wallet-pending-{label}-{}-{}",
            std::process::id(),
            hex::encode(random)
        ))
    }

    #[test]
    fn committed_generation_survives_restart() {
        let path = directory("roundtrip");
        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
            let mut snapshot = store.load_or_new(&network("chain-a")).expect("load");
            snapshot
                .journal
                .reserve_signed("aa".repeat(32), address(), &[selected()])
                .expect("reserve");
            let report = store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save");
            assert_eq!(report.generation, 1);
        }
        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("reopen");
            let loaded = store.load_or_new(&network("chain-a")).expect("reload");
            assert_eq!(loaded.generation, 1);
            assert_eq!(loaded.journal.entries.len(), 1);
            assert_eq!(loaded.journal.reserved_outpoints().len(), 1);
        }
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn concurrent_open_is_rejected_until_store_drops() {
        let path = directory("locking");
        let first = WalletPendingJournalStore::try_acquire(&path).expect("first");
        assert!(matches!(
            WalletPendingJournalStore::try_acquire(&path),
            Err(WalletPendingPersistenceError::Locked)
        ));
        drop(first);
        WalletPendingJournalStore::try_acquire(&path).expect("second after drop");
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn orphan_snapshot_without_commit_marker_is_ignored() {
        let path = directory("orphan");
        let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
        let mut snapshot = store.load_or_new(&network("chain-a")).expect("load");
        snapshot
            .journal
            .reserve_signed("aa".repeat(32), address(), &[selected()])
            .expect("reserve");
        store
            .save_next(snapshot.generation, &snapshot.journal)
            .expect("save");

        let orphan_digest = "00".repeat(32);
        fs::write(store.snapshot_path(2, &orphan_digest), b"torn").expect("write orphan snapshot");
        let loaded = store.load_latest().expect("load latest").expect("snapshot");
        assert_eq!(loaded.generation, 1);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn tampered_committed_snapshot_fails_closed() {
        let path = directory("tamper");
        let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
        let mut snapshot = store.load_or_new(&network("chain-a")).expect("load");
        snapshot
            .journal
            .reserve_signed("aa".repeat(32), address(), &[selected()])
            .expect("reserve");
        let report = store
            .save_next(snapshot.generation, &snapshot.journal)
            .expect("save");
        fs::write(
            store.snapshot_path(report.generation, &report.snapshot_digest_hex),
            b"{}",
        )
        .expect("tamper");
        assert!(matches!(
            store.load_latest(),
            Err(WalletPendingPersistenceError::DigestMismatch(1))
        ));
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn stale_generation_and_cross_network_load_fail_closed() {
        let path = directory("stale");
        let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
        let mut snapshot = store.load_or_new(&network("chain-a")).expect("load");
        snapshot
            .journal
            .reserve_signed("aa".repeat(32), address(), &[selected()])
            .expect("reserve");
        store
            .save_next(snapshot.generation, &snapshot.journal)
            .expect("save");
        assert!(matches!(
            store.save_next(0, &snapshot.journal),
            Err(WalletPendingPersistenceError::StaleGeneration {
                expected: 0,
                actual: 1
            })
        ));
        assert!(store.load_or_new(&network("chain-b")).is_err());
        let _ = fs::remove_dir_all(path);
    }
}
