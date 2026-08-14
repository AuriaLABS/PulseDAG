use std::{error::Error, fmt};

use pulsedag_core::address_from_public_key;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    deterministic::{
        derive_network_components, derive_wallet_key_from_seed, WalletDerivationBranch,
        WalletDeterministicError, WalletNetworkContext, WALLET_DERIVATION_DOMAIN,
        WALLET_DERIVATION_MAX_INDEX, WALLET_DERIVATION_VERSION,
    },
    secrets::WalletSeed,
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
        if self.derivation_domain != WALLET_DERIVATION_DOMAIN {
            return Err(WalletWatchOnlyError::UnsupportedDerivation);
        }
        if self.derivation_version != WALLET_DERIVATION_VERSION {
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
    Deterministic(WalletDeterministicError),
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
            Self::Deterministic(error) => write!(f, "watch-only derivation failed: {error}"),
        }
    }
}

impl Error for WalletWatchOnlyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Deterministic(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WalletDeterministicError> for WalletWatchOnlyError {
    fn from(value: WalletDeterministicError) -> Self {
        Self::Deterministic(value)
    }
}

pub(crate) fn build_watch_only_manifest(
    seed: &WalletSeed,
    network: &WalletNetworkContext,
    wallet_anchor_address: &str,
    scope: WalletWatchOnlyScope,
) -> Result<WalletWatchOnlyManifest, WalletWatchOnlyError> {
    let anchor = derive_wallet_key_from_seed(seed, network, 0, WalletDerivationBranch::Receive, 0)?;
    if anchor.address() != wallet_anchor_address {
        return Err(WalletWatchOnlyError::AnchorMismatch);
    }

    let mut entries = Vec::with_capacity(
        (scope.receive_count as usize).saturating_add(scope.change_count as usize),
    );
    append_entries(
        &mut entries,
        seed,
        network,
        scope.account,
        WalletDerivationBranch::Receive,
        scope.receive_count,
    )?;
    append_entries(
        &mut entries,
        seed,
        network,
        scope.account,
        WalletDerivationBranch::Change,
        scope.change_count,
    )?;

    let mut manifest = WalletWatchOnlyManifest {
        format: WALLET_WATCH_ONLY_FORMAT.to_string(),
        version: WALLET_WATCH_ONLY_VERSION,
        network_profile: network.network_profile().to_string(),
        chain_id: network.chain_id().to_string(),
        wallet_anchor_address: wallet_anchor_address.to_string(),
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

pub(crate) fn verify_watch_only_manifest_with_seed(
    seed: &WalletSeed,
    network: &WalletNetworkContext,
    wallet_anchor_address: &str,
    manifest: &WalletWatchOnlyManifest,
) -> Result<(), WalletWatchOnlyError> {
    manifest.validate()?;
    if manifest.network_profile != network.network_profile() || manifest.chain_id != network.chain_id() {
        return Err(WalletWatchOnlyError::NetworkMismatch);
    }
    if manifest.wallet_anchor_address != wallet_anchor_address {
        return Err(WalletWatchOnlyError::AnchorMismatch);
    }
    let anchor = derive_wallet_key_from_seed(seed, network, 0, WalletDerivationBranch::Receive, 0)?;
    if anchor.address() != wallet_anchor_address {
        return Err(WalletWatchOnlyError::AnchorMismatch);
    }

    for entry in &manifest.entries {
        let derived = derive_wallet_key_from_seed(
            seed,
            network,
            manifest.account,
            entry.branch.derivation_branch(),
            entry.index,
        )?;
        if derived.derivation_path() != entry.derivation_path
            || derived.public_key_hex() != entry.public_key_hex
            || derived.address() != entry.address
        {
            return Err(WalletWatchOnlyError::VerificationMismatch);
        }
    }
    Ok(())
}

fn append_entries(
    entries: &mut Vec<WalletWatchOnlyEntry>,
    seed: &WalletSeed,
    network: &WalletNetworkContext,
    account: u32,
    branch: WalletDerivationBranch,
    count: u32,
) -> Result<(), WalletWatchOnlyError> {
    for index in 0..count {
        let derived = derive_wallet_key_from_seed(seed, network, account, branch, index)?;
        entries.push(WalletWatchOnlyEntry {
            branch: branch.into(),
            index,
            derivation_path: derived.derivation_path().to_string(),
            public_key_hex: derived.public_key_hex().to_string(),
            address: derived.address().to_string(),
        });
    }
    Ok(())
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
    use super::*;
    use crate::{wallet_seed_from_mnemonic, SecretString};

    const MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const NETWORK_PROFILE: &str = "public-testnet-v2.4.0-candidate";
    const CHAIN_ID: &str = "pulsedag-public-testnet-v2.4.0-candidate";

    fn fixture() -> (WalletSeed, WalletNetworkContext, String) {
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
        (seed, network, anchor)
    }

    #[test]
    fn manifest_round_trip_is_public_canonical_and_watch_only() {
        let (seed, network, anchor) = fixture();
        let scope = WalletWatchOnlyScope::new(0, 3, 2).expect("scope");
        let manifest = build_watch_only_manifest(&seed, &network, &anchor, scope).expect("manifest");
        assert_eq!(manifest.entries().len(), 5);
        assert_eq!(manifest.entries()[0].branch(), WalletWatchOnlyBranch::Receive);
        assert_eq!(manifest.entries()[3].branch(), WalletWatchOnlyBranch::Change);
        assert_eq!(manifest.checksum_hex().len(), 64);

        let encoded = serde_json::to_string(&manifest).expect("serialize");
        assert!(!encoded.contains(MNEMONIC));
        assert!(!encoded.contains("seed-session-password"));
        let decoded: WalletWatchOnlyManifest = serde_json::from_str(&encoded).expect("deserialize");
        let watch_only = WalletWatchOnly::import(decoded).expect("import");
        assert_eq!(watch_only.entries().len(), 5);
        assert_eq!(watch_only.network_profile(), NETWORK_PROFILE);
        assert_eq!(watch_only.chain_id(), CHAIN_ID);
    }

    #[test]
    fn checksum_and_public_material_tampering_fail_closed() {
        let (seed, network, anchor) = fixture();
        let scope = WalletWatchOnlyScope::new(0, 1, 1).expect("scope");
        let manifest = build_watch_only_manifest(&seed, &network, &anchor, scope).expect("manifest");

        let mut bad_checksum = manifest.clone();
        bad_checksum.checksum_hex = "00".repeat(32);
        assert_eq!(bad_checksum.validate(), Err(WalletWatchOnlyError::InvalidChecksum));

        let mut bad_public_key = manifest;
        bad_public_key.entries[0].public_key_hex = "00".repeat(32);
        assert_eq!(bad_public_key.validate(), Err(WalletWatchOnlyError::AddressMismatch));
    }

    #[test]
    fn verification_rejects_wrong_seed_and_network() {
        let (seed, network, anchor) = fixture();
        let scope = WalletWatchOnlyScope::new(0, 2, 1).expect("scope");
        let manifest = build_watch_only_manifest(&seed, &network, &anchor, scope).expect("manifest");

        let wrong_seed = wallet_seed_from_mnemonic(
            &SecretString::new(
                "legal winner thank year wave sausage worth useful legal winner thank yellow",
            ),
            None,
        )
        .expect("wrong seed");
        assert_eq!(
            verify_watch_only_manifest_with_seed(&wrong_seed, &network, &anchor, &manifest),
            Err(WalletWatchOnlyError::AnchorMismatch)
        );

        let other_network = WalletNetworkContext::new("other-testnet", CHAIN_ID).expect("network");
        assert_eq!(
            verify_watch_only_manifest_with_seed(&seed, &other_network, &anchor, &manifest),
            Err(WalletWatchOnlyError::NetworkMismatch)
        );
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