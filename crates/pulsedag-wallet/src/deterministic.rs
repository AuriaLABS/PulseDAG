use std::{error::Error, fmt};

use bip39::{Language, Mnemonic};
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use pulsedag_core::address_from_public_key;
use sha2::{Digest, Sha256, Sha512};
use zeroize::Zeroizing;

use crate::{SecretString, WalletSecretKey};

pub const WALLET_MNEMONIC_WORDS: usize = 24;
pub const WALLET_DERIVATION_DOMAIN: u32 = 0x5055_4c53; // ASCII "PULS"
pub const WALLET_DERIVATION_VERSION: u32 = 1;
pub const WALLET_DERIVATION_MAX_INDEX: u32 = 0x7fff_ffff;
const HARDENED_BIT: u32 = 0x8000_0000;
const NETWORK_DOMAIN: &[u8] = b"PulseDAG:wallet-network:v1";
const ED25519_SLIP10_SEED: &[u8] = b"ed25519 seed";

type HmacSha512 = Hmac<Sha512>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletDerivationBranch {
    Receive,
    Change,
}

impl WalletDerivationBranch {
    fn component(self) -> u32 {
        match self {
            Self::Receive => 0,
            Self::Change => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletNetworkContext {
    network_profile: String,
    chain_id: String,
    network_component: u32,
}

impl WalletNetworkContext {
    pub fn new(
        network_profile: impl Into<String>,
        chain_id: impl Into<String>,
    ) -> Result<Self, WalletDeterministicError> {
        let network_profile = network_profile.into();
        let chain_id = chain_id.into();
        validate_metadata("network_profile", &network_profile)?;
        validate_metadata("chain_id", &chain_id)?;
        let network_component = derive_network_component(&network_profile, &chain_id);
        Ok(Self {
            network_profile,
            chain_id,
            network_component,
        })
    }

    pub fn network_profile(&self) -> &str {
        &self.network_profile
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    pub fn network_component(&self) -> u32 {
        self.network_component
    }
}

pub struct WalletDerivedKey {
    secret_key: WalletSecretKey,
    public_key_hex: String,
    address: String,
    derivation_path: String,
}

impl WalletDerivedKey {
    pub fn secret_key(&self) -> &WalletSecretKey {
        &self.secret_key
    }

    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn derivation_path(&self) -> &str {
        &self.derivation_path
    }
}

impl fmt::Debug for WalletDerivedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletDerivedKey")
            .field("secret_key", &self.secret_key)
            .field("public_key_hex", &self.public_key_hex)
            .field("address", &self.address)
            .field("derivation_path", &self.derivation_path)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletDeterministicError {
    InvalidMnemonic,
    InvalidMetadata(&'static str),
    IndexOutOfRange(&'static str),
}

impl fmt::Display for WalletDeterministicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMnemonic => f.write_str("invalid BIP-39 mnemonic"),
            Self::InvalidMetadata(field) => write!(f, "invalid wallet network metadata: {field}"),
            Self::IndexOutOfRange(field) => {
                write!(f, "wallet derivation index exceeds hardened range: {field}")
            }
        }
    }
}

impl Error for WalletDeterministicError {}

/// Generate a 24-word English BIP-39 recovery phrase.
///
/// The returned value is an explicit zeroizing secret boundary. Callers should
/// display/verify it only during wallet initialization and must not persist it
/// as plaintext.
pub fn generate_wallet_mnemonic() -> Result<SecretString, WalletDeterministicError> {
    let mnemonic = Mnemonic::generate_in(Language::English, WALLET_MNEMONIC_WORDS)
        .map_err(|_| WalletDeterministicError::InvalidMnemonic)?;
    Ok(SecretString::new(mnemonic.to_string()))
}

/// Deterministically restore one PulseDAG Ed25519 child key from BIP-39 backup material.
///
/// PulseDAG v1 path:
/// `m / PULS' / 1' / network' / account' / branch' / index'`.
/// Every child is hardened, as required by SLIP-0010 for Ed25519.
pub fn derive_wallet_key(
    mnemonic_secret: &SecretString,
    bip39_passphrase: Option<&SecretString>,
    network: &WalletNetworkContext,
    account: u32,
    branch: WalletDerivationBranch,
    index: u32,
) -> Result<WalletDerivedKey, WalletDeterministicError> {
    validate_index("account", account)?;
    validate_index("index", index)?;

    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_secret.expose_secret())
        .map_err(|_| WalletDeterministicError::InvalidMnemonic)?;
    let passphrase = bip39_passphrase
        .map(SecretString::expose_secret)
        .unwrap_or("");
    let seed = Zeroizing::new(mnemonic.to_seed(passphrase));

    let components = [
        WALLET_DERIVATION_DOMAIN,
        WALLET_DERIVATION_VERSION,
        network.network_component,
        account,
        branch.component(),
        index,
    ];
    let secret_bytes = derive_slip10_ed25519(&seed, &components);
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let address = address_from_public_key(&public_key_hex);
    let derivation_path = format!(
        "m/{}'/{}'/{}'/{}'/{}'/{}'",
        WALLET_DERIVATION_DOMAIN,
        WALLET_DERIVATION_VERSION,
        network.network_component,
        account,
        branch.component(),
        index
    );

    Ok(WalletDerivedKey {
        secret_key: WalletSecretKey::from_bytes(*secret_bytes),
        public_key_hex,
        address,
        derivation_path,
    })
}

pub fn derive_network_component(network_profile: &str, chain_id: &str) -> u32 {
    let mut hasher = Sha256::new();
    hasher.update(NETWORK_DOMAIN);
    update_len_prefixed(&mut hasher, network_profile.as_bytes());
    update_len_prefixed(&mut hasher, chain_id.as_bytes());
    let digest = hasher.finalize();
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) & WALLET_DERIVATION_MAX_INDEX
}

fn validate_metadata(
    field: &'static str,
    value: &str,
) -> Result<(), WalletDeterministicError> {
    if value.is_empty() || value.trim() != value {
        return Err(WalletDeterministicError::InvalidMetadata(field));
    }
    Ok(())
}

fn validate_index(field: &'static str, value: u32) -> Result<(), WalletDeterministicError> {
    if value > WALLET_DERIVATION_MAX_INDEX {
        return Err(WalletDeterministicError::IndexOutOfRange(field));
    }
    Ok(())
}

fn update_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("wallet metadata length exceeds u32::MAX");
    hasher.update(len.to_be_bytes());
    hasher.update(bytes);
}

