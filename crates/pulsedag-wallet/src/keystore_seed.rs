use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    derive_wallet_key_from_seed,
    keystore::{
        invalid, require_hex_len, require_nonempty, validate_kdf_metadata, WalletCipherMetadata,
        WalletKdfMetadata, WalletKeystoreEnvelope, KEYSTORE_CIPHER_XCHACHA20_POLY1305,
        KEYSTORE_DERIVED_KEY_BYTES, KEYSTORE_FORMAT, KEYSTORE_KDF_ARGON2ID,
        KEYSTORE_KDF_DEFAULT_ITERATIONS, KEYSTORE_KDF_DEFAULT_LANES,
        KEYSTORE_KDF_DEFAULT_MEMORY_KIB, KEYSTORE_NONCE_BYTES, KEYSTORE_SALT_BYTES,
        KEYSTORE_SEED_VERSION, KEYSTORE_V2_CIPHERTEXT_BYTES, KEYSTORE_V2_PLAINTEXT_BYTES,
    },
    SecretString, WalletDerivationBranch, WalletKeystoreCryptoError, WalletNetworkContext,
    WalletSeed, WALLET_SEED_BYTES,
};

const KEYSTORE_SEED_AAD_DOMAIN: &str = "PulseDAG:keystore-aad:v2";
const KEYSTORE_SEED_SECRET_KIND: &str = "bip39-seed";
const KEYSTORE_V2_SECRET_MAGIC: &[u8; 16] = b"PULSEDAG-SEED-V2";

#[derive(Clone, Copy)]
pub(crate) struct SeedKeystoreKdfCosts {
    pub(crate) memory_kib: u32,
    pub(crate) iterations: u32,
    pub(crate) lanes: u32,
}

impl SeedKeystoreKdfCosts {
    pub(crate) const fn new(memory_kib: u32, iterations: u32, lanes: u32) -> Self {
        Self {
            memory_kib,
            iterations,
            lanes,
        }
    }

    const fn defaults() -> Self {
        Self::new(
            KEYSTORE_KDF_DEFAULT_MEMORY_KIB,
            KEYSTORE_KDF_DEFAULT_ITERATIONS,
            KEYSTORE_KDF_DEFAULT_LANES,
        )
    }
}

#[derive(Clone, Copy)]
struct SeedKdfMaterial {
    costs: SeedKeystoreKdfCosts,
    salt: [u8; KEYSTORE_SALT_BYTES],
}

/// Encrypt the already-normalized 64-byte BIP-39 seed. Mnemonic words are not
/// accepted or serialized by this layer.
pub fn encrypt_wallet_seed(
    network_profile: &str,
    chain_id: &str,
    address: &str,
    seed: &WalletSeed,
    password: &SecretString,
) -> Result<WalletKeystoreEnvelope, WalletKeystoreCryptoError> {
    encrypt_wallet_seed_with_kdf_costs(
        network_profile,
        chain_id,
        address,
        seed,
        password,
        SeedKeystoreKdfCosts::defaults(),
    )
}

pub(crate) fn encrypt_wallet_seed_with_kdf_costs(
    network_profile: &str,
    chain_id: &str,
    address: &str,
    seed: &WalletSeed,
    password: &SecretString,
    costs: SeedKeystoreKdfCosts,
) -> Result<WalletKeystoreEnvelope, WalletKeystoreCryptoError> {
    if password.is_empty() {
        return Err(WalletKeystoreCryptoError::EmptyPassword);
    }
    ensure_seed_matches_address(seed, network_profile, chain_id, address)?;

    let mut salt = [0_u8; KEYSTORE_SALT_BYTES];
    let mut nonce = [0_u8; KEYSTORE_NONCE_BYTES];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut salt)
        .map_err(|_| WalletKeystoreCryptoError::RandomnessUnavailable)?;
    rng.try_fill_bytes(&mut nonce)
        .map_err(|_| WalletKeystoreCryptoError::RandomnessUnavailable)?;

    encrypt_wallet_seed_with_material(
        network_profile,
        chain_id,
        address,
        seed,
        password,
        SeedKdfMaterial { costs, salt },
        nonce,
    )
}

