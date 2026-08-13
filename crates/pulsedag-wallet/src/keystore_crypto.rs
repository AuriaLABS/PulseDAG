use std::{error::Error, fmt};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    keystore::{
        invalid, require_hex_len, require_nonempty, validate_kdf_metadata, WalletCipherMetadata,
        WalletKdfMetadata, WalletKeystoreEnvelope, WalletKeystoreFormatError,
        KEYSTORE_CIPHER_XCHACHA20_POLY1305, KEYSTORE_DERIVED_KEY_BYTES, KEYSTORE_FORMAT,
        KEYSTORE_KDF_ARGON2ID, KEYSTORE_KDF_DEFAULT_ITERATIONS, KEYSTORE_KDF_DEFAULT_LANES,
        KEYSTORE_KDF_DEFAULT_MEMORY_KIB, KEYSTORE_NONCE_BYTES, KEYSTORE_SALT_BYTES,
        KEYSTORE_V1_CIPHERTEXT_BYTES, KEYSTORE_V1_PLAINTEXT_BYTES, KEYSTORE_VERSION,
    },
    secrets::{SecretString, WalletSecretKey, ED25519_SECRET_KEY_BYTES},
};

const KEYSTORE_AAD_DOMAIN: &str = "PulseDAG:keystore-aad:v1";
const KEYSTORE_SECRET_KIND: &str = "ed25519-secret-key";
const KEYSTORE_V1_SECRET_MAGIC: &[u8; 16] = b"PULSEDAG-KEY-V1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletKeystoreCryptoError {
    Format(WalletKeystoreFormatError),
    EmptyPassword,
    RandomnessUnavailable,
    KdfFailed,
    CipherInitializationFailed,
    EncryptionFailed,
    AuthenticationFailed,
    InvalidSecretPayload,
    AadEncodingFailed,
}

impl fmt::Display for WalletKeystoreCryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(f, "invalid PulseDAG keystore: {error}"),
            Self::EmptyPassword => f.write_str("wallet keystore password must not be empty"),
            Self::RandomnessUnavailable => {
                f.write_str("operating-system randomness is unavailable")
            }
            Self::KdfFailed => f.write_str("wallet keystore key derivation failed"),
            Self::CipherInitializationFailed => {
                f.write_str("wallet keystore cipher initialization failed")
            }
            Self::EncryptionFailed => f.write_str("wallet keystore encryption failed"),
            Self::AuthenticationFailed => f.write_str("wallet keystore authentication failed"),
            Self::InvalidSecretPayload => f.write_str("wallet keystore secret payload is invalid"),
            Self::AadEncodingFailed => {
                f.write_str("wallet keystore authenticated metadata encoding failed")
            }
        }
    }
}

impl Error for WalletKeystoreCryptoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WalletKeystoreFormatError> for WalletKeystoreCryptoError {
    fn from(value: WalletKeystoreFormatError) -> Self {
        Self::Format(value)
    }
}

#[derive(Clone, Copy)]
struct KeystoreContext<'a> {
    network_profile: &'a str,
    chain_id: &'a str,
    address: &'a str,
}

#[derive(Clone, Copy)]
struct KdfMaterial {
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
    salt: [u8; KEYSTORE_SALT_BYTES],
}

/// Encrypt one Ed25519 secret key using the v1 production KDF policy and OS
/// CSPRNG salt/nonce generation. All public envelope metadata is authenticated.
pub fn encrypt_private_key(
    network_profile: &str,
    chain_id: &str,
    address: &str,
    secret_key: &WalletSecretKey,
    password: &SecretString,
) -> Result<WalletKeystoreEnvelope, WalletKeystoreCryptoError> {
    if password.is_empty() {
        return Err(WalletKeystoreCryptoError::EmptyPassword);
    }

    let mut salt = [0_u8; KEYSTORE_SALT_BYTES];
    let mut nonce = [0_u8; KEYSTORE_NONCE_BYTES];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut salt)
        .map_err(|_| WalletKeystoreCryptoError::RandomnessUnavailable)?;
    rng.try_fill_bytes(&mut nonce)
        .map_err(|_| WalletKeystoreCryptoError::RandomnessUnavailable)?;

    encrypt_private_key_with_material(
        KeystoreContext {
            network_profile,
            chain_id,
            address,
        },
        secret_key,
        password,
        KdfMaterial {
            memory_kib: KEYSTORE_KDF_DEFAULT_MEMORY_KIB,
            iterations: KEYSTORE_KDF_DEFAULT_ITERATIONS,
            lanes: KEYSTORE_KDF_DEFAULT_LANES,
            salt,
        },
        nonce,
    )
}

/// Authenticate and decrypt one Ed25519 secret key. Wrong passwords and any
/// AEAD-protected metadata/ciphertext tampering intentionally collapse to the
/// same `AuthenticationFailed` error.
pub fn decrypt_private_key(
    envelope: &WalletKeystoreEnvelope,
    password: &SecretString,
) -> Result<WalletSecretKey, WalletKeystoreCryptoError> {
    envelope.validate_structure()?;
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
    let aad = authenticated_metadata(envelope)?;
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

    decode_secret_payload(&plaintext)
}

