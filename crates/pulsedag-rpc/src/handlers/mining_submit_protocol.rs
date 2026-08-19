use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_activated_v2_mined_block_atomically, accept_block_atomically, evaluate_pow_for_protocol,
    pow_validation_result, preferred_tip_hash, resolve_pow_validation_path, AcceptSource,
    AtomicBlockAcceptance, Block, BlockAcceptanceResult, ChainState, PowValidationPath,
    ProtocolActivationIdentity, PulseError, BLOCK_HEADER_VERSION_V1,
};
use tokio::time::timeout;

pub use super::mining_submit_legacy::MiningSubmitData;

const MAX_TEMPLATE_PROTOCOL_BINDINGS: usize = 4_096;
const SUBMIT_V2_CHAIN_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoundTemplateProtocol {
    identity: ProtocolActivationIdentity,
    fingerprint: String,
}

fn template_protocol_bindings() -> &'static Mutex<BTreeMap<String, BoundTemplateProtocol>> {
    static BINDINGS: OnceLock<Mutex<BTreeMap<String, BoundTemplateProtocol>>> = OnceLock::new();
    BINDINGS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

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

fn invalid_protocol(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "mining submit protocol identity: {}",
        message.into()
    ))
}

pub(crate) fn bind_template_protocol(
    template_id: String,
    identity: ProtocolActivationIdentity,
    fingerprint: String,
) -> Result<(), PulseError> {
    let expected = identity
        .fingerprint()
        .map_err(|error| invalid_protocol(format!("template identity is invalid: {error}")))?;
    if fingerprint != expected {
        return Err(invalid_protocol(format!(
            "template fingerprint mismatch: expected {expected}, got {fingerprint}"
        )));
    }

    let mut bindings = template_protocol_bindings()
        .lock()
        .map_err(|_| PulseError::Internal("mining template protocol binding lock poisoned".into()))?;
    if !bindings.contains_key(&template_id) && bindings.len() >= MAX_TEMPLATE_PROTOCOL_BINDINGS {
        if let Some(oldest_key) = bindings.keys().next().cloned() {
            bindings.remove(&oldest_key);
        }
    }
    bindings.insert(
        template_id,
        BoundTemplateProtocol {
            identity,
            fingerprint,
        },
    );
    Ok(())
}

fn load_template_protocol(template_id: &str) -> Result<Option<BoundTemplateProtocol>, PulseError> {
    template_protocol_bindings()
        .lock()
        .map(|bindings| bindings.get(template_id).cloned())
        .map_err(|_| PulseError::Internal("mining template protocol binding lock poisoned".into()))
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
                return Err(invalid_protocol(format!(
                    "identity requires block header version {}, got {}",
                    identity.block_header_protocol_version, block.header.version
                )));
            }
            Ok(path)
        }
        None => {
            if block.header.version != BLOCK_HEADER_VERSION_V1 {
                return Err(invalid_protocol(format!(
                    "explicit identity is required for block header version {}",
                    block.header.version
                )));
            }
            Ok(PowValidationPath::LegacyV1)
        }
    }
}

