use std::fmt;

/// Stable marker used whenever wallet secret material is formatted.
pub const REDACTED_SECRET: &str = "[REDACTED]";

/// Explicit in-memory boundary for wallet secret text.
///
/// This type intentionally does not implement `Clone`, `Serialize`, `AsRef<str>`
/// or `Deref<Target = str>`. Callers must opt in to secret access through
/// [`SecretString::expose_secret`], which makes secret-handling code easy to
/// identify during review.
///
/// This first hardening step protects formatting/logging boundaries only. A
/// follow-up in #819 will add reviewed zeroization support together with the
/// encrypted keystore dependencies; this type must not be described as a
/// secure-erasure primitive by itself.
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Intentionally expose the underlying secret to code that must use it.
    pub fn expose_secret(&self) -> &str {
        &self.0
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
        f.debug_tuple("SecretString").field(&REDACTED_SECRET).finish()
    }
}

impl fmt::Display for SecretString {
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
}
