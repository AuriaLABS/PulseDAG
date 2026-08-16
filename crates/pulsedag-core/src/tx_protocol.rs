use crate::{
    errors::PulseError,
    protocol::{
        ProtocolActivationIdentity, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V1,
        BLOCK_HEADER_VERSION_V2,
    },
    state::ChainState,
    tx::{TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2},
    types::Transaction,
    validation::validate_transaction,
    validation_v2::validate_transaction_v2,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionValidationPath {
    LegacyV1,
    ActivatedV2,
}

fn invalid_identity(message: impl Into<String>) -> PulseError {
    PulseError::InvalidTransaction(format!(
        "invalid protocol activation identity: {}",
        message.into()
    ))
}

pub fn resolve_transaction_validation_path(
    identity: &ProtocolActivationIdentity,
    state: &ChainState,
) -> Result<TransactionValidationPath, PulseError> {
    identity.validate().map_err(invalid_identity)?;

    if identity.chain_id != state.chain_id {
        return Err(PulseError::ChainIdMismatch);
    }
    if identity.genesis_hash != state.dag.genesis_hash {
        return Err(invalid_identity("genesis hash does not match chain state"));
    }

    match (
        identity.transaction_protocol_version,
        identity.block_header_protocol_version,
        identity.consensus_mode,
    ) {
        (
            TRANSACTION_VERSION_V1,
            BLOCK_HEADER_VERSION_V1,
            ProtocolConsensusMode::Legacy | ProtocolConsensusMode::GhostdagDev,
        ) => Ok(TransactionValidationPath::LegacyV1),
        (TRANSACTION_VERSION_V2, BLOCK_HEADER_VERSION_V2, ProtocolConsensusMode::GhostdagV1) => {
            Ok(TransactionValidationPath::ActivatedV2)
        }
        _ => Err(invalid_identity(
            "mixed or unsupported transaction/header/consensus version tuple",
        )),
    }
}

pub fn validate_transaction_for_protocol(
    tx: &Transaction,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    let path = resolve_transaction_validation_path(identity, state)?;

    if tx.version != identity.transaction_protocol_version {
        return Err(PulseError::InvalidTransaction(format!(
            "protocol identity requires transaction version {}, got {}",
            identity.transaction_protocol_version, tx.version
        )));
    }

    match path {
        TransactionValidationPath::LegacyV1 => validate_transaction(tx, state),
        TransactionValidationPath::ActivatedV2 => {
            validate_transaction_v2(tx, state, &identity.chain_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state, ordering_v2::GHOSTDAG_V1_ORDERING_VERSION, types::Transaction,
    };

    fn empty_transaction(version: u32) -> Transaction {
        Transaction {
            txid: String::new(),
            version,
            inputs: Vec::new(),
            outputs: Vec::new(),
            fee: 0,
            nonce: 0,
        }
    }

    #[test]
    fn legacy_identity_selects_only_v1_validation() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let identity = ProtocolActivationIdentity::legacy_from_state(&state);

        assert_eq!(
            resolve_transaction_validation_path(&identity, &state).unwrap(),
            TransactionValidationPath::LegacyV1
        );
    }

    #[test]
    fn activated_identity_selects_only_v2_validation() {
        let state = init_chain_state("pulsedag-testnet-v2".to_string());
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert_eq!(
            resolve_transaction_validation_path(&identity, &state).unwrap(),
            TransactionValidationPath::ActivatedV2
        );
    }

    #[test]
    fn wrong_chain_and_genesis_fail_before_transaction_validation() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.chain_id = "pulsedag-private".to_string();
        assert!(matches!(
            resolve_transaction_validation_path(&identity, &state),
            Err(PulseError::ChainIdMismatch)
        ));

        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.genesis_hash = "wrong-genesis".to_string();
        assert!(matches!(
            resolve_transaction_validation_path(&identity, &state),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("genesis hash")
        ));
    }

    #[test]
    fn mixed_protocol_tuple_fails_closed() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.transaction_protocol_version = TRANSACTION_VERSION_V2;

        assert!(matches!(
            resolve_transaction_validation_path(&identity, &state),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("mixed or unsupported")
        ));
    }

    #[test]
    fn transaction_version_mismatch_fails_before_utxo_checks() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let identity = ProtocolActivationIdentity::legacy_from_state(&state);
        let v2 = empty_transaction(TRANSACTION_VERSION_V2);

        assert!(matches!(
            validate_transaction_for_protocol(&v2, &state, &identity),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("requires transaction version 1")
        ));
    }

    #[test]
    fn activated_identity_rejects_v1_before_utxo_checks() {
        let state = init_chain_state("pulsedag-testnet-v2".to_string());
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let v1 = empty_transaction(TRANSACTION_VERSION_V1);

        assert!(matches!(
            validate_transaction_for_protocol(&v1, &state, &identity),
            Err(PulseError::InvalidTransaction(message))
                if message.contains("requires transaction version 2")
        ));
    }
}