/// Authenticate and decrypt a v2 deterministic-seed keystore. Version 1
/// single-key envelopes are intentionally rejected by this API.
pub fn decrypt_wallet_seed(
    envelope: &WalletKeystoreEnvelope,
    password: &SecretString,
) -> Result<WalletSeed, WalletKeystoreCryptoError> {
    envelope.validate_structure()?;
    if envelope.version != KEYSTORE_SEED_VERSION {
        return Err(WalletKeystoreCryptoError::InvalidSecretPayload);
    }
    if password.is_empty() {
        return Err(WalletKeystoreCryptoError::EmptyPassword);
    }

    let derived_key = derive_key(password, &envelope.kdf)?;
    let nonce =
        decode_hex_array::<KEYSTORE_NONCE_BYTES>("cipher.nonce_hex", &envelope.cipher.nonce_hex)?;
    let ciphertext = hex::decode(&envelope.ciphertext_hex).map_err(|_| {
        WalletKeystoreCryptoError::Format(invalid(
            "ciphertext_hex",
            "must contain canonical lowercase hexadecimal data",
        ))
    })?;
    let aad = authenticated_seed_metadata(envelope)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&derived_key[..])
        .map_err(|_| WalletKeystoreCryptoError::CipherInitializationFailed)?;
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| WalletKeystoreCryptoError::AuthenticationFailed)?;
    let plaintext = Zeroizing::new(plaintext);
    let seed = decode_seed_payload(&plaintext)?;
    ensure_seed_matches_address(
        &seed,
        &envelope.network_profile,
        &envelope.chain_id,
        &envelope.address,
    )?;
    Ok(seed)
}

fn encrypt_wallet_seed_with_material(
    network_profile: &str,
    chain_id: &str,
    address: &str,
    seed: &WalletSeed,
    password: &SecretString,
    material: SeedKdfMaterial,
    nonce: [u8; KEYSTORE_NONCE_BYTES],
) -> Result<WalletKeystoreEnvelope, WalletKeystoreCryptoError> {
    require_nonempty("network_profile", network_profile)?;
    require_nonempty("chain_id", chain_id)?;
    require_nonempty("address", address)?;
    if password.is_empty() {
        return Err(WalletKeystoreCryptoError::EmptyPassword);
    }
    ensure_seed_matches_address(seed, network_profile, chain_id, address)?;

    let kdf = WalletKdfMetadata {
        algorithm: KEYSTORE_KDF_ARGON2ID.to_string(),
        memory_kib: material.costs.memory_kib,
        iterations: material.costs.iterations,
        lanes: material.costs.lanes,
        salt_hex: hex::encode(material.salt),
    };
    validate_kdf_metadata(&kdf)?;
    let cipher_metadata = WalletCipherMetadata {
        algorithm: KEYSTORE_CIPHER_XCHACHA20_POLY1305.to_string(),
        nonce_hex: hex::encode(nonce),
    };
    let mut envelope = WalletKeystoreEnvelope {
        format: KEYSTORE_FORMAT.to_string(),
        version: KEYSTORE_SEED_VERSION,
        network_profile: network_profile.to_string(),
        chain_id: chain_id.to_string(),
        address: address.to_string(),
        kdf,
        cipher: cipher_metadata,
        ciphertext_hex: String::new(),
    };

    let aad = authenticated_seed_metadata(&envelope)?;
    let derived_key = derive_key(password, &envelope.kdf)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&derived_key[..])
        .map_err(|_| WalletKeystoreCryptoError::CipherInitializationFailed)?;
    let mut secret_payload = Zeroizing::new([0_u8; KEYSTORE_V2_PLAINTEXT_BYTES]);
    secret_payload[..KEYSTORE_V2_SECRET_MAGIC.len()].copy_from_slice(KEYSTORE_V2_SECRET_MAGIC);
    secret_payload[KEYSTORE_V2_SECRET_MAGIC.len()..].copy_from_slice(seed.expose_secret());

    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &secret_payload[..],
                aad: &aad,
            },
        )
        .map_err(|_| WalletKeystoreCryptoError::EncryptionFailed)?;
    if ciphertext.len() != KEYSTORE_V2_CIPHERTEXT_BYTES {
        return Err(WalletKeystoreCryptoError::EncryptionFailed);
    }
    envelope.ciphertext_hex = hex::encode(ciphertext);
    envelope.validate_structure()?;
    Ok(envelope)
}

