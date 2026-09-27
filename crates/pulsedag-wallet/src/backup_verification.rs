use std::{
    error::Error,
    ffi::OsString,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use pulsedag_core::address_from_public_key;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::{
    verify_watch_only_manifest, WalletDerivationBranch, WalletSession, WalletSessionError,
    WalletSessionIdentity, WalletWatchOnlyManifest, WalletWatchOnlyOperationError,
    WALLET_DERIVATION_MAX_INDEX,
};

pub const WALLET_BACKUP_VERIFICATION_FORMAT: &str = "pulsedag-wallet-backup-verification";
pub const WALLET_BACKUP_VERIFICATION_VERSION: u32 = 1;
pub const WALLET_BACKUP_VERIFICATION_DOMAIN_V1: &[u8] =
    b"PulseDAG:wallet-backup-verification:v1";
pub const WALLET_BACKUP_VERIFICATION_MAX_BYTES: u64 = 16 * 1024;
const TEMP_ATTEMPTS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletBackupVerificationReceipt {
    pub format: String,
    pub version: u32,
    pub network_profile: String,
    pub chain_id: String,
    pub wallet_anchor_address: String,
    pub manifest_checksum_hex: String,
    pub manifest_account: u32,
    pub signer_public_key_hex: String,
    pub signature_hex: String,
}

#[derive(Debug)]
pub enum WalletBackupVerificationError {
    InvalidReceipt {
        field: &'static str,
        reason: &'static str,
    },
    IdentityMismatch,
    AnchorSignerMismatch,
    InvalidSignature,
    AlreadyExists,
    InvalidPath(&'static str),
    UnsafePath(&'static str),
    TooLarge {
        limit: u64,
        actual: u64,
    },
    RandomnessUnavailable,
    WatchOnly(WalletWatchOnlyOperationError),
    Session(WalletSessionError),
    Json(serde_json::Error),
    Io(&'static str, io::Error),
}

impl fmt::Display for WalletBackupVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReceipt { field, reason } => {
                write!(f, "invalid wallet backup verification receipt field {field}: {reason}")
            }
            Self::IdentityMismatch => {
                f.write_str("wallet backup verification receipt does not match unlocked wallet identity")
            }
            Self::AnchorSignerMismatch => {
                f.write_str("wallet backup verification signer does not control the wallet anchor")
            }
            Self::InvalidSignature => {
                f.write_str("wallet backup verification receipt signature is invalid")
            }
            Self::AlreadyExists => {
                f.write_str("wallet backup verification receipt already exists")
            }
            Self::InvalidPath(reason) => {
                write!(f, "invalid wallet backup verification receipt path: {reason}")
            }
            Self::UnsafePath(reason) => {
                write!(f, "unsafe wallet backup verification receipt path: {reason}")
            }
            Self::TooLarge { limit, actual } => {
                write!(
                    f,
                    "wallet backup verification receipt too large ({actual} bytes > {limit} bytes)"
                )
            }
            Self::RandomnessUnavailable => {
                f.write_str("operating-system randomness is unavailable")
            }
            Self::WatchOnly(error) => write!(f, "wallet backup verification failed: {error}"),
            Self::Session(error) => write!(f, "wallet backup verification session failed: {error}"),
            Self::Json(_) => f.write_str("wallet backup verification receipt JSON is invalid"),
            Self::Io(operation, _) => {
                write!(f, "wallet backup verification receipt I/O failed during {operation}")
            }
        }
    }
}

impl Error for WalletBackupVerificationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WatchOnly(error) => Some(error),
            Self::Session(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(_, error) => Some(error),
            _ => None,
        }
    }
}

impl From<WalletWatchOnlyOperationError> for WalletBackupVerificationError {
    fn from(value: WalletWatchOnlyOperationError) -> Self {
        Self::WatchOnly(value)
    }
}

impl From<WalletSessionError> for WalletBackupVerificationError {
    fn from(value: WalletSessionError) -> Self {
        Self::Session(value)
    }
}

