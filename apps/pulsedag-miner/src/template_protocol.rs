use anyhow::{anyhow, Result};
use pulsedag_core::types::BlockHeader;
use pulsedag_core::ProtocolActivationIdentity;
use pulsedag_miner::protocol_pow::resolve_mining_pow_path;

/// Validate the optional protocol identity carried by a mining template.
///
/// Historical v1 templates carry neither identity nor fingerprint and remain
/// accepted only for a v1 block header. New protocol-aware templates must carry
/// both fields, and the fingerprint must match the canonical identity exactly.
pub fn validated_template_protocol_identity(
    header: &BlockHeader,
    identity: Option<&ProtocolActivationIdentity>,
    fingerprint: Option<&str>,
) -> Result<Option<ProtocolActivationIdentity>> {
    match (identity, fingerprint) {
        (None, None) => {
            resolve_mining_pow_path(header, None)?;
            Ok(None)
        }
        (Some(_), None) => Err(anyhow!(
            "mining template protocol identity is missing protocol_identity_fingerprint"
        )),
        (None, Some(_)) => Err(anyhow!(
            "mining template protocol_identity_fingerprint is present without protocol_identity"
        )),
        (Some(identity), Some(fingerprint)) => {
            let expected = identity
                .fingerprint()
                .map_err(|error| anyhow!("invalid mining template protocol identity: {error}"))?;
            if fingerprint != expected {
                return Err(anyhow!(
                    "mining template protocol identity fingerprint mismatch: expected {expected}, got {fingerprint}"
                ));
            }

            resolve_mining_pow_path(header, Some(identity))?;
            Ok(Some(identity.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION,
    };

    fn header(version: u32) -> BlockHeader {
        BlockHeader {
            version,
            parents: vec!["11".repeat(32)],
            timestamp: 1_700_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: "22".repeat(32),
            state_root: "33".repeat(32),
            blue_score: 1,
            height: 2,
        }
    }

    fn activated_identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "44".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    #[test]
    fn historical_v1_template_without_identity_is_preserved() {
        let identity =
            validated_template_protocol_identity(&header(BLOCK_HEADER_VERSION_V1), None, None)
                .unwrap();
        assert!(identity.is_none());
    }

    #[test]
    fn v2_header_without_identity_fails_closed() {
        let error =
            validated_template_protocol_identity(&header(BLOCK_HEADER_VERSION_V2), None, None)
                .unwrap_err();
        assert!(error.to_string().contains("requires block header version"));
    }

    #[test]
    fn identity_and_fingerprint_must_arrive_together() {
        let identity = activated_identity();
        let fingerprint = identity.fingerprint().unwrap();

        assert!(validated_template_protocol_identity(
            &header(BLOCK_HEADER_VERSION_V2),
            Some(&identity),
            None,
        )
        .is_err());
        assert!(validated_template_protocol_identity(
            &header(BLOCK_HEADER_VERSION_V2),
            None,
            Some(&fingerprint),
        )
        .is_err());
    }

    #[test]
    fn activated_v2_identity_with_matching_fingerprint_is_accepted() {
        let identity = activated_identity();
        let fingerprint = identity.fingerprint().unwrap();
        let validated = validated_template_protocol_identity(
            &header(BLOCK_HEADER_VERSION_V2),
            Some(&identity),
            Some(&fingerprint),
        )
        .unwrap()
        .expect("v2 identity should be returned");

        assert_eq!(validated, identity);
    }

    #[test]
    fn fingerprint_mismatch_fails_closed() {
        let identity = activated_identity();
        let error = validated_template_protocol_identity(
            &header(BLOCK_HEADER_VERSION_V2),
            Some(&identity),
            Some("00"),
        )
        .unwrap_err();

        assert!(error.to_string().contains("fingerprint mismatch"));
    }
}
