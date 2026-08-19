from pathlib import Path

path = Path('crates/pulsedag-rpc/src/handlers/mining_submit.rs')
text = path.read_text()

def replace_once(old: str, new: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'expected exactly one match, found {count}: {old[:120]!r}')
    text = text.replace(old, new, 1)

replace_once(
'''use super::mining_template::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};
''',
'''use super::mining_submit_protocol::{
    accept_mined_block_for_protocol, evaluate_mining_submit_pow, rpc_protocol_identity,
};
use super::mining_template::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};
''')

replace_once(
'''use pulsedag_core::{
    accept_block_atomically, adopt_ready_orphans, expected_difficulty, pow_validation_result,
    preferred_tip_hash, AcceptSource,
};
''',
'''use pulsedag_core::{
    adopt_ready_orphans, expected_difficulty, preferred_tip_hash, AcceptSource, PowValidationPath,
};
''')

replace_once(
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
''')

replace_once(
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
''')

replace_once(
'''    if req.block.header.difficulty != expected_difficulty {
''',
'''    if pow.path == PowValidationPath::LegacyV1
        && req.block.header.difficulty != expected_difficulty
    {
''')

replace_once(
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
''')

replace_once(
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
''')

text = text.replace(
    'pow.rejection_code.map(|v| v.to_string())',
    'pow.rejection_code.clone()',
)
text = text.replace(
    'pow.rejection_code.unwrap_or("score_above_target")',
    'pow.rejection_code.as_deref().unwrap_or("score_above_target")',
)
text = text.replace('pow.algorithm.to_string()', 'pow.algorithm.clone()')

path.write_text(text)
