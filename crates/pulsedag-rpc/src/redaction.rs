pub const REDACTED_VALUE: &str = "<redacted>";

pub fn redact_if_sensitive_key_value(key: &str, value: &str) -> String {
    if is_sensitive_key(key) || looks_sensitive_value(value) {
        REDACTED_VALUE.to_string()
    } else {
        value.to_string()
    }
}

pub fn redact_path(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        REDACTED_VALUE.to_string()
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    [
        "token", "secret", "private", "seed", "mnemonic", "password", "key", "auth",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn looks_sensitive_value(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("-----begin")
        || normalized.contains("seed phrase")
        || normalized.contains("mnemonic")
        || normalized.contains("private key")
        || normalized.contains("auth token")
        || normalized.contains("bearer ")
        || normalized.split_whitespace().count() >= 12
}

#[cfg(test)]
mod tests {
    use super::{redact_if_sensitive_key_value, redact_path, REDACTED_VALUE};

    #[test]
    fn redacts_auth_token_fields() {
        assert_eq!(
            redact_if_sensitive_key_value("operator_auth_token", "super-secret-token"),
            REDACTED_VALUE
        );
    }

    #[test]
    fn redacts_private_key_like_fields() {
        assert_eq!(
            redact_if_sensitive_key_value("wallet_private_key", "ab".repeat(32).as_str()),
            REDACTED_VALUE
        );
    }

    #[test]
    fn keeps_safe_fields_visible() {
        assert_eq!(
            redact_if_sensitive_key_value("chain_id", "pulsedag-private"),
            "pulsedag-private"
        );
    }

    #[test]
    fn redacts_paths_when_present() {
        assert_eq!(redact_path("/var/lib/pulsedag"), REDACTED_VALUE);
        assert_eq!(redact_path(""), "");
    }
}