pub fn create_wallet_backup_verification_receipt(
    session: &WalletSession,
    manifest: &WalletWatchOnlyManifest,
) -> Result<WalletBackupVerificationReceipt, WalletBackupVerificationError> {
    verify_watch_only_manifest(session, manifest)?;
    let identity = session
        .status()
        .identity
        .ok_or(WalletBackupVerificationError::Session(
            WalletSessionError::Locked,
        ))?;
    let message = canonical_receipt_message(
        &identity.network_profile,
        &identity.chain_id,
        &identity.address,
        manifest.checksum_hex(),
        manifest.account(),
    )?;

    let (signer_address, signer_public_key_hex, signature_hex) = session.with_derived_key(
        0,
        WalletDerivationBranch::Receive,
        0,
        |derived| {
            let signing_key = SigningKey::from_bytes(derived.secret_key().expose_secret());
            (
                derived.address().to_string(),
                derived.public_key_hex().to_string(),
                hex::encode(signing_key.sign(&message).to_bytes()),
            )
        },
    )?;
    if signer_address != identity.address {
        return Err(WalletBackupVerificationError::AnchorSignerMismatch);
    }

    let receipt = WalletBackupVerificationReceipt {
        format: WALLET_BACKUP_VERIFICATION_FORMAT.to_string(),
        version: WALLET_BACKUP_VERIFICATION_VERSION,
        network_profile: identity.network_profile,
        chain_id: identity.chain_id,
        wallet_anchor_address: identity.address,
        manifest_checksum_hex: manifest.checksum_hex().to_string(),
        manifest_account: manifest.account(),
        signer_public_key_hex,
        signature_hex,
    };
    verify_wallet_backup_verification_receipt(&receipt, &receipt_identity(&receipt))?;
    Ok(receipt)
}

