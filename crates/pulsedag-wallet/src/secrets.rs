use std::fmt;
use zeroize::Zeroizing;

/// Stable marker used whenever wallet secret material is formatted.
pub const REDACTED_SECRET: &str = "[REDACTED]";
pub const ED25519_SECRET_KEY_BYTES: usize = 32;
pub const WALLET_SEED_BYTES: usize = 64;

/// Explicit in-memory boundary for wallet secret text.
///
/// The value is zeroized when dropped. This type intentionally does not
/// implement `Clone`, `Serialize`, `AsRef<str>` or `Deref<Target = str>`;
/// callers must opt in to secret access through [`SecretString::expose_secret`].
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// Intentionally expose the underlying secret to code that must use it.
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SecretString")
            .field(&REDACTED_SECRET)
            .finish()
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_SECRET)
    }
}

/// Zeroizing in-memory BIP-39 seed bytes used by deterministic wallet logic.
///
/// The original mnemonic is never retained here. This type intentionally does
/// not implement `Clone` or serialization and formatting is always redacted.
pub struct WalletSeed(Zeroizing<[u8; WALLET_SEED_BYTES]>);

impl WalletSeed {
    pub fn from_bytes(bytes: [u8; WALLET_SEED_BYTES]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn expose_secret(&self) -> &[u8; WALLET_SEED_BYTES] {
        &self.0
    }
}

impl fmt::Debug for WalletSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("WalletSeed")
            .field(&REDACTED_SECRET)
            .finish()
    }
}

impl fmt::Display for WalletSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_SECRET)
    }
}

/// Zeroizing in-memory Ed25519 secret-key bytes used by the local wallet.
///
/// It deliberately cannot be cloned or serialized and never exposes bytes via
/// formatting. Explicit access is limited to code performing local signing or
/// keystore encryption/decryption.
pub struct WalletSecretKey(Zeroizing<[u8; ED25519_SECRET_KEY_BYTES]>);

impl WalletSecretKey {
    pub fn from_bytes(bytes: [u8; ED25519_SECRET_KEY_BYTES]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn expose_secret(&self) -> &[u8; ED25519_SECRET_KEY_BYTES] {
        &self.0
    }
}

impl fmt::Debug for WalletSecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("WalletSecretKey")
            .field(&REDACTED_SECRET)
            .finish()
    }
}

impl fmt::Display for WalletSecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_SECRET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_never_expose_secret_contents() {
        let secret_value = "wallet-private-material-should-not-leak";
        let secret = SecretString::new(secret_value);

        let debug = format!("{secret:?}");
        let display = format!("{secret}");

        assert!(debug.contains(REDACTED_SECRET));
        assert!(display.contains(REDACTED_SECRET));
        assert!(!debug.contains(secret_value));
        assert!(!display.contains(secret_value));
    }

    #[test]
    fn secret_access_is_explicit() {
        let secret = SecretString::new("expected-secret");
        assert_eq!(secret.expose_secret(), "expected-secret");
        assert!(!secret.is_empty());
    }

    #[test]
    fn wallet_seed_formatting_is_always_redacted() {
        let seed = WalletSeed::from_bytes([0xcdu8; WALLET_SEED_BYTES]);
        let leaked_hex = "cd".repeat(WALLET_SEED_BYTES);
        let debug = format!("{seed:?}");
        let display = format!("{seed}");
        assert!(debug.contains(REDACTED_SECRET));
        assert!(display.contains(REDACTED_SECRET));
        assert!(!debug.contains(&leaked_hex));
        assert!(!display.contains(&leaked_hex));
        assert_eq!(seed.expose_secret(), &[0xcdu8; WALLET_SEED_BYTES]);
    }

    #[test]
    fn private_key_formatting_is_always_redacted() {
        let key = WalletSecretKey::from_bytes([0xabu8; ED25519_SECRET_KEY_BYTES]);
        let leaked_hex = "ab".repeat(ED25519_SECRET_KEY_BYTES);

        let debug = format!("{key:?}");
        let display = format!("{key}");
        assert!(debug.contains(REDACTED_SECRET));
        assert!(display.contains(REDACTED_SECRET));
        assert!(!debug.contains(&leaked_hex));
        assert!(!display.contains(&leaked_hex));
        assert_eq!(key.expose_secret(), &[0xabu8; ED25519_SECRET_KEY_BYTES]);
    }
}
