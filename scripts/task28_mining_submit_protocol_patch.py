from pathlib import Path


def patch_file(path_str: str, edits: list[tuple[str, str]]) -> None:
    path = Path(path_str)
    text = path.read_text()
    for old, new in edits:
        count = text.count(old)
        if count != 1:
            raise SystemExit(
                f'{path_str}: expected exactly one match, found {count}: {old[:120]!r}'
            )
        text = text.replace(old, new, 1)
    path.write_text(text)


submit_edits = [
    (
'''use super::mining_template::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};
''',
'''use super::mining_submit_protocol::{
    accept_mined_block_for_protocol, evaluate_mining_submit_pow, rpc_protocol_identity,
    validate_stored_template_protocol_identity,
};
use super::mining_template::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};
'''),
    (
'''use pulsedag_core::{
    accept_block_atomically, adopt_ready_orphans, expected_difficulty, pow_validation_result,
    preferred_tip_hash, AcceptSource,
};
''',
'''use pulsedag_core::{
    adopt_ready_orphans, expected_difficulty, preferred_tip_hash, AcceptSource, PowValidationPath,
};
'''),
    (
'''    record_submit_phase(&state, "precheck").await;
    let pow = pow_validation_result(&req.block.header);
    let pow_accepted_dev = pow.accepted;
    let target_u64 = pow.target_u64;
    let pow_hash_score_u64 = pow.score_u64.unwrap_or(0);
    {
''',
'''    record_submit_phase(&state, "precheck").await;
    let protocol_identity = match rpc_protocol_identity(&state) {
        Ok(identity) => identity,
        Err(error) => {
            let detail = format!(
                "mining submit protocol identity unavailable before acceptance: {error}"
            );
            record_external_mining_rejection(
                &state,
                ExternalMiningRejectKind::UnknownValidationError,
                &detail,
            )
            .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(rejection_submit_response(
                "protocol_identity_unavailable",
                detail,
                Some(block_hash),
                Some(height),
                false,
            ));
        }
    };
    {
'''),
    (
'''    if chain.dag.blocks.contains_key(&block_hash) {
''',
'''    let pow = match evaluate_mining_submit_pow(&req.block, &chain, protocol_identity.as_ref()) {
        Ok(pow) => pow,
        Err(error) => {
            let detail = format!("mining submit protocol mismatch: {error}");
            drop(chain);
            record_external_mining_rejection(
                &state,
                ExternalMiningRejectKind::UnknownValidationError,
                &detail,
            )
            .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(rejection_submit_response(
                "protocol_mismatch",
                detail,
                Some(block_hash),
                Some(height),
                false,
            ));
        }
    };
    let pow_accepted_dev = pow.accepted;
    let target_u64 = pow.target_u64;
    let pow_hash_score_u64 = pow.score_u64;

    if chain.dag.blocks.contains_key(&block_hash) {
'''),
    (
'''    if let Some(stored) = load_template(template_id) {
        if req.block.header.height != stored.height {
''',
'''    if let Some(stored) = load_template(template_id) {
        if let Err(error) = validate_stored_template_protocol_identity(
            &req.block,
            &chain,
            stored.protocol_identity.as_ref(),
            stored.protocol_identity_fingerprint.as_deref(),
            protocol_identity.as_ref(),
        ) {
            let detail = format!("stored mining template protocol mismatch: {error}");
            drop(chain);
            record_external_mining_rejection(
                &state,
                ExternalMiningRejectKind::UnknownValidationError,
                &detail,
            )
            .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(rejection_submit_response(
                "protocol_mismatch",
                detail,
                Some(block_hash.clone()),
                Some(height),
                true,
            ));
        }
        if req.block.header.height != stored.height {
'''),
    (
'''    if req.block.header.difficulty != expected_difficulty {
''',
'''    if pow.path == PowValidationPath::LegacyV1
        && req.block.header.difficulty != expected_difficulty
    {
'''),
    (
'''    let acceptance = match accept_block_atomically(
        req.block.clone(),
        &mut chain,
        AcceptSource::Rpc,
        |block, chain| state.storage().persist_block_and_chain_state(block, chain),
        |_block| Ok(()),
    ) {
''',
'''    let acceptance = match accept_mined_block_for_protocol(
        req.block.clone(),
        &mut chain,
        protocol_identity.as_ref(),
        |block, chain| state.storage().persist_block_and_chain_state(block, chain),
    ) {
'''),
    (
'''        Err(e) => {
            drop(chain);
            record_external_mining_rejection(
                &state,
                ExternalMiningRejectKind::SubmitBlockError,
                &e.to_string(),
            )
            .await;
            record_submit_completed(&state, submit_started, "error").await;
            return Json(rejection_submit_response(
                "storage_rejected",
                e.to_string(),
                Some(block_hash.clone()),
                Some(height),
                false,
            ));
        }
''',
'''        Err(e) => {
            drop(chain);
            let detail = e.to_string();
            let (reason_code, rejection_kind) =
                if matches!(&e, pulsedag_core::PulseError::StorageError(_)) {
                    ("storage_rejected", ExternalMiningRejectKind::SubmitBlockError)
                } else {
                    let (reason_code, kind) = classify_rejected_validation_message(&detail);
                    (reason_code, kind)
                };
            record_external_mining_rejection(&state, rejection_kind, &detail).await;
            record_submit_completed(&state, submit_started, "error").await;
            return Json(rejection_submit_response(
                reason_code,
                detail,
                Some(block_hash.clone()),
                Some(height),
                false,
            ));
        }
'''),
]
patch_file('crates/pulsedag-rpc/src/handlers/mining_submit.rs', submit_edits)

submit_path = Path('crates/pulsedag-rpc/src/handlers/mining_submit.rs')
submit_text = submit_path.read_text()
submit_text = submit_text.replace(
    'pow.rejection_code.map(|v| v.to_string())',
    'pow.rejection_code.clone()',
)
submit_text = submit_text.replace(
    'pow.rejection_code.unwrap_or("score_above_target")',
    'pow.rejection_code.as_deref().unwrap_or("score_above_target")',
)
submit_text = submit_text.replace('pow.algorithm.to_string()', 'pow.algorithm.clone()')
submit_path.write_text(submit_text)


template_edits = [
    (
'''pub struct StoredMiningTemplate {
    #[serde(default = "default_mining_protocol_version")]
    pub protocol_version: u32,
    pub template_id: String,
''',
'''pub struct StoredMiningTemplate {
    #[serde(default = "default_mining_protocol_version")]
    pub protocol_version: u32,
    #[serde(default)]
    pub protocol_identity: Option<pulsedag_core::ProtocolActivationIdentity>,
    #[serde(default)]
    pub protocol_identity_fingerprint: Option<String>,
    pub template_id: String,
'''),
    (
'''    store_template(&StoredMiningTemplate {
        protocol_version: MINING_PROTOCOL_VERSION,
        template_id: template_id.clone(),
''',
'''    let stored_protocol_identity = pulsedag_core::ProtocolActivationIdentity::legacy_from_state(&chain);
    let stored_protocol_identity_fingerprint = match stored_protocol_identity.fingerprint() {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot bind mining template to protocol identity: {error}"),
            ));
        }
    };

    store_template(&StoredMiningTemplate {
        protocol_version: MINING_PROTOCOL_VERSION,
        protocol_identity: Some(stored_protocol_identity),
        protocol_identity_fingerprint: Some(stored_protocol_identity_fingerprint),
        template_id: template_id.clone(),
'''),
]
patch_file('crates/pulsedag-rpc/src/handlers/mining_template.rs', template_edits)