fn derive_key(
    password: &SecretString,
    kdf: &WalletKdfMetadata,
) -> Result<Zeroizing<[u8; KEYSTORE_DERIVED_KEY_BYTES]>, WalletKeystoreCryptoError> {
    validate_kdf_metadata(kdf)?;
    let salt = decode_hex_array::<KEYSTORE_SALT_BYTES>("kdf.salt_hex", &kdf.salt_hex)?;
    let params = Params::new(
        kdf.memory_kib,
        kdf.iterations,
        kdf.lanes,
        Some(KEYSTORE_DERIVED_KEY_BYTES),
    )
    .map_err(|_| WalletKeystoreCryptoError::KdfFailed)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = Zeroizing::new([0_u8; KEYSTORE_DERIVED_KEY_BYTES]);
    argon2
        .hash_password_into(password.expose_secret().as_bytes(), &salt, &mut output[..])
        .map_err(|_| WalletKeystoreCryptoError::KdfFailed)?;
    Ok(output)
}

fn ensure_seed_matches_address(
    seed: &WalletSeed,
    network_profile: &str,
    chain_id: &str,
    expected_address: &str,
) -> Result<(), WalletKeystoreCryptoError> {
    let network = WalletNetworkContext::new(network_profile, chain_id)
        .map_err(|_| WalletKeystoreCryptoError::InvalidSecretPayload)?;
    let derived = derive_wallet_key_from_seed(seed, &network, 0, WalletDerivationBranch::Receive, 0)
        .map_err(|_| WalletKeystoreCryptoError::InvalidSecretPayload)?;
    if derived.address() == expected_address {
        return Ok(());
    }
    Err(WalletKeystoreCryptoError::AddressKeyMismatch {
        expected_address: expected_address.to_string(),
        derived_address: derived.address().to_string(),
    })
}

fn decode_seed_payload(plaintext: &[u8]) -> Result<WalletSeed, WalletKeystoreCryptoError> {
    if plaintext.len() != KEYSTORE_V2_PLAINTEXT_BYTES
        || &plaintext[..KEYSTORE_V2_SECRET_MAGIC.len()] != KEYSTORE_V2_SECRET_MAGIC
    {
        return Err(WalletKeystoreCryptoError::InvalidSecretPayload);
    }
    let seed_bytes: [u8; WALLET_SEED_BYTES] = plaintext[KEYSTORE_V2_SECRET_MAGIC.len()..]
        .try_into()
        .map_err(|_| WalletKeystoreCryptoError::InvalidSecretPayload)?;
    Ok(WalletSeed::from_bytes(seed_bytes))
}

#[derive(Serialize)]
struct WalletSeedKeystoreAad<'a> {
    domain: &'static str,
    secret_kind: &'static str,
    format: &'a str,
    version: u32,
    network_profile: &'a str,
    chain_id: &'a str,
    address: &'a str,
    kdf: &'a WalletKdfMetadata,
    cipher: &'a WalletCipherMetadata,
}

fn authenticated_seed_metadata(
    envelope: &WalletKeystoreEnvelope,
) -> Result<Vec<u8>, WalletKeystoreCryptoError> {
    serde_json::to_vec(&WalletSeedKeystoreAad {
        domain: KEYSTORE_SEED_AAD_DOMAIN,
        secret_kind: KEYSTORE_SEED_SECRET_KIND,
        format: &envelope.format,
        version: envelope.version,
        network_profile: &envelope.network_profile,
        chain_id: &envelope.chain_id,
        address: &envelope.address,
        kdf: &envelope.kdf,
        cipher: &envelope.cipher,
    })
    .map_err(|_| WalletKeystoreCryptoError::AadEncodingFailed)
}

