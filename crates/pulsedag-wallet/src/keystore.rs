use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub const KEYSTORE_FORMAT: &str = "pulsedag-keystore";
pub const KEYSTORE_VERSION: u32 = 1;
pub const KEYSTORE_KDF_ARGON2ID: &str = "argon2id";
pub const KEYSTORE_CIPHER_XCHACHA20_POLY1305: &str = "xchacha20-poly1305";
pub const KEYSTORE_SALT_BYTES: usize = 16;
pub const KEYSTORE_NONCE_BYTES: usize = 24;
pub const KEYSTORE_MIN_CIPHERTEXT_BYTES: usize = 16;

/// Public, versioned metadata for a PulseDAG encrypted wallet file.
///
/// This envelope deliberately has no plaintext private-key, mnemonic, seed or
/// password fields. Encryption/decryption is implemented in a later #819
/// change after the reviewed Argon2id/XChaCha20-Poly1305 dependency update is
/// committed together with `Cargo.lock`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletKeystoreEnvelope {
    pub format: String,
    pub version: u32,
    pub network: String,
    pub address: String,
    pub kdf: WalletKdfMetadata,
    pub cipher: WalletCipherMetadata,
    pub ciphertext_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletKdfMetadata {
    pub algorithm: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub lanes: u32,
    pub salt_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

impl WalletKeystoreEnvelope {
    /// Validate the public structure of an encrypted keystore envelope.
    ///
    /// This is intentionally structural validation only. Successful validation
    /// does not mean the password is correct or the ciphertext authentic; AEAD
    /// authentication belongs to the encryption/decryption implementation.
    pub fn validate_structure(&self) -> Result<(), WalletKeystoreFormatError> {
        if self.format != KEYSTORE_FORMAT {
            return Err(WalletKeystoreFormatError::UnsupportedFormat);
        }
        if self.version != KEYSTORE_VERSION {
            return Err(WalletKeystoreFormatError::UnsupportedVersion(self.version));
        }
        require_nonempty("network", &self.network)?;
        require_nonempty("address", &self.address)?;

        if self.kdf.algorithm != KEYSTORE_KDF_ARGON2ID {
            return Err(WalletKeystoreFormatError::UnsupportedKdf);
        }
        if self.kdf.memory_kib == 0 {
            return Err(invalid("kdf.memory_kib", "must be greater than zero"));
        }
        if self.kdf.iterations == 0 {
            return Err(invalid("kdf.iterations", "must be greater than zero"));
        }
        if self.kdf.lanes == 0 {
            return Err(invalid("kdf.lanes", "must be greater than zero"));
        }
        require_hex_len("kdf.salt_hex", &self.kdf.salt_hex, KEYSTORE_SALT_BYTES)?;

        if self.cipher.algorithm != KEYSTORE_CIPHER_XCHACHA20_POLY1305 {
            return Err(WalletKeystoreFormatError::UnsupportedCipher);
        }
        require_hex_len(
            "cipher.nonce_hex",
            &self.cipher.nonce_hex,
            KEYSTORE_NONCE_BYTES,
        )?;
        require_hex_min_len(
            "ciphertext_hex",
            &self.ciphertext_hex,
            KEYSTORE_MIN_CIPHERTEXT_BYTES,
        )?;

        Ok(())
    }
}

fn invalid(field: &'static str, reason: &'static str) -> WalletKeystoreFormatError {
    WalletKeystoreFormatError::InvalidField { field, reason }
}

fn require_nonempty(field: &'static str, value: &str) -> Result<(), WalletKeystoreFormatError> {
    if value.trim().is_empty() {
        Err(invalid(field, "must not be empty"))
    } else {
        Ok(())
    }
}

fn require_hex_len(
    field: &'static str,
    value: &str,
    expected_bytes: usize,
) -> Result<(), WalletKeystoreFormatError> {
    if value.len() != expected_bytes.saturating_mul(2) {
        return Err(invalid(field, "unexpected encoded length"));
    }
    require_hex(field, value)
}

fn require_hex_min_len(
    field: &'static str,
    value: &str,
    minimum_bytes: usize,
) -> Result<(), WalletKeystoreFormatError> {
    if value.len() < minimum_bytes.saturating_mul(2) || value.len() % 2 != 0 {
        return Err(invalid(field, "encoded value is too short or malformed"));
    }
    require_hex(field, value)
}

fn require_hex(field: &'static str, value: &str) -> Result<(), WalletKeystoreFormatError> {
    if value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid(field, "must contain hexadecimal data only"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_envelope() -> WalletKeystoreEnvelope {
        WalletKeystoreEnvelope {
            format: KEYSTORE_FORMAT.to_string(),
            version: KEYSTORE_VERSION,
            network: "pulsedag-public-testnet-v2.4.0-candidate".to_string(),
            address: "pulse1exampleaddress".to_string(),
            kdf: WalletKdfMetadata {
                algorithm: KEYSTORE_KDF_ARGON2ID.to_string(),
                memory_kib: 65_536,
                iterations: 3,
                lanes: 1,
                salt_hex: "11".repeat(KEYSTORE_SALT_BYTES),
            },
            cipher: WalletCipherMetadata {
                algorithm: KEYSTORE_CIPHER_XCHACHA20_POLY1305.to_string(),
                nonce_hex: "22".repeat(KEYSTORE_NONCE_BYTES),
            },
            ciphertext_hex: "33".repeat(64),
        }
    }

    #[test]
    fn valid_v1_envelope_round_trips() {
        let envelope = sample_envelope();
        envelope.validate_structure().expect("valid envelope");

        let encoded = serde_json::to_string(&envelope).expect("serialize envelope");
        let decoded: WalletKeystoreEnvelope =
            serde_json::from_str(&encoded).expect("deserialize envelope");

        assert_eq!(decoded, envelope);
        decoded.validate_structure().expect("decoded envelope");
    }

    #[test]
    fn serialized_schema_has_no_plaintext_secret_fields() {
        let encoded = serde_json::to_string(&sample_envelope()).expect("serialize envelope");
        for forbidden in [
            "private_key",
            "mnemonic",
            "seed_phrase",
            "password",
            "secret_key",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "keystore schema unexpectedly contains plaintext-secret field {forbidden}"
            );
        }
    }

    #[test]
    fn unsupported_version_fails_closed() {
        let mut envelope = sample_envelope();
        envelope.version = KEYSTORE_VERSION + 1;
        assert_eq!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::UnsupportedVersion(
                KEYSTORE_VERSION + 1
            ))
        );
    }

    #[test]
    fn malformed_nonce_and_ciphertext_fail_closed() {
        let mut envelope = sample_envelope();
        envelope.cipher.nonce_hex = "00".repeat(KEYSTORE_NONCE_BYTES - 1);
        assert!(matches!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::InvalidField {
                field: "cipher.nonce_hex",
                ..
            })
        ));

        let mut envelope = sample_envelope();
        envelope.ciphertext_hex = "not-hex".to_string();
        assert!(matches!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::InvalidField {
                field: "ciphertext_hex",
                ..
            })
        ));
    }

    #[test]
    fn unsupported_algorithms_fail_closed() {
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