fn encrypt_private_key_with_material(
    context: KeystoreContext<'_>,
    secret_key: &WalletSecretKey,
    password: &SecretString,
    kdf_material: KdfMaterial,
    nonce: [u8; KEYSTORE_NONCE_BYTES],
) -> Result<WalletKeystoreEnvelope, WalletKeystoreCryptoError> {
    require_nonempty("network_profile", context.network_profile)?;
    require_nonempty("chain_id", context.chain_id)?;
    require_nonempty("address", context.address)?;
    if password.is_empty() {
        return Err(WalletKeystoreCryptoError::EmptyPassword);
    }

    let kdf = WalletKdfMetadata {
        algorithm: KEYSTORE_KDF_ARGON2ID.to_string(),
        memory_kib: kdf_material.memory_kib,
        iterations: kdf_material.iterations,
        lanes: kdf_material.lanes,
        salt_hex: hex::encode(kdf_material.salt),
    };
    validate_kdf_metadata(&kdf)?;
    let cipher_metadata = WalletCipherMetadata {
        algorithm: KEYSTORE_CIPHER_XCHACHA20_POLY1305.to_string(),
        nonce_hex: hex::encode(nonce),
    };
    let mut envelope = WalletKeystoreEnvelope {
        format: KEYSTORE_FORMAT.to_string(),
        version: KEYSTORE_VERSION,
        network_profile: context.network_profile.to_string(),
        chain_id: context.chain_id.to_string(),
        address: context.address.to_string(),
        kdf,
        cipher: cipher_metadata,
        ciphertext_hex: String::new(),
    };

    let aad = authenticated_metadata(&envelope)?;
    let derived_key = derive_key(password, &envelope.kdf)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&derived_key[..])
        .map_err(|_| WalletKeystoreCryptoError::CipherInitializationFailed)?;
    let mut secret_payload = Zeroizing::new([0_u8; KEYSTORE_V1_PLAINTEXT_BYTES]);
    secret_payload[..KEYSTORE_V1_SECRET_MAGIC.len()].copy_from_slice(KEYSTORE_V1_SECRET_MAGIC);
    secret_payload[KEYSTORE_V1_SECRET_MAGIC.len()..].copy_from_slice(secret_key.expose_secret());

    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &secret_payload[..],
                aad: &aad,
            },
        )
        .map_err(|_| WalletKeystoreCryptoError::EncryptionFailed)?;
    if ciphertext.len() != KEYSTORE_V1_CIPHERTEXT_BYTES {
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

fn decode_secret_payload(plaintext: &[u8]) -> Result<WalletSecretKey, WalletKeystoreCryptoError> {
    if plaintext.len() != KEYSTORE_V1_PLAINTEXT_BYTES
        || &plaintext[..KEYSTORE_V1_SECRET_MAGIC.len()] != KEYSTORE_V1_SECRET_MAGIC
    {
        return Err(WalletKeystoreCryptoError::InvalidSecretPayload);
    }
    let secret_bytes: [u8; ED25519_SECRET_KEY_BYTES] = plaintext[KEYSTORE_V1_SECRET_MAGIC.len()..]
        .try_into()
        .map_err(|_| WalletKeystoreCryptoError::InvalidSecretPayload)?;
    Ok(WalletSecretKey::from_bytes(secret_bytes))
}

#[derive(Serialize)]
struct WalletKeystoreAad<'a> {
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

fn authenticated_metadata(
    envelope: &WalletKeystoreEnvelope,
) -> Result<Vec<u8>, WalletKeystoreCryptoError> {
    serde_json::to_vec(&WalletKeystoreAad {
        domain: KEYSTORE_AAD_DOMAIN,
        secret_kind: KEYSTORE_SECRET_KIND,
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
    use std::sync::OnceLock;

    use super::*;
    use crate::keystore::{
        KEYSTORE_KDF_MAX_MEMORY_KIB, KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_MEMORY_KIB,
    };

    const TEST_PASSWORD: &str = "test-wallet-password";

    fn encrypted_fixture() -> WalletKeystoreEnvelope {
        static FIXTURE: OnceLock<WalletKeystoreEnvelope> = OnceLock::new();
        FIXTURE
            .get_or_init(|| {
                let key = WalletSecretKey::from_bytes([0x42; ED25519_SECRET_KEY_BYTES]);
                encrypt_private_key_with_material(
                    KeystoreContext {
                        network_profile: "public-testnet-v2.4.0-candidate",
                        chain_id: "pulsedag-public-testnet-v2.4.0-candidate",
                        address: "pulse1exampleaddress",
                    },
                    &key,
                    &SecretString::new(TEST_PASSWORD),
                    KdfMaterial {
                        memory_kib: KEYSTORE_KDF_MIN_MEMORY_KIB,
                        iterations: KEYSTORE_KDF_MIN_ITERATIONS,
                        lanes: 1,
                        salt: [0x11; KEYSTORE_SALT_BYTES],
                    },
                    [0x22; KEYSTORE_NONCE_BYTES],
                )
                .expect("fixture encrypts")
            })
            .clone()
    }

    #[test]
    fn encrypted_private_key_round_trips() {
        let envelope = encrypted_fixture();
        let recovered = decrypt_private_key(&envelope, &SecretString::new(TEST_PASSWORD))
            .expect("correct password decrypts");
        assert_eq!(recovered.expose_secret(), &[0x42; ED25519_SECRET_KEY_BYTES]);
        assert_eq!(
            hex::decode(&envelope.ciphertext_hex)
                .expect("ciphertext hex")
                .len(),
            KEYSTORE_V1_CIPHERTEXT_BYTES
        );
    }

    #[test]
    fn wrong_password_and_ciphertext_tamper_share_authentication_failure() {
        let envelope = encrypted_fixture();
        assert!(matches!(
            decrypt_private_key(&envelope, &SecretString::new("wrong-password")),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        ));

        let mut tampered = envelope;
        let mut ciphertext = hex::decode(&tampered.ciphertext_hex).expect("ciphertext hex");
        ciphertext[0] ^= 0x01;
        tampered.ciphertext_hex = hex::encode(ciphertext);
        assert!(matches!(
            decrypt_private_key(&tampered, &SecretString::new(TEST_PASSWORD)),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        ));
    }

    #[test]
    fn authenticated_metadata_tamper_fails_closed() {
        let mut envelope = encrypted_fixture();
        envelope.chain_id.push_str("-tampered");
        assert!(matches!(
            decrypt_private_key(&envelope, &SecretString::new(TEST_PASSWORD)),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        ));
    }

    #[test]
    fn production_encryption_uses_v1_defaults_and_random_material() {
        let key = WalletSecretKey::from_bytes([0x24; ED25519_SECRET_KEY_BYTES]);
        let envelope = encrypt_private_key(
            "public-testnet-v2.4.0-candidate",
            "pulsedag-public-testnet-v2.4.0-candidate",
            "pulse1productionexample",
            &key,
            &SecretString::new("production-test-password"),
        )
        .expect("production envelope encrypts");

        assert_eq!(envelope.kdf.memory_kib, KEYSTORE_KDF_DEFAULT_MEMORY_KIB);
        assert_eq!(envelope.kdf.iterations, KEYSTORE_KDF_DEFAULT_ITERATIONS);
        assert_eq!(envelope.kdf.lanes, KEYSTORE_KDF_DEFAULT_LANES);
        assert_eq!(
            envelope.kdf.salt_hex.len(),
            KEYSTORE_SALT_BYTES.saturating_mul(2)
        );
        assert_eq!(
            envelope.cipher.nonce_hex.len(),
            KEYSTORE_NONCE_BYTES.saturating_mul(2)
        );
        envelope
            .validate_structure()
            .expect("valid production envelope");
    }

    #[test]
    fn argon2id_v19_known_answer_vector_is_stable() {
        let salt: [u8; KEYSTORE_SALT_BYTES] =
            std::array::from_fn(|index| u8::try_from(index).expect("salt index fits u8"));
        let kdf = WalletKdfMetadata {
            algorithm: KEYSTORE_KDF_ARGON2ID.to_string(),
            memory_kib: 65_536,
            iterations: 3,
            lanes: 1,
            salt_hex: hex::encode(salt),
        };
        let derived = derive_key(&SecretString::new("pulsedag-test-password"), &kdf)
            .expect("known-answer KDF derives");
        assert_eq!(
            hex::encode(&derived[..]),
            "0314fb96baacd47016eec70a9766498282ef6fce7934d38d92117736cd0f2bfb"
        );
    }

    #[test]
    fn oversized_kdf_cost_fails_before_decryption() {
        let mut envelope = encrypted_fixture();
        envelope.kdf.memory_kib = KEYSTORE_KDF_MAX_MEMORY_KIB + 1;
        assert!(matches!(
            decrypt_private_key(&envelope, &SecretString::new(TEST_PASSWORD)),
            Err(WalletKeystoreCryptoError::Format(
                WalletKeystoreFormatError::InvalidField {
                    field: "kdf.memory_kib",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn empty_password_is_rejected() {
        let key = WalletSecretKey::from_bytes([0x55; ED25519_SECRET_KEY_BYTES]);
        assert!(matches!(
            encrypt_private_key(
                "public-testnet",
                "pulsedag-public-testnet",
                "pulse1example",
                &key,
                &SecretString::new("")
            ),
            Err(WalletKeystoreCryptoError::EmptyPassword)
        ));
        assert!(matches!(
            decrypt_private_key(&encrypted_fixture(), &SecretString::new("")),
            Err(WalletKeystoreCryptoError::EmptyPassword)
        ));
    }

    #[test]
    fn serialized_envelope_has_no_plaintext_secret_material() {
        let encoded = serde_json::to_string(&encrypted_fixture()).expect("serialize envelope");
        for forbidden in [
            "private_key",
            "mnemonic",
            "seed_phrase",
            "password",
            "secret_key",
            TEST_PASSWORD,
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }
}
