use std::{error::Error, fmt};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::secrets::{SecretString, WalletSecretKey, ED25519_SECRET_KEY_BYTES};

pub const KEYSTORE_FORMAT: &str = "pulsedag-keystore";
pub const KEYSTORE_VERSION: u32 = 1;
pub const KEYSTORE_KDF_ARGON2ID: &str = "argon2id";
pub const KEYSTORE_CIPHER_XCHACHA20_POLY1305: &str = "xchacha20-poly1305";
pub const KEYSTORE_SALT_BYTES: usize = 16;
pub const KEYSTORE_NONCE_BYTES: usize = 24;
pub const KEYSTORE_DERIVED_KEY_BYTES: usize = 32;
pub const KEYSTORE_V1_PLAINTEXT_BYTES: usize = 16 + ED25519_SECRET_KEY_BYTES;
pub const KEYSTORE_V1_CIPHERTEXT_BYTES: usize = KEYSTORE_V1_PLAINTEXT_BYTES + 16;
pub const KEYSTORE_MIN_CIPHERTEXT_BYTES: usize = KEYSTORE_V1_CIPHERTEXT_BYTES;

pub const KEYSTORE_KDF_DEFAULT_MEMORY_KIB: u32 = 65_536;
pub const KEYSTORE_KDF_DEFAULT_ITERATIONS: u32 = 3;
pub const KEYSTORE_KDF_DEFAULT_LANES: u32 = 1;
pub const KEYSTORE_KDF_MIN_MEMORY_KIB: u32 = 32_768;
pub const KEYSTORE_KDF_MAX_MEMORY_KIB: u32 = 262_144;
pub const KEYSTORE_KDF_MIN_ITERATIONS: u32 = 2;
pub const KEYSTORE_KDF_MAX_ITERATIONS: u32 = 10;
pub const KEYSTORE_KDF_MIN_LANES: u32 = 1;
pub const KEYSTORE_KDF_MAX_LANES: u32 = 4;

const KEYSTORE_AAD_DOMAIN: &str = "PulseDAG:keystore-aad:v1";
const KEYSTORE_SECRET_KIND: &str = "ed25519-secret-key";
const KEYSTORE_V1_SECRET_MAGIC: &[u8; 16] = b"PULSEDAG-KEY-V1\0";

/// Public, versioned metadata plus authenticated ciphertext for a PulseDAG
/// wallet secret. The envelope contains no plaintext password or private key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletKeystoreEnvelope {
    pub format: String,
    pub version: u32,
    pub network_profile: String,
    pub chain_id: String,
    pub address: String,
    pub kdf: WalletKdfMetadata,
    pub cipher: WalletCipherMetadata,
    pub ciphertext_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletKdfMetadata {
    pub algorithm: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub lanes: u32,
    pub salt_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletCipherMetadata {
    pub algorithm: String,
    pub nonce_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletKeystoreFormatError {
    UnsupportedFormat,
    UnsupportedVersion(u32),
    UnsupportedKdf,
    UnsupportedCipher,
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for WalletKeystoreFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("unsupported PulseDAG keystore format"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported PulseDAG keystore version {version}")
            }
            Self::UnsupportedKdf => f.write_str("unsupported PulseDAG keystore KDF"),
            Self::UnsupportedCipher => f.write_str("unsupported PulseDAG keystore cipher"),
            Self::InvalidField { field, reason } => {
                write!(f, "invalid PulseDAG keystore field {field}: {reason}")
            }
        }
    }
}

impl Error for WalletKeystoreFormatError {}

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
            Self::AuthenticationFailed => {
                f.write_str("wallet keystore authentication failed")
            }
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