fn decode_hex_array<const N: usize>(
    field: &'static str,
    value: &str,
) -> Result<[u8; N], WalletKeystoreCryptoError> {
    require_hex_len(field, value, N)?;
    let decoded = hex::decode(value).map_err(|_| {
        WalletKeystoreCryptoError::Format(invalid(field, "must contain hexadecimal data only"))
    })?;
    decoded
        .try_into()
        .map_err(|_| WalletKeystoreCryptoError::Format(invalid(field, "unexpected encoded length")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        wallet_seed_from_mnemonic, SecretString, WalletDerivationBranch, WalletNetworkContext,
        KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_LANES, KEYSTORE_KDF_MIN_MEMORY_KIB,
    };

    const VECTOR_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const TEST_PASSWORD: &str = "seed-wallet-password";

    fn fixture() -> (WalletSeed, WalletNetworkContext, String) {
        let mnemonic = SecretString::new(VECTOR_MNEMONIC);
        let seed = wallet_seed_from_mnemonic(&mnemonic, None).expect("seed");
        let network = WalletNetworkContext::new(
            "public-testnet-v2.4.0-candidate",
            "pulsedag-public-testnet-v2.4.0-candidate",
        )
        .expect("network");
        let address = derive_wallet_key_from_seed(
            &seed,
            &network,
            0,
            WalletDerivationBranch::Receive,
            0,
        )
        .expect("anchor")
        .address()
        .to_string();
        (seed, network, address)
    }

    fn encrypted_fixture() -> WalletKeystoreEnvelope {
        let (seed, network, address) = fixture();
        encrypt_wallet_seed_with_material(
            network.network_profile(),
            network.chain_id(),
            &address,
            &seed,
            &SecretString::new(TEST_PASSWORD),
            SeedKdfMaterial {
                costs: SeedKeystoreKdfCosts::new(
                    KEYSTORE_KDF_MIN_MEMORY_KIB,
                    KEYSTORE_KDF_MIN_ITERATIONS,
                    KEYSTORE_KDF_MIN_LANES,
                ),
                salt: [0x31; KEYSTORE_SALT_BYTES],
            },
            [0x42; KEYSTORE_NONCE_BYTES],
        )
        .expect("encrypt fixture")
    }

    #[test]
    fn seed_keystore_round_trips_without_plaintext_seed_or_mnemonic() {
        let envelope = encrypted_fixture();
        assert_eq!(envelope.version, KEYSTORE_SEED_VERSION);
        let seed = decrypt_wallet_seed(&envelope, &SecretString::new(TEST_PASSWORD))
            .expect("decrypt seed");
        let (expected, network, _) = fixture();
        assert_eq!(seed.expose_secret(), expected.expose_secret());

        let derived = derive_wallet_key_from_seed(
            &seed,
            &network,
            0,
            WalletDerivationBranch::Change,
            2,
        )
        .expect("derive after reopen");
        assert_eq!(derived.address(), "pulse116db0da992b6a80cb5aa9541fa63eb404755f183");

        let serialized = serde_json::to_string(&envelope).expect("serialize");
        assert!(!serialized.contains(VECTOR_MNEMONIC));
        assert!(!serialized.contains(&hex::encode(seed.expose_secret())));
    }

    #[test]
    fn wrong_password_tamper_and_wrong_anchor_fail_closed() {
        let envelope = encrypted_fixture();
        assert!(matches!(
            decrypt_wallet_seed(&envelope, &SecretString::new("wrong-password")),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        ));

        let mut tampered = envelope.clone();
        tampered.address.push('x');
        assert!(matches!(
            decrypt_wallet_seed(&tampered, &SecretString::new(TEST_PASSWORD)),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        ));

        let (seed, network, _) = fixture();
        assert!(matches!(
            encrypt_wallet_seed_with_kdf_costs(
                network.network_profile(),
                network.chain_id(),
                "pulse1wronganchor",
                &seed,
                &SecretString::new(TEST_PASSWORD),
                SeedKeystoreKdfCosts::new(
                    KEYSTORE_KDF_MIN_MEMORY_KIB,
                    KEYSTORE_KDF_MIN_ITERATIONS,
                    KEYSTORE_KDF_MIN_LANES,
                ),
            ),
            Err(WalletKeystoreCryptoError::AddressKeyMismatch { .. })
        ));
    }
}