pub(crate) fn validate_stored_template_protocol_identity(
    block: &Block,
    state: &ChainState,
    stored_identity: Option<&ProtocolActivationIdentity>,
    stored_fingerprint: Option<&str>,
    local_identity: Option<&ProtocolActivationIdentity>,
) -> Result<PowValidationPath, PulseError> {
    match (stored_identity, stored_fingerprint) {
        (None, None) => {
            let path = mining_submit_protocol_path(block, state, local_identity)?;
            if path != PowValidationPath::LegacyV1 {
                return Err(invalid_protocol(
                    "historical template without identity cannot authorize activated-v2 submit",
                ));
            }
            Ok(path)
        }
        (Some(_), None) => Err(invalid_protocol(
            "stored template identity is missing protocol_identity_fingerprint",
        )),
        (None, Some(_)) => Err(invalid_protocol(
            "stored template protocol_identity_fingerprint is present without identity",
        )),
        (Some(stored_identity), Some(stored_fingerprint)) => {
            let expected_fingerprint = stored_identity.fingerprint().map_err(|error| {
                invalid_protocol(format!("stored identity is invalid: {error}"))
            })?;
            if stored_fingerprint != expected_fingerprint {
                return Err(invalid_protocol(format!(
                    "stored template fingerprint mismatch: expected {expected_fingerprint}, got {stored_fingerprint}"
                )));
            }

            let stored_path = mining_submit_protocol_path(block, state, Some(stored_identity))?;
            match local_identity {
                Some(local_identity) if local_identity != stored_identity => {
                    return Err(invalid_protocol(
                        "stored template identity does not match current local protocol identity",
                    ));
                }
                None if stored_path == PowValidationPath::ActivatedV2 => {
                    return Err(invalid_protocol(
                        "activated-v2 stored template requires explicit current local protocol identity",
                    ));
                }
                None => {
                    let expected_legacy = ProtocolActivationIdentity::legacy_from_state(state);
                    if stored_identity != &expected_legacy {
                        return Err(invalid_protocol(
                            "stored legacy template identity does not match current chain identity",
                        ));
                    }
                }
                Some(_) => {}
            }

            let local_path = mining_submit_protocol_path(block, state, local_identity)?;
            if local_path != stored_path {
                return Err(invalid_protocol(
                    "stored template and local protocol identities select different submit paths",
                ));
            }
            Ok(stored_path)
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
        PowValidationPath::LegacyV1 => {
            accept_block_atomically(block, state, AcceptSource::Rpc, persist, |_block| Ok(()))
        }
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

fn response_data(
    req: &SubmitMinedBlockRequest,
    accepted: bool,
    reason_code: &str,
    reason: String,
    pow: Option<&MiningSubmitPowEvaluation>,
    selected_tip: Option<String>,
) -> MiningSubmitData {
    let duplicate = reason_code == "duplicate_block";
    MiningSubmitData {
        accepted,
        reason: reason.clone(),
        block_hash: Some(req.block.hash.clone()),
        block_id: accepted.then(|| req.block.hash.clone()),
        height: Some(req.block.header.height),
        pow_algorithm: pow
            .map(|pow| pow.algorithm.clone())
            .unwrap_or_else(|| pulsedag_core::selected_pow_name().to_string()),
        pow_accepted: pow.is_some_and(|pow| pow.accepted),
        pow_accepted_dev: pow.is_some_and(|pow| pow.accepted),
        protocol_version: super::mining_template::MINING_PROTOCOL_VERSION,
        target_u64: pow.map(|pow| pow.target_u64).unwrap_or(0),
        target_hex: pow
            .map(|pow| pow.target_hex.clone())
            .unwrap_or_else(|| format!("{:064x}", 0_u64)),
        pow_hash: pow.and_then(|pow| pow.hash_hex.clone()),
        template_id: req.template_id.clone(),
        invalid_pow: reason_code == "invalid_pow",
        stale: false,
        duplicate,
        stale_template: false,
        reason_code: reason_code.to_string(),
        selected_tip,
        adopted_orphans: 0,
        pow_hash_score_u64: pow.map(|pow| pow.score_u64).unwrap_or(0),
        pow_rejection_code: pow.and_then(|pow| pow.rejection_code.clone()),
        pow_rejection_reason: (!accepted).then_some(reason),
    }
}

fn rejected_response(
    req: &SubmitMinedBlockRequest,
    reason_code: &str,
    reason: impl Into<String>,
    pow: Option<&MiningSubmitPowEvaluation>,
) -> Json<ApiResponse<MiningSubmitData>> {
    let reason = reason.into();
    Json(ApiResponse::ok(response_data(
        req,
        false,
        reason_code,
        reason,
        pow,
        None,
    )))
}

fn acceptance_reason(result: &BlockAcceptanceResult) -> (&'static str, String) {
    match result {
        BlockAcceptanceResult::Accepted => ("accepted", "accepted".to_string()),
        BlockAcceptanceResult::Duplicate => ("duplicate_block", "duplicate block".to_string()),
        BlockAcceptanceResult::InvalidPow => ("invalid_pow", "invalid proof of work".to_string()),
        BlockAcceptanceResult::MissingParent => {
            ("missing_parent", "submitted block has a missing parent".to_string())
        }
        BlockAcceptanceResult::InvalidTransaction => (
            "invalid_transaction",
            "submitted block contains an invalid transaction".to_string(),
        ),
        BlockAcceptanceResult::Malformed => ("malformed", "malformed block".to_string()),
        BlockAcceptanceResult::Rejected(reason) => ("rejected", reason.clone()),
    }
}

async fn post_activated_v2_mining_submit<S: RpcStateLike>(
    state: S,
    req: SubmitMinedBlockRequest,
) -> Json<ApiResponse<MiningSubmitData>> {
    let Some(template_id) = req.template_id.as_deref() else {
        return rejected_response(
            &req,
            "missing_template_id",
            "activated-v2 mining submit requires template_id",
            None,
        );
    };

    let binding = match load_template_protocol(template_id) {
        Ok(Some(binding)) => binding,
        Ok(None) => {
            return rejected_response(
                &req,
                "unknown_template_protocol",
                "template has no protocol identity binding; refresh template before submit",
                None,
            );
        }
        Err(error) => {
            return rejected_response(&req, "protocol_identity_unavailable", error.to_string(), None);
        }
    };

    let local_identity = match rpc_protocol_identity(&state) {
        Ok(Some(identity)) => identity,
        Ok(None) => {
            return rejected_response(
                &req,
                "protocol_identity_unavailable",
                "activated-v2 mining submit requires explicit local protocol identity",
                None,
            );
        }
        Err(error) => {
            return rejected_response(&req, "protocol_identity_unavailable", error.to_string(), None);
        }
    };

    let chain_handle = state.chain();
    let mut chain = match timeout(SUBMIT_V2_CHAIN_WRITE_TIMEOUT, chain_handle.write()).await {
        Ok(chain) => chain,
        Err(_) => {
            return rejected_response(
                &req,
                "submit_busy",
                "activated-v2 mining submit could not acquire the chain write lock within the bounded timeout",
                None,
            );
        }
    };

    let path = match validate_stored_template_protocol_identity(
        &req.block,
        &chain,
        Some(&binding.identity),
        Some(&binding.fingerprint),
        Some(&local_identity),
    ) {
        Ok(path) => path,
        Err(error) => {
            return rejected_response(&req, "protocol_mismatch", error.to_string(), None);
        }
    };
    if path != PowValidationPath::ActivatedV2 {
        return rejected_response(
            &req,
            "protocol_mismatch",
            "non-v1 submit did not resolve to activated-v2 protocol path",
            None,
        );
    }

    let pow = match evaluate_mining_submit_pow(&req.block, &chain, Some(&local_identity)) {
        Ok(pow) => pow,
        Err(error) => {
            return rejected_response(&req, "protocol_mismatch", error.to_string(), None);
        }
    };
    if !pow.accepted {
        return rejected_response(
            &req,
            "invalid_pow",
            format!(
                "submitted block does not satisfy {} policy: reason_code={} score={} target={} difficulty={} height={} nonce={}",
                pow.algorithm,
                pow.rejection_code.as_deref().unwrap_or("score_above_target"),
                pow.score_u64,
                pow.target_u64,
                req.block.header.difficulty,
                req.block.header.height,
                req.block.header.nonce
            ),
            Some(&pow),
        );
    }

    let acceptance = match accept_mined_block_for_protocol(
        req.block.clone(),
        &mut chain,
        Some(&local_identity),
        |block, chain| state.storage().persist_block_and_chain_state(block, chain),
    ) {
        Ok(acceptance) => acceptance,
        Err(error) => {
            let reason_code = if matches!(&error, PulseError::StorageError(_)) {
                "storage_rejected"
            } else {
                "validation_rejected"
            };
            return rejected_response(&req, reason_code, error.to_string(), Some(&pow));
        }
    };

    if acceptance.result.is_accepted() {
        let selected_tip = preferred_tip_hash(&chain);
        drop(chain);
        if let Some(p2p) = state.p2p() {
            let _ = p2p.broadcast_block(&req.block);
        }
        return Json(ApiResponse::ok(response_data(
            &req,
            true,
            "accepted",
            "accepted".to_string(),
            Some(&pow),
            selected_tip,
        )));
    }

    let (reason_code, reason) = acceptance_reason(&acceptance.result);
    rejected_response(&req, reason_code, reason, Some(&pow))
}

pub async fn post_mining_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitMinedBlockRequest>,
) -> Json<ApiResponse<MiningSubmitData>> {
    if req.block.header.version == BLOCK_HEADER_VERSION_V1 {
        return super::mining_submit_legacy::post_mining_submit(State(state), Json(req)).await;
    }
    post_activated_v2_mining_submit(state, req).await
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
    fn template_protocol_binding_is_canonical_and_bounded() {
        let state = init_chain_state("task28-rpc-mining-binding".to_string());
        let identity = ProtocolActivationIdentity::legacy_from_state(&state);
        let fingerprint = identity.fingerprint().unwrap();
        let template_id = "task28-binding-template".to_string();

        bind_template_protocol(template_id.clone(), identity.clone(), fingerprint.clone()).unwrap();
        let stored = load_template_protocol(&template_id).unwrap().unwrap();
        assert_eq!(stored.identity, identity);
        assert_eq!(stored.fingerprint, fingerprint);
        assert!(bind_template_protocol(template_id, stored.identity, "00".to_string()).is_err());
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
        assert!(error.to_string().contains("explicit identity is required"));
    }

    #[test]
    fn historical_template_without_identity_remains_legacy_only() {
        let state = init_chain_state("task28-rpc-mining-submit".to_string());
        let block = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();

        assert_eq!(
            validate_stored_template_protocol_identity(&block, &state, None, None, None).unwrap(),
            PowValidationPath::LegacyV1
        );
    }

    #[test]
    fn stored_identity_requires_matching_fingerprint_and_local_identity() {
        let state = init_chain_state("task28-rpc-mining-submit-v2".to_string());
        let identity = activated_identity(&state);
        let fingerprint = identity.fingerprint().unwrap();
        let mut block = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .unwrap();
        block.header.version = 2;

        assert!(validate_stored_template_protocol_identity(
            &block,
            &state,
            Some(&identity),
            Some("00"),
            Some(&identity),
        )
        .is_err());
        assert!(validate_stored_template_protocol_identity(
            &block,
            &state,
            Some(&identity),
            Some(&fingerprint),
            None,
        )
        .is_err());
        assert_eq!(
            validate_stored_template_protocol_identity(
                &block,
                &state,
                Some(&identity),
                Some(&fingerprint),
                Some(&identity),
            )
            .unwrap(),
            PowValidationPath::ActivatedV2
        );
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