fn derive_slip10_ed25519(seed: &[u8; 64], components: &[u32]) -> Zeroizing<[u8; 32]> {
    let master = hmac_sha512(ED25519_SLIP10_SEED, seed);
    let mut key = Zeroizing::new([0u8; 32]);
    let mut chain_code = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&master[..32]);
    chain_code.copy_from_slice(&master[32..]);

    for &component in components {
        let hardened = component | HARDENED_BIT;
        let mut data = Zeroizing::new([0u8; 37]);
        data[0] = 0;
        data[1..33].copy_from_slice(&key[..]);
        data[33..].copy_from_slice(&hardened.to_be_bytes());
        let child = hmac_sha512(&chain_code[..], &data[..]);
        key.copy_from_slice(&child[..32]);
        chain_code.copy_from_slice(&child[32..]);
    }
    key
}

fn hmac_sha512(key: &[u8], data: &[u8]) -> Zeroizing<[u8; 64]> {
    let mut mac = HmacSha512::new_from_slice(key).expect("HMAC-SHA512 accepts arbitrary key length");
    mac.update(data);
    let bytes = mac.finalize().into_bytes();
    let mut out = Zeroizing::new([0u8; 64]);
    out.copy_from_slice(&bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn vector_network() -> WalletNetworkContext {
        WalletNetworkContext::new(
            "public-testnet-v2.4.0-candidate",
            "pulsedag-public-testnet-v2.4.0-candidate",
        )
        .expect("network context")
    }

    #[test]
    fn golden_restore_vector_is_stable() {
        let mnemonic = SecretString::new(VECTOR_MNEMONIC);
        let passphrase = SecretString::new("TREZOR");
        let network = vector_network();
        assert_eq!(network.network_component(), 56_585_194);

        let derived = derive_wallet_key(
            &mnemonic,
            Some(&passphrase),
            &network,
            0,
            WalletDerivationBranch::Receive,
            0,
        )
        .expect("derive vector");

        assert_eq!(
            hex::encode(derived.secret_key().expose_secret()),
            "2c24f6320d0a44df743c2976ca77a8d16f51ef4f42604e3c04245760076b938a"
        );
        assert_eq!(
            derived.public_key_hex(),
            "b858b04e1cc0d49a27f6c8dd6caff3a57afbae0604df341a84ed48b932d76414"
        );
        assert_eq!(
            derived.address(),
            "pulse1fffffb1f953a1022bb2e8e12b9bf454e2bc379b5"
        );
        assert_eq!(
            derived.derivation_path(),
            "m/1347767379'/1'/56585194'/0'/0'/0'"
        );
    }

    #[test]
    fn restore_is_deterministic_and_separated_by_network_branch_and_index() {
        let mnemonic = SecretString::new(VECTOR_MNEMONIC);
        let network = vector_network();
        let same_a = derive_wallet_key(
            &mnemonic,
            None,
            &network,
            3,
            WalletDerivationBranch::Receive,
            7,
        )
        .expect("derive");
        let same_b = derive_wallet_key(
            &mnemonic,
            None,
            &network,
            3,
            WalletDerivationBranch::Receive,
            7,
        )
        .expect("derive");
        assert_eq!(same_a.address(), same_b.address());
        assert_eq!(same_a.public_key_hex(), same_b.public_key_hex());

        let change = derive_wallet_key(
            &mnemonic,
            None,
            &network,
            3,
            WalletDerivationBranch::Change,
            7,
        )
        .expect("derive change");
        let next = derive_wallet_key(
            &mnemonic,
            None,
            &network,
            3,
            WalletDerivationBranch::Receive,
            8,
        )
        .expect("derive next");
        let other_network = WalletNetworkContext::new("private-operator", "pulsedag-private")
            .expect("other network");
        let separated = derive_wallet_key(
            &mnemonic,
            None,
            &other_network,
            3,
            WalletDerivationBranch::Receive,
            7,
        )
        .expect("derive other network");

        assert_ne!(same_a.address(), change.address());
        assert_ne!(same_a.address(), next.address());
        assert_ne!(same_a.address(), separated.address());
    }

    #[test]
    fn generation_produces_valid_24_word_secret_and_invalid_input_fails_closed() {
        let generated = generate_wallet_mnemonic().expect("generate mnemonic");
        let parsed = Mnemonic::parse_in(Language::English, generated.expose_secret())
            .expect("generated mnemonic parses");
        assert_eq!(parsed.word_count(), WALLET_MNEMONIC_WORDS);

        let invalid = SecretString::new("not a valid wallet recovery phrase");
        assert!(matches!(
            derive_wallet_key(
                &invalid,
                None,
                &vector_network(),
                0,
                WalletDerivationBranch::Receive,
                0,
            ),
            Err(WalletDeterministicError::InvalidMnemonic)
        ));
    }

    #[test]
    fn metadata_and_indices_are_bounded_before_derivation() {
        assert!(WalletNetworkContext::new(" testnet", "chain").is_err());
        let mnemonic = SecretString::new(VECTOR_MNEMONIC);
        assert!(matches!(
            derive_wallet_key(
                &mnemonic,
                None,
                &vector_network(),
                WALLET_DERIVATION_MAX_INDEX + 1,
                WalletDerivationBranch::Receive,
                0,
            ),
            Err(WalletDeterministicError::IndexOutOfRange("account"))
        ));
    }
}
