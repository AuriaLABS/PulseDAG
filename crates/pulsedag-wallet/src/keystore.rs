use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::secrets::ED25519_SECRET_KEY_BYTES;

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

impl WalletKeystoreEnvelope {
    /// Validate every public field before any expensive KDF work.
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

pub(crate) fn validate_kdf_metadata(
    kdf: &WalletKdfMetadata,
) -> Result<(), WalletKeystoreFormatError> {
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

pub(crate) fn invalid(
    field: &'static str,
    reason: &'static str,
) -> WalletKeystoreFormatError {
    WalletKeystoreFormatError::InvalidField { field, reason }
}

pub(crate) fn require_nonempty(
    field: &'static str,
    value: &str,
) -> Result<(), WalletKeystoreFormatError> {
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

pub(crate) fn require_hex_len(
    field: &'static str,
    value: &str,
    expected_bytes: usize,
) -> Result<(), WalletKeystoreFormatError> {
    if value.len() != expected_bytes.saturating_mul(2) {
        return Err(invalid(field, "unexpected encoded length"));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn malformed_noncanonical_and_unknown_fields_fail_closed() {
        let mut envelope = sample_envelope();
        envelope.cipher.nonce_hex = "00".repeat(KEYSTORE_NONCE_BYTES - 1);
        assert!(envelope.validate_structure().is_err());

        let mut envelope = sample_envelope();
        envelope.kdf.salt_hex.replace_range(0..2, "AB");
        assert!(matches!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::InvalidField {
                field: "kdf.salt_hex",
                ..
            })
        ));

        let mut value = serde_json::to_value(sample_envelope()).expect("serialize envelope");
        value.as_object_mut().expect("envelope object").insert(
            "future_secret_hint".to_string(),
            serde_json::json!("ignored?"),
        );
        assert!(serde_json::from_value::<WalletKeystoreEnvelope>(value).is_err());
    }

    #[test]
    fn kdf_costs_are_bounded_before_crypto() {
        let mut envelope = sample_envelope();
        envelope.kdf.memory_kib = KEYSTORE_KDF_MAX_MEMORY_KIB + 1;
        assert!(matches!(
            envelope.validate_structure(),
            Err(WalletKeystoreFormatError::InvalidField {
                field: "kdf.memory_kib",
                ..
            })
        ));

        let mut envelope = sample_envelope();
        envelope.kdf.iterations = KEYSTORE_KDF_MIN_ITERATIONS - 1;
        assert!(envelope.validate_structure().is_err());
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
