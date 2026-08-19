use crate::api::RpcStateLike;
use pulsedag_core::{
    accept_activated_v2_mined_block_atomically, accept_block_atomically,
    evaluate_pow_for_protocol, pow_validation_result, resolve_pow_validation_path, AcceptSource,
    AtomicBlockAcceptance, Block, ChainState, PowValidationPath, ProtocolActivationIdentity,
    PulseError, BLOCK_HEADER_VERSION_V1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MiningSubmitPowEvaluation {
    pub path: PowValidationPath,
    pub algorithm: String,
    pub accepted: bool,
    pub target_u64: u64,
    pub target_hex: String,
    pub hash_hex: Option<String>,
    pub score_u64: u64,
    pub rejection_code: Option<String>,
}

pub(crate) fn rpc_protocol_identity<S: RpcStateLike>(
    state: &S,
) -> Result<Option<ProtocolActivationIdentity>, PulseError> {
    let Some(p2p) = state.p2p() else {
        return Ok(None);
    };
    p2p.local_protocol_capabilities_v1()
        .map(|capabilities| capabilities.map(|capabilities| capabilities.protocol_identity))
}

pub(crate) fn mining_submit_protocol_path(
    block: &Block,
    state: &ChainState,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<PowValidationPath, PulseError> {
    match identity {
        Some(identity) => {
            let path = resolve_pow_validation_path(identity, state)?;
            if block.header.version != identity.block_header_protocol_version {
                return Err(PulseError::InvalidBlock(format!(
                    "mining submit protocol identity requires block header version {}, got {}",
                    identity.block_header_protocol_version, block.header.version
                )));
            }
            Ok(path)
        }
        None => {
            if block.header.version != BLOCK_HEADER_VERSION_V1 {
                return Err(PulseError::InvalidBlock(format!(
                    "mining submit requires explicit protocol identity for block header version {}",
                    block.header.version
                )));
            }
            Ok(PowValidationPath::LegacyV1)
        }
    }
}

pub(crate) fn evaluate_mining_submit_pow(
    block: &Block,
    state: &ChainState,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<MiningSubmitPowEvaluation, PulseError> {
    let path = mining_submit_protocol_path(block, state, identity)?;
    match path {
        PowValidationPath::LegacyV1 => {
            let pow = pow_validation_result(&block.header);
            Ok(MiningSubmitPowEvaluation {
                path,
                algorithm: pow.algorithm.to_string(),
                accepted: pow.accepted,
                target_u64: pow.target_u64,
                target_hex: pow.target_hex,
                hash_hex: pow.hash_hex,
                score_u64: pow.score_u64.unwrap_or(0),
                rejection_code: pow.rejection_code.map(str::to_string),
            })
        }
        PowValidationPath::ActivatedV2 => {
            let identity = identity.expect("activated-v2 path requires explicit identity");
            let attempt = evaluate_pow_for_protocol(&block.header, state, identity)?;
            let accepted = attempt.comparison.accepted();
            Ok(MiningSubmitPowEvaluation {
                path,
                algorithm: pulsedag_core::selected_pow_name().to_string(),
                accepted,
                target_u64: attempt.material.target.target_u64,
                target_hex: attempt.material.target.target_hex,
                hash_hex: Some(attempt.final_hash.hash_hex),
                score_u64: attempt.final_hash.score_u64,
                rejection_code: (!accepted).then(|| "score_above_target".to_string()),
            })
        }
    }
}

pub(crate) fn accept_mined_block_for_protocol<FPersist>(
    block: Block,
    state: &mut ChainState,
    identity: Option<&ProtocolActivationIdentity>,
    persist: FPersist,
) -> Result<AtomicBlockAcceptance, PulseError>
where
    FPersist: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
{
    match mining_submit_protocol_path(&block, state, identity)? {
        PowValidationPath::LegacyV1 => accept_block_atomically(
            block,
            state,
            AcceptSource::Rpc,
            persist,
            |_block| Ok(()),
        ),
        PowValidationPath::ActivatedV2 => accept_activated_v2_mined_block_atomically(
            block,
            state,
            AcceptSource::Rpc,
            identity.expect("activated-v2 path requires explicit identity"),
            persist,
            |_block| Ok(()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        build_activated_v2_mining_template, compute_block_hash_v2, current_ts,
        genesis::init_chain_state, validate_pow_for_protocol, ActivatedV2MiningTemplateSpec,
        ProtocolConsensusMode, GHOSTDAG_V1_ORDERING_VERSION,
    };

    fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    #[test]
    fn v2_submit_without_identity_fails_before_pow_evaluation() {
        let state = init_chain_state("task28-rpc-mining-submit".to_string());
        let mut block = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        block.header.version = 2;

        let error = evaluate_mining_submit_pow(&block, &state, None).unwrap_err();
        assert!(error.to_string().contains("requires explicit protocol identity"));
    }

    #[test]
    fn mixed_local_protocol_tuple_fails_closed() {
        let state = init_chain_state("task28-rpc-mining-submit".to_string());
        let block = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.consensus_mode = ProtocolConsensusMode::GhostdagV1;

        assert!(evaluate_mining_submit_pow(&block, &state, Some(&identity)).is_err());
    }

    #[test]
    fn activated_v2_selector_commits_through_mining_specific_boundary() {
        let mut state = init_chain_state("task28-rpc-mining-submit-v2".to_string());
        let identity = activated_identity(&state);
        let template = build_activated_v2_mining_template(
            &state,
            &identity,
            ActivatedV2MiningTemplateSpec {
                miner_address: "pulse1task28rpcminer".to_string(),
                timestamp: current_ts(),
                coinbase_nonce: 17,
                transactions: Vec::new(),
            },
        )
        .unwrap();
        let mut block = template.block;
        for nonce in 0..=200_000_u64 {
            block.header.nonce = nonce;
            block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
            if validate_pow_for_protocol(&block.header, &state, &identity).is_ok() {
                break;
            }
        }
        assert!(validate_pow_for_protocol(&block.header, &state, &identity).is_ok());

        let pow = evaluate_mining_submit_pow(&block, &state, Some(&identity)).unwrap();
        assert_eq!(pow.path, PowValidationPath::ActivatedV2);
        assert!(pow.accepted);

        let expected_hash = block.hash.clone();
        let mut persisted = false;
        let acceptance = accept_mined_block_for_protocol(
            block,
            &mut state,
            Some(&identity),
            |persisted_block, persisted_state| {
                persisted = true;
                assert_eq!(persisted_block.hash, expected_hash);
                assert!(persisted_state.dag.blocks.contains_key(&expected_hash));
                Ok(())
            },
        )
        .unwrap();

        assert!(acceptance.result.is_accepted());
        assert!(persisted);
        assert!(state.dag.blocks.contains_key(&expected_hash));
    }
}