pub fn verify_wallet_backup_verification_receipt(
    receipt: &WalletBackupVerificationReceipt,
    expected_identity: &WalletSessionIdentity,
) -> Result<(), WalletBackupVerificationError> {
    validate_receipt_structure(receipt)?;
    if receipt.network_profile != expected_identity.network_profile
        || receipt.chain_id != expected_identity.chain_id
        || receipt.wallet_anchor_address != expected_identity.address
    {
        return Err(WalletBackupVerificationError::IdentityMismatch);
    }
    if address_from_public_key(&receipt.signer_public_key_hex) != receipt.wallet_anchor_address {
        return Err(WalletBackupVerificationError::AnchorSignerMismatch);
    }

    let public_key: [u8; 32] = hex::decode(&receipt.signer_public_key_hex)
        .map_err(|_| invalid("signer_public_key_hex", "must be canonical hexadecimal"))?
        .try_into()
        .map_err(|_| invalid("signer_public_key_hex", "must encode exactly 32 bytes"))?;
    let signature: [u8; 64] = hex::decode(&receipt.signature_hex)
        .map_err(|_| invalid("signature_hex", "must be canonical hexadecimal"))?
        .try_into()
        .map_err(|_| invalid("signature_hex", "must encode exactly 64 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| invalid("signer_public_key_hex", "is not a valid Ed25519 public key"))?;
    let message = canonical_receipt_message(
        &receipt.network_profile,
        &receipt.chain_id,
        &receipt.wallet_anchor_address,
        &receipt.manifest_checksum_hex,
        receipt.manifest_account,
    )?;
    verifying_key
        .verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| WalletBackupVerificationError::InvalidSignature)
}

pub fn wallet_backup_verification_receipt_path(
    keystore_path: &Path,
) -> Result<PathBuf, WalletBackupVerificationError> {
    let name = keystore_path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(WalletBackupVerificationError::InvalidPath(
            "keystore file name is required",
        ))?;
    let parent = keystore_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut receipt_name = OsString::from(name);
    receipt_name.push(".backup-verified.json");
    Ok(parent.join(receipt_name))
}

pub fn load_wallet_backup_verification_receipt(
    keystore_path: &Path,
) -> Result<WalletBackupVerificationReceipt, WalletBackupVerificationError> {
    let path = wallet_backup_verification_receipt_path(keystore_path)?;
    reject_symlink(&path)?;
    let mut file = File::open(&path).map_err(|error| ioerr("open receipt", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| ioerr("inspect receipt", error))?;
    if !metadata.is_file() {
        return Err(WalletBackupVerificationError::UnsafePath(
            "receipt target must be a regular file",
        ));
    }
    ensure_size(metadata.len())?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take(WALLET_BACKUP_VERIFICATION_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ioerr("read receipt", error))?;
    ensure_size(bytes.len() as u64)?;
    let receipt = serde_json::from_slice::<WalletBackupVerificationReceipt>(&bytes)
        .map_err(WalletBackupVerificationError::Json)?;
    validate_receipt_structure(&receipt)?;
    Ok(receipt)
}

pub fn persist_wallet_backup_verification_receipt(
    keystore_path: &Path,
    receipt: &WalletBackupVerificationReceipt,
) -> Result<PathBuf, WalletBackupVerificationError> {
    validate_receipt_structure(receipt)?;
    let path = wallet_backup_verification_receipt_path(keystore_path)?;
    reject_symlink(&path)?;
    match fs::metadata(&path) {
        Ok(_) => return Err(WalletBackupVerificationError::AlreadyExists),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ioerr("inspect receipt target", error)),
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !fs::metadata(parent)
        .map_err(|error| ioerr("inspect receipt parent", error))?
        .is_dir()
    {
        return Err(WalletBackupVerificationError::InvalidPath(
            "receipt parent must be an existing directory",
        ));
    }

    let mut payload =
        serde_json::to_vec_pretty(receipt).map_err(WalletBackupVerificationError::Json)?;
    payload.push(b'\n');
    ensure_size(payload.len() as u64)?;

    for _ in 0..TEMP_ATTEMPTS {
        let mut random = [0_u8; 8];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| WalletBackupVerificationError::RandomnessUnavailable)?;
        let mut temp_name = path
            .file_name()
            .ok_or(WalletBackupVerificationError::InvalidPath(
                "receipt file name is required",
            ))?
            .to_os_string();
        temp_name.push(format!(".tmp-{}", hex::encode(random)));
        let temp_path = parent.join(temp_name);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temp_path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ioerr("create temporary receipt", error)),
        };
        if let Err(error) = file.write_all(&payload) {
            let _ = fs::remove_file(&temp_path);
            return Err(ioerr("write temporary receipt", error));
        }
        if let Err(error) = file.sync_all() {
            let _ = fs::remove_file(&temp_path);
            return Err(ioerr("sync temporary receipt", error));
        }
        drop(file);

        if let Err(error) = fs::rename(&temp_path, &path) {
            let _ = fs::remove_file(&temp_path);
            return Err(ioerr("publish receipt", error));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| ioerr("secure receipt", error))?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| ioerr("sync receipt directory", error))?;
        }

        return Ok(path);
    }

    Err(ioerr(
        "allocate temporary receipt name",
        io::Error::new(io::ErrorKind::AlreadyExists, "temporary name collisions"),
    ))
}

fn validate_receipt_structure(
    receipt: &WalletBackupVerificationReceipt,
) -> Result<(), WalletBackupVerificationError> {
    if receipt.format != WALLET_BACKUP_VERIFICATION_FORMAT {
        return Err(invalid("format", "unsupported receipt format"));
    }
    if receipt.version != WALLET_BACKUP_VERIFICATION_VERSION {
        return Err(invalid("version", "unsupported receipt version"));
    }
    validate_text("network_profile", &receipt.network_profile)?;
    validate_text("chain_id", &receipt.chain_id)?;
    validate_text("wallet_anchor_address", &receipt.wallet_anchor_address)?;
    if receipt.manifest_account > WALLET_DERIVATION_MAX_INDEX {
        return Err(invalid(
            "manifest_account",
            "exceeds the hardened derivation range",
        ));
    }
    validate_hex_len(
        "manifest_checksum_hex",
        &receipt.manifest_checksum_hex,
        32,
    )?;
    validate_hex_len(
        "signer_public_key_hex",
        &receipt.signer_public_key_hex,
        32,
    )?;
    validate_hex_len("signature_hex", &receipt.signature_hex, 64)
}