impl WalletKeystoreEnvelope {
    /// Validate all public metadata before any expensive KDF work.
    pub fn validate_structure(&self) -> Result<(), WalletKeystoreFormatError> {
        if self.format != KEYSTORE_FORMAT {
            return Err(WalletKeystoreFormatError::UnsupportedFormat);
        }
        if self.version != KEYSTORE_VERSION {
            return Err(WalletKeystoreFormatError::UnsupportedVersion(self.version));
        }
        require_nonempty("network_profile", &self.network_profile)?;
        require_nonempty("chain_id", &self.chain_id)?;
        require_nonempty("address", &self.address)?;
        validate_kdf_metadata(&self.kdf)?;

        if self.cipher.algorithm != KEYSTORE_CIPHER_XCHACHA20_POLY1305 {
            return Err(WalletKeystoreFormatError::UnsupportedCipher);
        }
        require_hex_len(
            "cipher.nonce_hex",
            &self.cipher.nonce_hex,
            KEYSTORE_NONCE_BYTES,
        )?;
        require_hex_len(
            "ciphertext_hex",
            &self.ciphertext_hex,
            KEYSTORE_V1_CIPHERTEXT_BYTES,
        )?;
        Ok(())
    }
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
        network_profile,
        chain_id,
        address,
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
    let nonce = decode_hex_array::<KEYSTORE_NONCE_BYTES>(
        "cipher.nonce_hex",
        &envelope.cipher.nonce_hex,
    )?;
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

#[derive(Clone, Copy)]
struct KdfMaterial {
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
    salt: [u8; KEYSTORE_SALT_BYTES],
}

fn encrypt_private_key_with_material(
    network_profile: &str,
    chain_id: &str,
    address: &str,
    secret_key: &WalletSecretKey,
    password: &SecretString,
    kdf_material: KdfMaterial,
    nonce: [u8; KEYSTORE_NONCE_BYTES],
) -> Result<WalletKeystoreEnvelope, WalletKeystoreCryptoError> {
    require_nonempty("network_profile", network_profile)?;
    require_nonempty("chain_id", chain_id)?;
    require_nonempty("address", address)?;
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
        network_profile: network_profile.to_string(),
        chain_id: chain_id.to_string(),
        address: address.to_string(),
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
    secret_payload[KEYSTORE_V1_SECRET_MAGIC.len()..]
        .copy_from_slice(secret_key.expose_secret());

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

fn decode_secret_payload(
    plaintext: &[u8],
) -> Result<WalletSecretKey, WalletKeystoreCryptoError> {
    if plaintext.len() != KEYSTORE_V1_PLAINTEXT_BYTES
        || &plaintext[..KEYSTORE_V1_SECRET_MAGIC.len()] != KEYSTORE_V1_SECRET_MAGIC
    {
        return Err(WalletKeystoreCryptoError::InvalidSecretPayload);
    }
    let secret_bytes: [u8; ED25519_SECRET_KEY_BYTES] = plaintext
        [KEYSTORE_V1_SECRET_MAGIC.len()..]
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
    let aad = WalletKeystoreAad {
        domain: KEYSTORE_AAD_DOMAIN,
        secret_kind: KEYSTORE_SECRET_KIND,
        format: &envelope.format,
        version: envelope.version,
        network_profile: &envelope.network_profile,
        chain_id: &envelope.chain_id,
        address: &envelope.address,
        kdf: &envelope.kdf,
        cipher: &envelope.cipher,
    };
    serde_json::to_vec(&aad).map_err(|_| WalletKeystoreCryptoError::AadEncodingFailed)
}

fn validate_kdf_metadata(kdf: &WalletKdfMetadata) -> Result<(), WalletKeystoreFormatError> {
    if kdf.algorithm != KEYSTORE_KDF_ARGON2ID {
        return Err(WalletKeystoreFormatError::UnsupportedKdf);
    }
    if !(KEYSTORE_KDF_MIN_MEMORY_KIB..=KEYSTORE_KDF_MAX_MEMORY_KIB)
        .contains(&kdf.memory_kib)
    {
        return Err(invalid(
            "kdf.memory_kib",
            "outside supported v1 memory-cost bounds",
        ));
    }
    if !(KEYSTORE_KDF_MIN_ITERATIONS..=KEYSTORE_KDF_MAX_ITERATIONS)
        .contains(&kdf.iterations)
    {
        return Err(invalid(
            "kdf.iterations",
            "outside supported v1 iteration-count bounds",
        ));
    }
    if !(KEYSTORE_KDF_MIN_LANES..=KEYSTORE_KDF_MAX_LANES).contains(&kdf.lanes) {
        return Err(invalid(
            "kdf.lanes",
            "outside supported v1 lane-count bounds",
        ));
    }
    require_hex_len("kdf.salt_hex", &kdf.salt_hex, KEYSTORE_SALT_BYTES)
}

fn invalid(field: &'static str, reason: &'static str) -> WalletKeystoreFormatError {
    WalletKeystoreFormatError::InvalidField { field, reason }
}

fn require_nonempty(field: &'static str, value: &str) -> Result<(), WalletKeystoreFormatError> {
    if value.is_empty() {
        return Err(invalid(field, "must not be empty"));
    }
    if value.trim() != value {
        return Err(invalid(
            field,
            "must not contain leading or trailing whitespace",
        ));
    }
    Ok(())
}

fn require_hex_len(
    field: &'static str,
    value: &str,
    expected_bytes: usize,
) -> Result<(), WalletKeystoreFormatError> {
    if value.len() != expected_bytes.saturating_mul(2) {
        return Err(invalid(field, "unexpected encoded length"));
    }
    require_canonical_hex(field, value)
}

fn require_canonical_hex(
    field: &'static str,
    value: &str,
) -> Result<(), WalletKeystoreFormatError> {
    let decoded = hex::decode(value)
        .map_err(|_| invalid(field, "must contain hexadecimal data only"))?;
    if hex::encode(decoded) != value {
        return Err(invalid(
            field,
            "must use canonical lowercase hexadecimal encoding",
        ));
    }
    Ok(())
}

fn decode_hex_array<const N: usize>(
    field: &'static str,
    value: &str,
) -> Result<[u8; N], WalletKeystoreCryptoError> {
    require_hex_len(field, value, N)?;
    let decoded = hex::decode(value).map_err(|_| {
        WalletKeystoreCryptoError::Format(invalid(field, "must contain hexadecimal data only"))
    })?;
    decoded.try_into().map_err(|_| {
        WalletKeystoreCryptoError::Format(invalid(field, "unexpected encoded length"))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;

    const TEST_PASSWORD: &str = "test-wallet-password";
    const TEST_MEMORY_KIB: u32 = KEYSTORE_KDF_MIN_MEMORY_KIB;
    const TEST_ITERATIONS: u32 = KEYSTORE_KDF_MIN_ITERATIONS;

    fn sample_envelope() -> WalletKeystoreEnvelope {
        WalletKeystoreEnvelope {
            format: KEYSTORE_FORMAT.to_string(),
            version: KEYSTORE_VERSION,
            network_profile: "public-testnet-v2.4.0-candidate".to_string(),
            chain_id: "pulsedag-public-testnet-v2.4.0-candidate".to_string(),
            address: "pulse1exampleaddress".to_string(),
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

    fn encrypted_fixture() -> WalletKeystoreEnvelope {
        static FIXTURE: OnceLock<WalletKeystoreEnvelope> = OnceLock::new();
        FIXTURE
            .get_or_init(|| {
                let key = WalletSecretKey::from_bytes([0x42; ED25519_SECRET_KEY_BYTES]);
                encrypt_private_key_with_material(
                    "public-testnet-v2.4.0-candidate",
                    "pulsedag-public-testnet-v2.4.0-candidate",
                    "pulse1exampleaddress",
                    &key,
                    &SecretString::new(TEST_PASSWORD),
                    KdfMaterial {
                        memory_kib: TEST_MEMORY_KIB,
                        iterations: TEST_ITERATIONS,
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
    fn valid_v1_envelope_round_trips_structurally() {
        let envelope = sample_envelope();
        envelope.validate_structure().expect("valid envelope");
        let encoded = serde_json::to_string(&envelope).expect("serialize envelope");
        let decoded: WalletKeystoreEnvelope =
            serde_json::from_str(&encoded).expect("deserialize envelope");
        assert_eq!(decoded, envelope);
        decoded.validate_structure().expect("decoded envelope");
    }

    #[test]
    fn encrypted_private_key_round_trips() {
        let envelope = encrypted_fixture();
        let recovered = decrypt_private_key(&envelope, &SecretString::new(TEST_PASSWORD))
            .expect("correct password decrypts");
        assert_eq!(
            recovered.expose_secret(),
            &[0x42; ED25519_SECRET_KEY_BYTES]
        );
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
        assert_eq!(
            decrypt_private_key(&envelope, &SecretString::new("wrong-password")),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        );

        let mut tampered = envelope;
        let mut ciphertext = hex::decode(&tampered.ciphertext_hex).expect("ciphertext hex");
        ciphertext[0] ^= 0x01;
        tampered.ciphertext_hex = hex::encode(ciphertext);
        assert_eq!(
            decrypt_private_key(&tampered, &SecretString::new(TEST_PASSWORD)),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn authenticated_metadata_tamper_fails_closed() {
        let mut envelope = encrypted_fixture();
        envelope.chain_id.push_str("-tampered");
        assert_eq!(
            decrypt_private_key(&envelope, &SecretString::new(TEST_PASSWORD)),
            Err(WalletKeystoreCryptoError::AuthenticationFailed)
        );
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
        envelope.validate_structure().expect("valid production envelope");
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
    fn oversized_or_undersized_kdf_costs_fail_before_decryption() {
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

        let mut envelope = sample_envelope();
        envelope.kdf.iterations = KEYSTORE_KDF_MIN_ITERATIONS - 1;
        assert!(envelope.validate_structure().is_err());
    }

    #[test]
    fn empty_password_is_rejected() {
        let key = WalletSecretKey::from_bytes([0x55; ED25519_SECRET_KEY_BYTES]);
        assert_eq!(
            encrypt_private_key(
                "public-testnet",
                "pulsedag-public-testnet",
                "pulse1example",
                &key,
                &SecretString::new("")
            ),
            Err(WalletKeystoreCryptoError::EmptyPassword)
        );
        assert_eq!(
            decrypt_private_key(&encrypted_fixture(), &SecretString::new("")),
            Err(WalletKeystoreCryptoError::EmptyPassword)
        );
    }

    #[test]
    fn network_identity_and_unknown_fields_are_rejected_fail_closed() {
        let mut envelope = sample_envelope();
        envelope.network_profile.clear();
        assert!(matches!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::InvalidField {
                field: "network_profile",
                ..
            })
        ));

        let mut value = serde_json::to_value(sample_envelope()).expect("serialize envelope");
        value.as_object_mut().expect("envelope object").insert(
            "future_secret_hint".to_string(),
            serde_json::json!("ignored?"),
        );
        let encoded = serde_json::to_string(&value).expect("encode modified envelope");
        assert!(serde_json::from_str::<WalletKeystoreEnvelope>(&encoded).is_err());
    }

    #[test]
    fn serialized_schema_has_no_plaintext_secret_fields() {
        let encoded = serde_json::to_string(&encrypted_fixture()).expect("serialize envelope");
        for forbidden in [
            "private_key",
            "mnemonic",
            "seed_phrase",
            "password",
            "secret_key",
            TEST_PASSWORD,
        ] {
            assert!(
                !encoded.contains(forbidden),
                "keystore schema unexpectedly contains plaintext-secret material {forbidden}"
            );
        }
    }

    #[test]
    fn malformed_or_noncanonical_encoded_fields_fail_closed() {
        let mut envelope = sample_envelope();
        envelope.cipher.nonce_hex = "00".repeat(KEYSTORE_NONCE_BYTES - 1);
        assert!(envelope.validate_structure().is_err());

        let mut envelope = sample_envelope();
        envelope.kdf.salt_hex = envelope.kdf.salt_hex.to_uppercase();
        envelope.kdf.salt_hex.replace_range(0..2, "AB");
        assert!(matches!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::InvalidField {
                field: "kdf.salt_hex",
                ..
            })
        ));
    }

    #[test]
    fn unsupported_version_and_algorithms_fail_closed() {
        let mut envelope = sample_envelope();
        envelope.version = KEYSTORE_VERSION + 1;
        assert_eq!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::UnsupportedVersion(
                KEYSTORE_VERSION + 1
            ))
        );

        let mut envelope = sample_envelope();
        envelope.kdf.algorithm = "pbkdf2".to_string();
        assert_eq!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::UnsupportedKdf)
        );

        let mut envelope = sample_envelope();
        envelope.cipher.algorithm = "aes-cbc".to_string();
        assert_eq!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::UnsupportedCipher)
        );
    }
}
