use std::{error::Error, fmt};

use pulsedag_core::address_from_public_key;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    deterministic::{
        derive_network_components, WalletDerivationBranch, WalletDerivedKey,
        WALLET_DERIVATION_DOMAIN, WALLET_DERIVATION_MAX_INDEX, WALLET_DERIVATION_VERSION,
    },
    session_clock::WalletSession,
    session_v1::{WalletSessionError, WalletSessionIdentity},
};

pub const WALLET_WATCH_ONLY_FORMAT: &str = "pulsedag-watch-only";
pub const WALLET_WATCH_ONLY_VERSION: u32 = 1;
pub const WALLET_WATCH_ONLY_MAX_ENTRIES: usize = 4_096;
const WATCH_ONLY_CHECKSUM_DOMAIN: &[u8] = b"PulseDAG:watch-only-manifest:v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WalletWatchOnlyBranch {
    Receive,
    Change,
}

impl WalletWatchOnlyBranch {
    fn derivation_branch(self) -> WalletDerivationBranch {
        match self {
            Self::Receive => WalletDerivationBranch::Receive,
            Self::Change => WalletDerivationBranch::Change,
        }
    }

    fn component(self) -> u32 {
        match self {
            Self::Receive => 0,
            Self::Change => 1,
        }
    }
}