fn canonical_receipt_message(
    network_profile: &str,
    chain_id: &str,
    wallet_anchor_address: &str,
    manifest_checksum_hex: &str,
    manifest_account: u32,
) -> Result<Vec<u8>, WalletBackupVerificationError> {
    validate_text("network_profile", network_profile)?;
    validate_text("chain_id", chain_id)?;
    validate_text("wallet_anchor_address", wallet_anchor_address)?;
    validate_hex_len("manifest_checksum_hex", manifest_checksum_hex, 32)?;

    let checksum = hex::decode(manifest_checksum_hex)
        .map_err(|_| invalid("manifest_checksum_hex", "must be canonical hexadecimal"))?;
    let mut out = Vec::with_capacity(192);
    encode_len_prefixed(&mut out, WALLET_BACKUP_VERIFICATION_DOMAIN_V1)?;
    out.extend_from_slice(&WALLET_BACKUP_VERIFICATION_VERSION.to_be_bytes());
    encode_len_prefixed(&mut out, network_profile.as_bytes())?;
    encode_len_prefixed(&mut out, chain_id.as_bytes())?;
    encode_len_prefixed(&mut out, wallet_anchor_address.as_bytes())?;
    out.extend_from_slice(&checksum);
    out.extend_from_slice(&manifest_account.to_be_bytes());
    Ok(out)
}

fn receipt_identity(receipt: &WalletBackupVerificationReceipt) -> WalletSessionIdentity {
    WalletSessionIdentity {
        network_profile: receipt.network_profile.clone(),
        chain_id: receipt.chain_id.clone(),
        address: receipt.wallet_anchor_address.clone(),
    }
}

fn encode_len_prefixed(
    out: &mut Vec<u8>,
    value: &[u8],
) -> Result<(), WalletBackupVerificationError> {
    let len = u32::try_from(value.len())
        .map_err(|_| invalid("canonical_message", "field length exceeds u32"))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
) -> Result<(), WalletBackupVerificationError> {
    if value.is_empty() || value.trim() != value {
        return Err(invalid(field, "must be non-empty canonical text"));
    }
    Ok(())
}

fn validate_hex_len(
    field: &'static str,
    value: &str,
    expected_bytes: usize,
) -> Result<(), WalletBackupVerificationError> {
    if value.len() != expected_bytes.saturating_mul(2) {
        return Err(invalid(field, "has an unexpected encoded length"));
    }
    let decoded =
        hex::decode(value).map_err(|_| invalid(field, "must be canonical hexadecimal"))?;
    if hex::encode(decoded) != value {
        return Err(invalid(field, "must use canonical lowercase hexadecimal"));
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), WalletBackupVerificationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(
            WalletBackupVerificationError::UnsafePath("receipt target must not be a symbolic link"),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ioerr("inspect receipt path", error)),
    }
}

fn ensure_size(actual: u64) -> Result<(), WalletBackupVerificationError> {
    if actual > WALLET_BACKUP_VERIFICATION_MAX_BYTES {
        return Err(WalletBackupVerificationError::TooLarge {
            limit: WALLET_BACKUP_VERIFICATION_MAX_BYTES,
            actual,
        });
    }
    Ok(())
}

fn invalid(
    field: &'static str,
    reason: &'static str,
) -> WalletBackupVerificationError {
    WalletBackupVerificationError::InvalidReceipt { field, reason }
}

fn ioerr(operation: &'static str, error: io::Error) -> WalletBackupVerificationError {
    WalletBackupVerificationError::Io(operation, error)
}

#[cfg(test)]
mod tests {
    use sha2::Digest;

    use super::*;

