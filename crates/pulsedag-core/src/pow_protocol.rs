use crate::{
    errors::PulseError,
    pow::{canonical_pow_adapter, CanonicalPowAttempt, PowTargetComparison},
    pow_v2::canonical_pow_v2_adapter,
    protocol::{
        ProtocolActivationIdentity, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V1,
        BLOCK_HEADER_VERSION_V2,
    },
    state::ChainState,
    tx::{TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2},
    types::BlockHeader,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowValidationPath {
    LegacyV1,
    ActivatedV2,
}

impl PowValidationPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegacyV1 => "legacy_v1",
            Self::ActivatedV2 => "activated_v2",
        }
    }
}

fn invalid_identity(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "invalid protocol activation identity: {}",
        message.into()
    ))
}

fn resolve_validated_pow_identity_path(
    identity: &ProtocolActivationIdentity,
) -> Result<PowValidationPath, PulseError> {
    match (
        identity.transaction_protocol_version,
        identity.block_header_protocol_version,
        identity.consensus_mode,
    ) {
        (
            TRANSACTION_VERSION_V1,
            BLOCK_HEADER_VERSION_V1,
            ProtocolConsensusMode::Legacy | ProtocolConsensusMode::GhostdagDev,
        ) => Ok(PowValidationPath::LegacyV1),
        (TRANSACTION_VERSION_V2, BLOCK_HEADER_VERSION_V2, ProtocolConsensusMode::GhostdagV1) => {
            Ok(PowValidationPath::ActivatedV2)
        }
        _ => Err(invalid_identity(
            "mixed or unsupported transaction/header/consensus version tuple",
        )),
    }
}

/// Resolve only the protocol-version/consensus tuple carried by an activation
/// identity. This is intentionally independent of `ChainState` so external
/// protocol-aware components such as the standalone miner can select the exact
/// v1/v2 PoW domain without duplicating the consensus tuple match.
///
/// Callers that own authoritative chain state must continue to use
/// [`resolve_pow_validation_path`] so chain-id and genesis identity are checked
/// before PoW evaluation.
pub fn resolve_pow_identity_path(
    identity: &ProtocolActivationIdentity,
) -> Result<PowValidationPath, PulseError> {
    identity.validate().map_err(invalid_identity)?;
    resolve_validated_pow_identity_path(identity)
}

pub fn resolve_pow_validation_path(
    identity: &ProtocolActivationIdentity,
    state: &ChainState,
) -> Result<PowValidationPath, PulseError> {
    identity.validate().map_err(invalid_identity)?;

    if identity.chain_id != state.chain_id {
        return Err(PulseError::ChainIdMismatch);
    }
    if identity.genesis_hash != state.dag.genesis_hash {
        return Err(invalid_identity("genesis hash does not match chain state"));
    }

    resolve_validated_pow_identity_path(identity)
}

pub fn evaluate_pow_for_protocol(
    header: &BlockHeader,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<CanonicalPowAttempt, PulseError> {
    let path = resolve_pow_validation_path(identity, state)?;

    if header.version != identity.block_header_protocol_version {
        return Err(PulseError::InvalidBlock(format!(
            "protocol identity requires block header version {}, got {}",
            identity.block_header_protocol_version, header.version
        )));
    }

    match path {
        PowValidationPath::LegacyV1 => {
            canonical_pow_adapter()
                .evaluate_header(header)
                .map_err(|reason| {
                    PulseError::InvalidBlock(format!("invalid PoW preimage: {}", reason.code()))
                })
        }
        PowValidationPath::ActivatedV2 => {
            canonical_pow_v2_adapter().evaluate_header(header, &identity.chain_id)
        }
    }
}

pub fn validate_pow_for_protocol(
    header: &BlockHeader,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    let attempt = evaluate_pow_for_protocol(header, state, identity)?;
    if attempt.comparison == PowTargetComparison::MeetsTarget {
        Ok(())
    } else {
        Err(PulseError::InvalidBlock(
            "proof of work is above target".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{genesis::init_chain_state, ordering_v2::GHOSTDAG_V1_ORDERING_VERSION};

    fn sample_header(version: u32) -> BlockHeader {
        BlockHeader {
            version,
            parents: vec!["11".repeat(32)],
            timestamp: 1_700_000_000,
            difficulty: 1,
            nonce: 7,
            merkle_root: "33".repeat(32),
            state_root: "44".repeat(32),
            blue_score: 5,
            height: 6,
        }
    }

    #[test]
    fn validation_path_labels_are_stable() {
        assert_eq!(PowValidationPath::LegacyV1.as_str(), "legacy_v1");
        assert_eq!(PowValidationPath::ActivatedV2.as_str(), "activated_v2");
    }

    #[test]
    fn identity_only_path_matches_legacy_and_activated_tuples() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        assert_eq!(
            resolve_pow_identity_path(&legacy).unwrap(),
            PowValidationPath::LegacyV1
        );

        let activated = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        assert_eq!(
            resolve_pow_identity_path(&activated).unwrap(),
            PowValidationPath::ActivatedV2
        );
    }

    #[test]
    fn legacy_identity_selects_only_v1_pow() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let identity = ProtocolActivationIdentity::legacy_from_state(&state);
        assert_eq!(
            resolve_pow_validation_path(&identity, &state).unwrap(),
            PowValidationPath::LegacyV1
        );
        assert!(evaluate_pow_for_protocol(
            &sample_header(BLOCK_HEADER_VERSION_V1),
            &state,
            &identity,
        )
        .is_ok());
    }

    #[test]
    fn activated_identity_selects_chain_bound_v2_pow() {
        let state = init_chain_state("pulsedag-testnet-v2".to_string());
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        assert_eq!(
            resolve_pow_validation_path(&identity, &state).unwrap(),
            PowValidationPath::ActivatedV2
        );
        assert!(evaluate_pow_for_protocol(
            &sample_header(BLOCK_HEADER_VERSION_V2),
            &state,
            &identity,
        )
        .is_ok());
    }

    #[test]
    fn wrong_chain_and_genesis_fail_before_pow_evaluation() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.chain_id = "pulsedag-private".to_string();
        assert!(matches!(
            resolve_pow_validation_path(&identity, &state),
            Err(PulseError::ChainIdMismatch)
        ));

        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.genesis_hash = "wrong-genesis".to_string();
        assert!(matches!(
            resolve_pow_validation_path(&identity, &state),
            Err(PulseError::InvalidBlock(message)) if message.contains("genesis hash")
        ));
    }

    #[test]
    fn mixed_protocol_tuple_fails_closed() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.block_header_protocol_version = BLOCK_HEADER_VERSION_V2;
        assert!(matches!(
            resolve_pow_identity_path(&identity),
            Err(PulseError::InvalidBlock(message)) if message.contains("mixed or unsupported")
        ));
        assert!(matches!(
            resolve_pow_validation_path(&identity, &state),
            Err(PulseError::InvalidBlock(message)) if message.contains("mixed or unsupported")
        ));
    }

    #[test]
    fn header_version_mismatch_fails_before_pow_evaluation() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let identity = ProtocolActivationIdentity::legacy_from_state(&state);
        assert!(matches!(
            evaluate_pow_for_protocol(
                &sample_header(BLOCK_HEADER_VERSION_V2),
                &state,
                &identity,
            ),
            Err(PulseError::InvalidBlock(message))
                if message.contains("requires block header version 1")
        ));
    }
}