impl From<WalletDerivationBranch> for WalletWatchOnlyBranch {
    fn from(value: WalletDerivationBranch) -> Self {
        match value {
            WalletDerivationBranch::Receive => Self::Receive,
            WalletDerivationBranch::Change => Self::Change,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalletWatchOnlyScope {
    account: u32,
    receive_count: u32,
    change_count: u32,
}

impl WalletWatchOnlyScope {
    pub fn new(
        account: u32,
        receive_count: u32,
        change_count: u32,
    ) -> Result<Self, WalletWatchOnlyError> {
        if account > WALLET_DERIVATION_MAX_INDEX {
            return Err(WalletWatchOnlyError::IndexOutOfRange("account"));
        }
        let total = (receive_count as usize)
            .checked_add(change_count as usize)
            .ok_or(WalletWatchOnlyError::TooManyEntries)?;
        if total == 0 {
            return Err(WalletWatchOnlyError::EmptyManifest);
        }
        if total > WALLET_WATCH_ONLY_MAX_ENTRIES {
            return Err(WalletWatchOnlyError::TooManyEntries);
        }
        Ok(Self {
            account,
            receive_count,
            change_count,
        })
    }

    pub fn account(self) -> u32 {
        self.account
    }

    pub fn receive_count(self) -> u32 {
        self.receive_count
    }

    pub fn change_count(self) -> u32 {
        self.change_count
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletWatchOnlyEntry {
    branch: WalletWatchOnlyBranch,
    index: u32,
    derivation_path: String,
    public_key_hex: String,
    address: String,
}

impl WalletWatchOnlyEntry {
    pub fn branch(&self) -> WalletWatchOnlyBranch {
        self.branch
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn derivation_path(&self) -> &str {
        &self.derivation_path
    }

    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletWatchOnlyManifest {
    format: String,
    version: u32,
    network_profile: String,
    chain_id: String,
    wallet_anchor_address: String,
    derivation_domain: u32,
    derivation_version: u32,
    account: u32,
    entries: Vec<WalletWatchOnlyEntry>,
    checksum_hex: String,
}

impl WalletWatchOnlyManifest {
    pub fn format(&self) -> &str {
        &self.format
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn network_profile(&self) -> &str {
        &self.network_profile
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    pub fn wallet_anchor_address(&self) -> &str {
        &self.wallet_anchor_address
    }

    pub fn account(&self) -> u32 {
        self.account
    }

    pub fn entries(&self) -> &[WalletWatchOnlyEntry] {
        &self.entries
    }

    pub fn checksum_hex(&self) -> &str {
        &self.checksum_hex
    }

    pub fn validate(&self) -> Result<(), WalletWatchOnlyError> {
        if self.format != WALLET_WATCH_ONLY_FORMAT {
            return Err(WalletWatchOnlyError::UnsupportedFormat);
        }
        if self.version != WALLET_WATCH_ONLY_VERSION {
            return Err(WalletWatchOnlyError::UnsupportedVersion(self.version));
        }
        validate_metadata("network_profile", &self.network_profile)?;
        validate_metadata("chain_id", &self.chain_id)?;
        validate_metadata("wallet_anchor_address", &self.wallet_anchor_address)?;
        if self.derivation_domain != WALLET_DERIVATION_DOMAIN
            || self.derivation_version != WALLET_DERIVATION_VERSION
        {
            return Err(WalletWatchOnlyError::UnsupportedDerivation);
        }
        if self.account > WALLET_DERIVATION_MAX_INDEX {
            return Err(WalletWatchOnlyError::IndexOutOfRange("account"));
        }
        if self.entries.is_empty() {
            return Err(WalletWatchOnlyError::EmptyManifest);
        }
        if self.entries.len() > WALLET_WATCH_ONLY_MAX_ENTRIES {
            return Err(WalletWatchOnlyError::TooManyEntries);
        }

        let network_components = derive_network_components(&self.network_profile, &self.chain_id);
        let mut previous: Option<(WalletWatchOnlyBranch, u32)> = None;
        for entry in &self.entries {
            if entry.index > WALLET_DERIVATION_MAX_INDEX {
                return Err(WalletWatchOnlyError::IndexOutOfRange("index"));
            }
            let current = (entry.branch, entry.index);
            if previous.is_some_and(|previous| current <= previous) {
                return Err(WalletWatchOnlyError::NonCanonicalEntries);
            }
            previous = Some(current);

            let expected_path = derivation_path(
                network_components,
                self.account,
                entry.branch,
                entry.index,
            );
            if entry.derivation_path != expected_path {
                return Err(WalletWatchOnlyError::InvalidDerivationPath);
            }
            let public_key = decode_canonical_hex(&entry.public_key_hex)
                .ok_or(WalletWatchOnlyError::InvalidPublicKey)?;
            if public_key.len() != 32 {
                return Err(WalletWatchOnlyError::InvalidPublicKey);
            }
            validate_metadata("address", &entry.address)?;
            if address_from_public_key(&entry.public_key_hex) != entry.address {
                return Err(WalletWatchOnlyError::AddressMismatch);
            }
        }

        let checksum = decode_canonical_hex(&self.checksum_hex)
            .ok_or(WalletWatchOnlyError::InvalidChecksum)?;
        if checksum.len() != 32 || self.checksum_hex != compute_checksum(self) {
            return Err(WalletWatchOnlyError::InvalidChecksum);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletWatchOnly {
    manifest: WalletWatchOnlyManifest,
}

impl WalletWatchOnly {
    pub fn import(manifest: WalletWatchOnlyManifest) -> Result<Self, WalletWatchOnlyError> {
        manifest.validate()?;
        Ok(Self { manifest })
    }

    pub fn manifest(&self) -> &WalletWatchOnlyManifest {
        &self.manifest
    }

    pub fn entries(&self) -> &[WalletWatchOnlyEntry] {
        self.manifest.entries()
    }

    pub fn network_profile(&self) -> &str {
        self.manifest.network_profile()
    }

    pub fn chain_id(&self) -> &str {
        self.manifest.chain_id()
    }

    pub fn account(&self) -> u32 {
        self.manifest.account()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletWatchOnlyError {
    UnsupportedFormat,
    UnsupportedVersion(u32),
    UnsupportedDerivation,
    InvalidMetadata(&'static str),
    EmptyManifest,
    TooManyEntries,
    IndexOutOfRange(&'static str),
    NonCanonicalEntries,
    InvalidDerivationPath,
    InvalidPublicKey,
    AddressMismatch,
    InvalidChecksum,
    NetworkMismatch,
    AnchorMismatch,
    VerificationMismatch,
}

impl fmt::Display for WalletWatchOnlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("unsupported watch-only manifest format"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported watch-only manifest version: {version}")
            }
            Self::UnsupportedDerivation => {
                f.write_str("unsupported watch-only derivation domain or version")
            }
            Self::InvalidMetadata(field) => write!(f, "invalid watch-only metadata: {field}"),
            Self::EmptyManifest => f.write_str("watch-only manifest contains no public entries"),
            Self::TooManyEntries => f.write_str("watch-only manifest exceeds the bounded entry limit"),
            Self::IndexOutOfRange(field) => {
                write!(f, "watch-only derivation index exceeds the hardened range: {field}")
            }
            Self::NonCanonicalEntries => {
                f.write_str("watch-only entries are duplicated or not in canonical order")
            }
            Self::InvalidDerivationPath => f.write_str("watch-only derivation path is invalid"),
            Self::InvalidPublicKey => f.write_str("watch-only public key is invalid"),
            Self::AddressMismatch => {
                f.write_str("watch-only address does not match its public key")
            }
            Self::InvalidChecksum => f.write_str("watch-only manifest checksum is invalid"),
            Self::NetworkMismatch => {
                f.write_str("watch-only manifest does not match the unlocked wallet network")
            }
            Self::AnchorMismatch => {
                f.write_str("watch-only manifest does not match the unlocked wallet anchor")
            }
            Self::VerificationMismatch => {
                f.write_str("watch-only manifest does not match the unlocked deterministic seed")
            }
        }
    }
}

impl Error for WalletWatchOnlyError {}

#[derive(Debug)]
pub enum WalletWatchOnlyOperationError {
    Manifest(WalletWatchOnlyError),
    Session(WalletSessionError),
}

impl fmt::Display for WalletWatchOnlyOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "watch-only manifest operation failed: {error}"),
            Self::Session(error) => write!(f, "watch-only wallet session operation failed: {error}"),
        }
    }
}

impl Error for WalletWatchOnlyOperationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Manifest(error) => Some(error),
            Self::Session(error) => Some(error),
        }
    }
}

impl From<WalletWatchOnlyError> for WalletWatchOnlyOperationError {
    fn from(value: WalletWatchOnlyError) -> Self {
        Self::Manifest(value)
    }
}

impl From<WalletSessionError> for WalletWatchOnlyOperationError {
    fn from(value: WalletSessionError) -> Self {
        Self::Session(value)
    }
}

pub trait WalletWatchOnlySessionExt {
    fn export_watch_only_manifest(
        &self,
        scope: WalletWatchOnlyScope,
    ) -> Result<WalletWatchOnlyManifest, WalletWatchOnlyOperationError>;

    fn verify_watch_only_manifest(
        &self,
        manifest: &WalletWatchOnlyManifest,
    ) -> Result<(), WalletWatchOnlyOperationError>;
}

impl WalletWatchOnlySessionExt for WalletSession {
    fn export_watch_only_manifest(
        &self,
        scope: WalletWatchOnlyScope,
    ) -> Result<WalletWatchOnlyManifest, WalletWatchOnlyOperationError> {
        export_watch_only_manifest(self, scope)
    }

    fn verify_watch_only_manifest(
        &self,
        manifest: &WalletWatchOnlyManifest,
    ) -> Result<(), WalletWatchOnlyOperationError> {
        verify_watch_only_manifest(self, manifest)
    }
}

pub fn export_watch_only_manifest(
    session: &WalletSession,
    scope: WalletWatchOnlyScope,
) -> Result<WalletWatchOnlyManifest, WalletWatchOnlyOperationError> {
    let identity = unlocked_identity(session)?;
    let mut entries = Vec::with_capacity(
        (scope.receive_count as usize).saturating_add(scope.change_count as usize),
    );
    append_session_entries(
        &mut entries,
        session,
        scope.account,
        WalletDerivationBranch::Receive,
        scope.receive_count,
    )?;
    append_session_entries(
        &mut entries,
        session,
        scope.account,
        WalletDerivationBranch::Change,
        scope.change_count,
    )?;

    let mut manifest = WalletWatchOnlyManifest {
        format: WALLET_WATCH_ONLY_FORMAT.to_string(),
        version: WALLET_WATCH_ONLY_VERSION,
        network_profile: identity.network_profile,
        chain_id: identity.chain_id,
        wallet_anchor_address: identity.address,
        derivation_domain: WALLET_DERIVATION_DOMAIN,
        derivation_version: WALLET_DERIVATION_VERSION,
        account: scope.account,
        entries,
        checksum_hex: String::new(),
    };
    manifest.checksum_hex = compute_checksum(&manifest);
    manifest.validate()?;
    Ok(manifest)
}

pub fn verify_watch_only_manifest(
    session: &WalletSession,
    manifest: &WalletWatchOnlyManifest,
) -> Result<(), WalletWatchOnlyOperationError> {
    manifest.validate()?;
    let identity = unlocked_identity(session)?;
    if manifest.network_profile != identity.network_profile || manifest.chain_id != identity.chain_id {
        return Err(WalletWatchOnlyError::NetworkMismatch.into());
    }
    if manifest.wallet_anchor_address != identity.address {
        return Err(WalletWatchOnlyError::AnchorMismatch.into());
    }

    for entry in &manifest.entries {
        let matches = session.with_derived_key(
            manifest.account,
            entry.branch.derivation_branch(),
            entry.index,
            |derived| entry_matches_derived(entry, derived),
        )?;
        if !matches {
            return Err(WalletWatchOnlyError::VerificationMismatch.into());
        }
    }
    Ok(())
}

fn unlocked_identity(
    session: &WalletSession,
) -> Result<WalletSessionIdentity, WalletWatchOnlyOperationError> {
    session.status().identity.ok_or(WalletSessionError::Locked.into())
}

fn append_session_entries(
    entries: &mut Vec<WalletWatchOnlyEntry>,
    session: &WalletSession,
    account: u32,
    branch: WalletDerivationBranch,
    count: u32,
) -> Result<(), WalletWatchOnlyOperationError> {
    for index in 0..count {
        let entry = session.with_derived_key(account, branch, index, |derived| {
            WalletWatchOnlyEntry {
                branch: branch.into(),
                index,
                derivation_path: derived.derivation_path().to_string(),
                public_key_hex: derived.public_key_hex().to_string(),
                address: derived.address().to_string(),
            }
        })?;
        entries.push(entry);
    }
    Ok(())
}

fn entry_matches_derived(entry: &WalletWatchOnlyEntry, derived: &WalletDerivedKey) -> bool {
    entry.derivation_path == derived.derivation_path()
        && entry.public_key_hex == derived.public_key_hex()
        && entry.address == derived.address()
}

fn derivation_path(
    network_components: [u32; 4],
    account: u32,
    branch: WalletWatchOnlyBranch,
    index: u32,
) -> String {
    format!(
        "m/{}'/{}'/{}'/{}'/{}'/{}'/{}'/{}'/{}'",
        WALLET_DERIVATION_DOMAIN,
        WALLET_DERIVATION_VERSION,
        network_components[0],
        network_components[1],
        network_components[2],
        network_components[3],
        account,
        branch.component(),
        index
    )
}

fn compute_checksum(manifest: &WalletWatchOnlyManifest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(WATCH_ONLY_CHECKSUM_DOMAIN);
    update_len_prefixed(&mut hasher, manifest.format.as_bytes());
    hasher.update(manifest.version.to_be_bytes());
    update_len_prefixed(&mut hasher, manifest.network_profile.as_bytes());
    update_len_prefixed(&mut hasher, manifest.chain_id.as_bytes());
    update_len_prefixed(&mut hasher, manifest.wallet_anchor_address.as_bytes());
    hasher.update(manifest.derivation_domain.to_be_bytes());
    hasher.update(manifest.derivation_version.to_be_bytes());
    hasher.update(manifest.account.to_be_bytes());
    hasher.update((manifest.entries.len() as u64).to_be_bytes());
    for entry in &manifest.entries {
        hasher.update([entry.branch.component() as u8]);
        hasher.update(entry.index.to_be_bytes());
        update_len_prefixed(&mut hasher, entry.derivation_path.as_bytes());
        update_len_prefixed(&mut hasher, entry.public_key_hex.as_bytes());
        update_len_prefixed(&mut hasher, entry.address.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn update_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn validate_metadata(field: &'static str, value: &str) -> Result<(), WalletWatchOnlyError> {
    if value.is_empty() || value.trim() != value {
        return Err(WalletWatchOnlyError::InvalidMetadata(field));
    }
    Ok(())
}

fn decode_canonical_hex(value: &str) -> Option<Vec<u8>> {
    let decoded = hex::decode(value).ok()?;
    if hex::encode(&decoded) != value {
        return None;
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::Duration};

    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};

    use super::*;
    use crate::{
        deterministic::{derive_wallet_key_from_seed, WalletNetworkContext},
        keystore_crypto::{encrypt_private_key_with_kdf_costs, KeystoreKdfCosts},
        keystore_seed::{encrypt_wallet_seed_with_kdf_costs, SeedKeystoreKdfCosts},
        wallet_seed_from_mnemonic, SecretString, WalletKeystoreFile, WalletSecretKey,
        WalletSessionLockState, WalletUnlockPolicy, ED25519_SECRET_KEY_BYTES,
        KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_LANES, KEYSTORE_KDF_MIN_MEMORY_KIB,
    };

    const MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const PASSWORD: &str = "watch-only-test-password";
    const NETWORK_PROFILE: &str = "public-testnet-v2.4.0-candidate";
    const CHAIN_ID: &str = "pulsedag-public-testnet-v2.4.0-candidate";

    fn test_dir(label: &str) -> PathBuf {
        let mut random = [0_u8; 8];
        OsRng.fill_bytes(&mut random);
        let dir = std::env::temp_dir().join(format!(
            "pulsedag-watch-only-{label}-{}-{}",
            std::process::id(),
            hex::encode(random)
        ));
        fs::create_dir(&dir).expect("create watch-only test directory");
        dir
    }

    fn seed_fixture(label: &str) -> (PathBuf, WalletKeystoreFile) {
        let dir = test_dir(label);
        let path = dir.join("wallet.json");
        let seed = wallet_seed_from_mnemonic(&SecretString::new(MNEMONIC), None).expect("seed");
        let network = WalletNetworkContext::new(NETWORK_PROFILE, CHAIN_ID).expect("network");
        let anchor = derive_wallet_key_from_seed(
            &seed,
            &network,
            0,
            WalletDerivationBranch::Receive,
            0,
        )
        .expect("anchor")
        .address()
        .to_string();
        let envelope = encrypt_wallet_seed_with_kdf_costs(
            NETWORK_PROFILE,
            CHAIN_ID,
            &anchor,
            &seed,
            &SecretString::new(PASSWORD),
            SeedKeystoreKdfCosts::new(
                KEYSTORE_KDF_MIN_MEMORY_KIB,
                KEYSTORE_KDF_MIN_ITERATIONS,
                KEYSTORE_KDF_MIN_LANES,
            ),
        )
        .expect("encrypt seed fixture");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire seed fixture");
        file.create_new(&envelope).expect("persist seed fixture");
        (dir, file)
    }

    fn v1_fixture(label: &str) -> (PathBuf, WalletKeystoreFile) {
        let dir = test_dir(label);
        let path = dir.join("wallet.json");
        let bytes = [0x55; ED25519_SECRET_KEY_BYTES];
        let secret = WalletSecretKey::from_bytes(bytes);
        let address = address_from_public_key(&hex::encode(
            SigningKey::from_bytes(&bytes).verifying_key().to_bytes(),
        ));
        let envelope = encrypt_private_key_with_kdf_costs(
            NETWORK_PROFILE,
            CHAIN_ID,
            &address,
            &secret,
            &SecretString::new(PASSWORD),
            KeystoreKdfCosts::new(
                KEYSTORE_KDF_MIN_MEMORY_KIB,
                KEYSTORE_KDF_MIN_ITERATIONS,
                KEYSTORE_KDF_MIN_LANES,
            ),
        )
        .expect("encrypt v1 fixture");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire v1 fixture");
        file.create_new(&envelope).expect("persist v1 fixture");
        (dir, file)
    }

    #[test]
    fn seed_session_exports_public_manifest_and_verifies_it() {
        let (dir, file) = seed_fixture("export-verify");
        let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
            .expect("policy");
        let mut session = WalletSession::new(policy).expect("session");
        let status = session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock");
        assert_eq!(status.lock_state, WalletSessionLockState::Unlocked);

        let scope = WalletWatchOnlyScope::new(0, 3, 2).expect("scope");
        let manifest = session.export_watch_only_manifest(scope).expect("export");
        assert_eq!(manifest.entries().len(), 5);
        assert_eq!(manifest.entries()[0].branch(), WalletWatchOnlyBranch::Receive);
        assert_eq!(manifest.entries()[3].branch(), WalletWatchOnlyBranch::Change);
        session
            .verify_watch_only_manifest(&manifest)
            .expect("verify matching backup");

        let encoded = serde_json::to_string(&manifest).expect("serialize");
        assert!(!encoded.contains(MNEMONIC));
        assert!(!encoded.contains(PASSWORD));
        let decoded: WalletWatchOnlyManifest = serde_json::from_str(&encoded).expect("deserialize");
        let imported = WalletWatchOnly::import(decoded).expect("watch-only import");
        assert_eq!(imported.entries().len(), 5);
        assert_eq!(imported.network_profile(), NETWORK_PROFILE);
        assert_eq!(imported.chain_id(), CHAIN_ID);

        assert!(session.lock());
        assert!(matches!(
            session.verify_watch_only_manifest(&manifest),
            Err(WalletWatchOnlyOperationError::Session(WalletSessionError::Locked))
        ));
        drop(session);
        drop(file);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn v1_session_rejects_watch_only_export_distinctly() {
        let (dir, file) = v1_fixture("wrong-kind");
        let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
            .expect("policy");
        let mut session = WalletSession::new(policy).expect("session");
        session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock v1");
        let scope = WalletWatchOnlyScope::new(0, 1, 0).expect("scope");
        assert!(matches!(
            session.export_watch_only_manifest(scope),
            Err(WalletWatchOnlyOperationError::Session(
                WalletSessionError::WrongSecretKind
            ))
        ));
        drop(session);
        drop(file);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn public_tampering_and_noncanonical_entries_fail_closed() {
        let (dir, file) = seed_fixture("tamper");
        let policy = WalletUnlockPolicy::new(Duration::from_secs(5), 3, Duration::from_secs(1))
            .expect("policy");
        let mut session = WalletSession::new(policy).expect("session");
        session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock");
        let scope = WalletWatchOnlyScope::new(0, 2, 1).expect("scope");
        let manifest = session.export_watch_only_manifest(scope).expect("export");

        let mut bad_checksum = manifest.clone();
        bad_checksum.checksum_hex = "00".repeat(32);
        assert_eq!(bad_checksum.validate(), Err(WalletWatchOnlyError::InvalidChecksum));

        let mut bad_public_key = manifest.clone();
        bad_public_key.entries[0].public_key_hex = "00".repeat(32);
        assert_eq!(bad_public_key.validate(), Err(WalletWatchOnlyError::AddressMismatch));

        let mut reordered = manifest;
        reordered.entries.swap(0, 1);
        assert_eq!(
            reordered.validate(),
            Err(WalletWatchOnlyError::NonCanonicalEntries)
        );

        drop(session);
        drop(file);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn scope_is_bounded_and_nonempty() {
        assert_eq!(
            WalletWatchOnlyScope::new(0, 0, 0),
            Err(WalletWatchOnlyError::EmptyManifest)
        );
        assert_eq!(
            WalletWatchOnlyScope::new(0, WALLET_WATCH_ONLY_MAX_ENTRIES as u32, 1),
            Err(WalletWatchOnlyError::TooManyEntries)
        );
    }
}