    fn signed_receipt(seed: u8, genesis_label: &str) -> WalletBackupVerificationReceipt {
        let signing_key = SigningKey::from_bytes(&[seed; 32]);
        let public_key = hex::encode(signing_key.verifying_key().to_bytes());
        let anchor = address_from_public_key(&public_key);
        let checksum = hex::encode(sha2::Sha256::digest(genesis_label.as_bytes()));
        let message = canonical_receipt_message(
            "testnet",
            "pulsedag-testnet",
            &anchor,
            &checksum,
            0,
        )
        .unwrap();
        WalletBackupVerificationReceipt {
            format: WALLET_BACKUP_VERIFICATION_FORMAT.to_string(),
            version: WALLET_BACKUP_VERIFICATION_VERSION,
            network_profile: "testnet".to_string(),
            chain_id: "pulsedag-testnet".to_string(),
            wallet_anchor_address: anchor,
            manifest_checksum_hex: checksum,
            manifest_account: 0,
            signer_public_key_hex: public_key,
            signature_hex: hex::encode(signing_key.sign(&message).to_bytes()),
        }
    }

    #[test]
    fn signed_receipt_binds_wallet_network_anchor_and_manifest() {
        let receipt = signed_receipt(7, "backup-a");
        let identity = receipt_identity(&receipt);
        verify_wallet_backup_verification_receipt(&receipt, &identity).unwrap();

        let mut tampered = receipt.clone();
        tampered.manifest_checksum_hex = hex::encode(sha2::Sha256::digest(b"backup-b"));
        assert!(matches!(
            verify_wallet_backup_verification_receipt(&tampered, &identity),
            Err(WalletBackupVerificationError::InvalidSignature)
        ));

        let foreign = WalletSessionIdentity {
            network_profile: identity.network_profile,
            chain_id: "pulsedag-mainnet".to_string(),
            address: identity.address,
        };
        assert!(matches!(
            verify_wallet_backup_verification_receipt(&receipt, &foreign),
            Err(WalletBackupVerificationError::IdentityMismatch)
        ));

        let substitute_key = SigningKey::from_bytes(&[8_u8; 32]);
        let mut substituted_signer = receipt.clone();
        substituted_signer.signer_public_key_hex =
            hex::encode(substitute_key.verifying_key().to_bytes());
        let substitute_message = canonical_receipt_message(
            &substituted_signer.network_profile,
            &substituted_signer.chain_id,
            &substituted_signer.wallet_anchor_address,
            &substituted_signer.manifest_checksum_hex,
            substituted_signer.manifest_account,
        )
        .unwrap();
        substituted_signer.signature_hex =
            hex::encode(substitute_key.sign(&substitute_message).to_bytes());
        assert!(matches!(
            verify_wallet_backup_verification_receipt(&substituted_signer, &receipt_identity(&receipt)),
            Err(WalletBackupVerificationError::AnchorSignerMismatch)
        ));
    }

    #[test]
    fn receipt_persistence_is_create_once_and_tamper_visible() {
        let mut random = [0_u8; 8];
        OsRng.fill_bytes(&mut random);
        let dir = std::env::temp_dir().join(format!(
            "pulsedag-backup-verification-{}-{}",
            std::process::id(),
            hex::encode(random)
        ));
        fs::create_dir(&dir).unwrap();
        let keystore = dir.join("wallet.json");
        fs::write(&keystore, b"fixture").unwrap();

        let receipt = signed_receipt(9, "backup");
        let path = persist_wallet_backup_verification_receipt(&keystore, &receipt).unwrap();
        assert_eq!(load_wallet_backup_verification_receipt(&keystore).unwrap(), receipt);
        assert!(matches!(
            persist_wallet_backup_verification_receipt(&keystore, &receipt),
            Err(WalletBackupVerificationError::AlreadyExists)
        ));

        fs::write(&path, b"{\"tampered\":true}").unwrap();
        assert!(load_wallet_backup_verification_receipt(&keystore).is_err());

        let _ = fs::remove_dir_all(dir);
    }
}
