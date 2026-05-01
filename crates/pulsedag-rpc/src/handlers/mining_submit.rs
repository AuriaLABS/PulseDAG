use super::mining_template::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
};
use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_block_with_result, adopt_ready_orphans, pow_validation_result, preferred_tip_hash,
    AcceptSource,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

#[derive(Debug, serde::Serialize)]
pub struct MiningSubmitData {
    pub accepted: bool,
    pub block_hash: Option<String>,
    pub block_id: Option<String>,
    pub height: Option<u64>,
    pub pow_algorithm: String,
    pub pow_accepted: bool,
    pub pow_accepted_dev: bool,
    pub target_u64: u64,
    pub stale_template: bool,
    pub reason_code: String,
    pub selected_tip: Option<String>,
    pub adopted_orphans: usize,
    pub pow_hash_score_u64: u64,
    pub pow_rejection_code: Option<String>,
    pub pow_rejection_reason: Option<String>,
}

fn persist_then_broadcast_mined_block<FPersist, FBroadcast>(
    block: &pulsedag_core::types::Block,
    chain: &pulsedag_core::state::ChainState,
    persist: FPersist,
    broadcast: FBroadcast,
) -> Result<(), pulsedag_core::errors::PulseError>
where
    FPersist: FnOnce(
        &pulsedag_core::types::Block,
        &pulsedag_core::state::ChainState,
    ) -> Result<(), pulsedag_core::errors::PulseError>,
    FBroadcast: FnOnce(&pulsedag_core::types::Block),
{
    persist(block, chain)?;
    broadcast(block);
    Ok(())
}

#[derive(Clone, Copy)]
enum ExternalMiningRejectKind {
    InvalidPow,
    StaleTemplate,
    UnknownTemplate,
    SubmitBlockError,
    DuplicateBlock,
    InvalidBlock,
    ChainIdMismatch,
    InternalError,
    StorageError,
}

#[derive(Clone, Copy)]
enum StaleTemplateReason {
    ChainHeightAdvanced,
    TemplateHeightStale,
    TemplateParentsMismatch,
    TemplateSelectedTipMismatch,
    TemplateDifficultyTargetMismatch,
    TemplateMempoolChanged,
    TemplateExpired,
    TemplateFutureClockSkew,
    TemplateLifecycleChanged,
    SubmittedDifficultyMismatch,
    SubmittedTransactionsMismatch,
    SubmittedParentsMismatch,
}

impl StaleTemplateReason {
    fn code(self) -> &'static str {
        match self {
            Self::ChainHeightAdvanced => "chain_height_advanced",
            Self::TemplateHeightStale => "template_height_stale",
            Self::TemplateParentsMismatch => "template_parents_mismatch",
            Self::TemplateSelectedTipMismatch => "template_selected_tip_mismatch",
            Self::TemplateDifficultyTargetMismatch => "template_difficulty_target_mismatch",
            Self::TemplateMempoolChanged => "template_mempool_changed",
            Self::TemplateExpired => "template_expired",
            Self::TemplateFutureClockSkew => "template_future_clock_skew",
            Self::TemplateLifecycleChanged => "template_lifecycle_changed",
            Self::SubmittedDifficultyMismatch => "submitted_difficulty_mismatch",
            Self::SubmittedTransactionsMismatch => "submitted_transactions_mismatch",
            Self::SubmittedParentsMismatch => "submitted_parents_mismatch",
        }
    }
}

fn stale_template_error(
    reason: StaleTemplateReason,
    detail: impl Into<String>,
) -> ApiResponse<MiningSubmitData> {
    rejection_submit_response(
        "stale_template",
        format!(
            "reason_code={}; template is stale; {}",
            reason.code(),
            detail.into()
        ),
        None,
        None,
        true,
    )
}

fn rejection_submit_response(
    reason_code: impl Into<String>,
    detail: impl Into<String>,
    block_hash: Option<String>,
    height: Option<u64>,
    stale_template: bool,
) -> ApiResponse<MiningSubmitData> {
    ApiResponse::ok(MiningSubmitData {
        accepted: false,
        block_hash,
        block_id: None,
        height,
        pow_algorithm: pulsedag_core::selected_pow_name().to_string(),
        pow_accepted: false,
        pow_accepted_dev: false,
        target_u64: 0,
        stale_template,
        reason_code: reason_code.into(),
        selected_tip: None,
        adopted_orphans: 0,
        pow_hash_score_u64: 0,
        pow_rejection_code: None,
        pow_rejection_reason: Some(detail.into()),
    })
}

async fn record_external_mining_rejection<S: RpcStateLike>(
    state: &S,
    kind: ExternalMiningRejectKind,
    message: &str,
) {
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_submit_rejected =
        runtime.external_mining_submit_rejected.saturating_add(1);
    runtime.external_mining_last_rejection_kind = Some(
        match kind {
            ExternalMiningRejectKind::InvalidPow => "invalid_pow",
            ExternalMiningRejectKind::StaleTemplate => "stale_template",
            ExternalMiningRejectKind::UnknownTemplate => "unknown_template",
            ExternalMiningRejectKind::SubmitBlockError => "submit_block_error",
            ExternalMiningRejectKind::DuplicateBlock => "duplicate_block",
            ExternalMiningRejectKind::InvalidBlock => "invalid_block",
            ExternalMiningRejectKind::ChainIdMismatch => "chain_id_mismatch",
            ExternalMiningRejectKind::InternalError => "internal_error",
            ExternalMiningRejectKind::StorageError => "storage_error",
        }
        .to_string(),
    );
    runtime.external_mining_last_rejection_reason = Some(message.to_string());
    match kind {
        ExternalMiningRejectKind::InvalidPow => {
            runtime.external_mining_rejected_invalid_pow = runtime
                .external_mining_rejected_invalid_pow
                .saturating_add(1);
            runtime.external_mining_last_invalid_pow_reason = Some(message.to_string());
        }
        ExternalMiningRejectKind::StaleTemplate => {
            runtime.external_mining_rejected_stale_template = runtime
                .external_mining_rejected_stale_template
                .saturating_add(1);
            runtime.external_mining_stale_work_detected = runtime
                .external_mining_stale_work_detected
                .saturating_add(1);
        }
        ExternalMiningRejectKind::UnknownTemplate => {
            runtime.external_mining_rejected_unknown_template = runtime
                .external_mining_rejected_unknown_template
                .saturating_add(1);
        }
        ExternalMiningRejectKind::SubmitBlockError => {
            runtime.external_mining_rejected_submit_block_error = runtime
                .external_mining_rejected_submit_block_error
                .saturating_add(1);
        }
        ExternalMiningRejectKind::DuplicateBlock => {
            runtime.external_mining_rejected_duplicate_block = runtime
                .external_mining_rejected_duplicate_block
                .saturating_add(1);
        }
        ExternalMiningRejectKind::InvalidBlock => {
            runtime.external_mining_rejected_invalid_block = runtime
                .external_mining_rejected_invalid_block
                .saturating_add(1);
        }
        ExternalMiningRejectKind::ChainIdMismatch => {
            runtime.external_mining_rejected_chain_id_mismatch = runtime
                .external_mining_rejected_chain_id_mismatch
                .saturating_add(1);
        }
        ExternalMiningRejectKind::InternalError => {
            runtime.external_mining_rejected_internal_error = runtime
                .external_mining_rejected_internal_error
                .saturating_add(1);
        }
        ExternalMiningRejectKind::StorageError => {
            runtime.external_mining_rejected_storage_error = runtime
                .external_mining_rejected_storage_error
                .saturating_add(1);
        }
    }
    drop(runtime);

    let kind_label = match kind {
        ExternalMiningRejectKind::InvalidPow => "invalid_pow",
        ExternalMiningRejectKind::StaleTemplate => "stale_template",
        ExternalMiningRejectKind::UnknownTemplate => "unknown_template",
        ExternalMiningRejectKind::SubmitBlockError => "submit_block_error",
        ExternalMiningRejectKind::DuplicateBlock => "duplicate_block",
        ExternalMiningRejectKind::InvalidBlock => "invalid_block",
        ExternalMiningRejectKind::ChainIdMismatch => "chain_id_mismatch",
        ExternalMiningRejectKind::InternalError => "internal_error",
        ExternalMiningRejectKind::StorageError => "storage_error",
    };
    let _ = state.storage().append_runtime_event(
        "warn",
        "external_mining_submit_rejected",
        &format!("reason={} {}", kind_label, message),
    );
}

pub async fn post_mining_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitMinedBlockRequest>,
) -> Json<ApiResponse<MiningSubmitData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let pow = pow_validation_result(&req.block.header);
    let pow_accepted_dev = pow.accepted;
    let target_u64 = pow.target_u64;
    let pow_hash_score_u64 = pow.score_u64.unwrap_or(0);
    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.pulsedag_mining_submits_total =
            runtime.pulsedag_mining_submits_total.saturating_add(1);
    }

    if !pow.accepted {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.rejected_mined_blocks += 1;
        runtime.pulsedag_invalid_pow_total = runtime.pulsedag_invalid_pow_total.saturating_add(1);
        runtime.pulsedag_blocks_rejected_total =
            runtime.pulsedag_blocks_rejected_total.saturating_add(1);
        drop(runtime);
        warn!(block_hash = %block_hash, height, "mining submit rejected: invalid PoW");
        record_external_mining_rejection(
            &state,
            ExternalMiningRejectKind::InvalidPow,
            &format!(
                "submitted block does not satisfy {} policy: reason_code={} score={} target={} difficulty={} height={} nonce={}",
                pulsedag_core::selected_pow_name(),
                pow.rejection_code.unwrap_or("score_above_target"),
                pow_hash_score_u64,
                target_u64,
                req.block.header.difficulty,
                req.block.header.height,
                req.block.header.nonce
            ),
        )
        .await;
        return Json(rejection_submit_response(
            "invalid_pow",
            format!(
                "submitted block does not satisfy {} policy: reason_code={} score={} target={} difficulty={} height={} nonce={}",
                pulsedag_core::selected_pow_name(),
                pow.rejection_code.unwrap_or("score_above_target"),
                pow_hash_score_u64,
                target_u64,
                req.block.header.difficulty,
                req.block.header.height,
                req.block.header.nonce
            ),
            Some(block_hash),
            Some(height),
            false,
        ));
    }

    if height <= chain.dag.best_height {
        let msg = format!(
            "reason_code={}; current best height is {} and submitted block height is {}",
            StaleTemplateReason::ChainHeightAdvanced.code(),
            chain.dag.best_height,
            height
        );
        record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
            .await;
        return Json(stale_template_error(
            StaleTemplateReason::ChainHeightAdvanced,
            format!(
                "current best height is {} and submitted block height is {}",
                chain.dag.best_height, height
            ),
        ));
    }

    let lifecycle = current_template_state(&chain);
    let current_parents = lifecycle.parent_hashes.clone();
    let current_selected_tip = lifecycle.selected_tip.clone();
    let expected_template_id = template_id_for_state(&lifecycle);
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if let Some(template_id) = req.template_id.as_ref() {
        if let Some(stored) = load_template(template_id) {
            if stored.height != chain.dag.best_height + 1 {
                let msg = format!(
                    "reason_code={}; template height stale for current next height",
                    StaleTemplateReason::TemplateHeightStale.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateHeightStale,
                    format!(
                        "template height {} is stale; current next height is {}",
                        stored.height,
                        chain.dag.best_height + 1
                    ),
                ));
            }
            if stored.parent_hashes != current_parents {
                let msg = format!(
                    "reason_code={}; template parents mismatch current tips",
                    StaleTemplateReason::TemplateParentsMismatch.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateParentsMismatch,
                    "template parents no longer match current tips",
                ));
            }
            if stored.selected_tip != current_selected_tip {
                let msg = format!(
                    "reason_code={}; template selected_tip mismatch current preferred tip",
                    StaleTemplateReason::TemplateSelectedTipMismatch.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateSelectedTipMismatch,
                    "template selected_tip no longer matches current preferred tip",
                ));
            }
            if stored.difficulty != lifecycle.difficulty
                || stored.target_u64 != lifecycle.target_u64
            {
                let msg = format!(
                    "reason_code={}; template difficulty/target mismatch node state",
                    StaleTemplateReason::TemplateDifficultyTargetMismatch.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateDifficultyTargetMismatch,
                    "template difficulty/target no longer matches node state",
                ));
            }
            if stored.mempool_fingerprint != lifecycle.mempool_fingerprint {
                let msg = format!(
                    "reason_code={}; template mempool view changed",
                    StaleTemplateReason::TemplateMempoolChanged.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateMempoolChanged,
                    "template mempool view changed; refresh template",
                ));
            }
            let (not_before, _expiry, hard_expiry) =
                template_freshness_window(stored.created_at_unix, stored.expires_at_unix);
            if stored.created_at_unix != 0 && now_unix.saturating_add(1) < not_before {
                let msg = format!(
                    "reason_code={}; template created_at is in the future",
                    StaleTemplateReason::TemplateFutureClockSkew.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateFutureClockSkew,
                    "template appears to be from the future; check node/miner clocks and refresh",
                ));
            }
            if now_unix > hard_expiry {
                let msg = format!(
                    "reason_code={}; template freshness window elapsed",
                    StaleTemplateReason::TemplateExpired.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateExpired,
                    format!(
                        "template freshness window elapsed at {}; refresh and retry",
                        hard_expiry
                    ),
                ));
            }
            if template_id != &expected_template_id {
                let msg = format!(
                    "reason_code={}; template lifecycle state changed",
                    StaleTemplateReason::TemplateLifecycleChanged.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::TemplateLifecycleChanged,
                    "template no longer matches current lifecycle state",
                ));
            }
            if req.block.header.difficulty != stored.difficulty {
                let msg = format!(
                    "reason_code={}; submitted header difficulty differs from template difficulty",
                    StaleTemplateReason::SubmittedDifficultyMismatch.code()
                );
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                return Json(stale_template_error(
                    StaleTemplateReason::SubmittedDifficultyMismatch,
                    format!(
                        "submitted difficulty {} does not match template difficulty {}",
                        req.block.header.difficulty, stored.difficulty
                    ),
                ));
            }
            if !stored.template_txids.is_empty() {
                let submitted_txids = req
                    .block
                    .transactions
                    .iter()
                    .map(|tx| tx.txid.as_str())
                    .collect::<Vec<_>>();
                let expected_txids = stored
                    .template_txids
                    .iter()
                    .map(|txid| txid.as_str())
                    .collect::<Vec<_>>();
                if submitted_txids != expected_txids {
                    let msg = format!(
                        "reason_code={}; submitted transaction list differs from template transaction list",
                        StaleTemplateReason::SubmittedTransactionsMismatch.code()
                    );
                    record_external_mining_rejection(
                        &state,
                        ExternalMiningRejectKind::StaleTemplate,
                        &msg,
                    )
                    .await;
                    return Json(stale_template_error(
                        StaleTemplateReason::SubmittedTransactionsMismatch,
                        "submitted transactions differ from template; refresh template and retry",
                    ));
                }
            }
        } else {
            record_external_mining_rejection(
                &state,
                ExternalMiningRejectKind::UnknownTemplate,
                &format!("template_id {} not found", template_id),
            )
            .await;
            return Json(rejection_submit_response(
                "stale_template",
                format!("template_id {} not found", template_id),
                None,
                Some(height),
                true,
            ));
        }
    }

    let mut submitted_parents = req.block.header.parents.clone();
    submitted_parents.sort();
    if submitted_parents != current_parents {
        let msg = format!(
            "reason_code={}; submitted block parents mismatch current tips",
            StaleTemplateReason::SubmittedParentsMismatch.code()
        );
        record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
            .await;
        return Json(stale_template_error(
            StaleTemplateReason::SubmittedParentsMismatch,
            "submitted block parents no longer match current tip set",
        ));
    }

    match accept_block_with_result(req.block.clone(), &mut chain, AcceptSource::Rpc) {
        pulsedag_core::BlockAcceptanceResult::Accepted => {
            let adopted_orphans = adopt_ready_orphans(&mut chain, AcceptSource::Rpc);

            if let Err(e) = persist_then_broadcast_mined_block(
                &req.block,
                &chain,
                |block, chain| state.storage().persist_block_and_chain_state(block, chain),
                |block| {
                    if let Some(p2p) = state.p2p() {
                        let _ = p2p.broadcast_block(block);
                    }
                },
            ) {
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StorageError,
                    &e.to_string(),
                )
                .await;
                return Json(rejection_submit_response(
                    "block_rejected",
                    e.to_string(),
                    Some(block_hash.clone()),
                    Some(height),
                    false,
                ));
            }

            {
                let runtime_handle = state.runtime();
                let mut runtime = runtime_handle.write().await;
                runtime.accepted_mined_blocks += 1;
                runtime.pulsedag_blocks_accepted_total =
                    runtime.pulsedag_blocks_accepted_total.saturating_add(1);
                runtime.external_mining_submit_accepted =
                    runtime.external_mining_submit_accepted.saturating_add(1);
                runtime.adopted_orphan_blocks += adopted_orphans as u64;
            }
            info!(block_hash = %block_hash, height, adopted_orphans, "mining submit accepted");
            let _ = state.storage().append_runtime_event(
                "info",
                "external_mining_submit_accepted",
                &format!(
                    "template_id={} block_hash={} height={} adopted_orphans={}",
                    req.template_id.clone().unwrap_or_else(|| "-".to_string()),
                    block_hash,
                    height,
                    adopted_orphans
                ),
            );

            Json(ApiResponse::ok(MiningSubmitData {
                accepted: true,
                block_hash: Some(block_hash.clone()),
                block_id: Some(block_hash),
                height: Some(height),
                pow_algorithm: pow.algorithm.to_string(),
                pow_accepted: pow.accepted,
                pow_accepted_dev,
                target_u64,
                stale_template: false,
                reason_code: "accepted".to_string(),
                selected_tip: preferred_tip_hash(&chain),
                adopted_orphans,
                pow_hash_score_u64,
                pow_rejection_code: pow.rejection_code.map(|v| v.to_string()),
                pow_rejection_reason: None,
            }))
        }
        outcome => {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.rejected_mined_blocks += 1;
            runtime.pulsedag_blocks_rejected_total =
                runtime.pulsedag_blocks_rejected_total.saturating_add(1);
            drop(runtime);
            warn!(block_hash = %block_hash, height, outcome = ?outcome, "mining submit rejected");
            let rejection_kind = match outcome {
                pulsedag_core::BlockAcceptanceResult::Duplicate => {
                    ExternalMiningRejectKind::DuplicateBlock
                }
                pulsedag_core::BlockAcceptanceResult::InvalidPow => {
                    ExternalMiningRejectKind::InvalidPow
                }
                pulsedag_core::BlockAcceptanceResult::UnknownParent
                | pulsedag_core::BlockAcceptanceResult::InvalidTimestamp
                | pulsedag_core::BlockAcceptanceResult::InvalidStructure => {
                    ExternalMiningRejectKind::InvalidBlock
                }
                pulsedag_core::BlockAcceptanceResult::Rejected(_) => {
                    ExternalMiningRejectKind::SubmitBlockError
                }
                pulsedag_core::BlockAcceptanceResult::Accepted => {
                    ExternalMiningRejectKind::InternalError
                }
            };
            record_external_mining_rejection(
                &state,
                rejection_kind,
                &format!("block acceptance outcome: {:?}", outcome),
            )
            .await;
            Json(ApiResponse::err(
                "SUBMIT_BLOCK_ERROR",
                format!("block acceptance outcome: {:?}", outcome),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{persist_then_broadcast_mined_block, post_mining_submit};
    use crate::{
        api::{GetBlockTemplateRequest, NodeRuntimeStats, RpcStateLike, SubmitMinedBlockRequest},
        handlers::mining_template::{post_mining_template, store_template, StoredMiningTemplate},
    };
    use axum::{extract::State, Json};
    use pulsedag_core::{
        build_candidate_block, build_coinbase_transaction, dev_difficulty_snapshot,
        dev_mine_header, dev_target_u64,
        errors::PulseError,
        preferred_tip_hash,
        state::ChainState,
        types::{Block, Transaction},
    };
    use pulsedag_p2p::{P2pHandle, P2pStatus};
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct FakeP2pHandle {
        broadcast_block_calls: Arc<AtomicUsize>,
    }

    impl FakeP2pHandle {
        fn new() -> Self {
            Self {
                broadcast_block_calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn block_calls(&self) -> usize {
            self.broadcast_block_calls.load(Ordering::SeqCst)
        }
    }

    impl P2pHandle for FakeP2pHandle {
        fn broadcast_transaction(&self, _tx: &Transaction) -> Result<(), PulseError> {
            Ok(())
        }

        fn broadcast_block(&self, _block: &Block) -> Result<(), PulseError> {
            self.broadcast_block_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn status(&self) -> Result<P2pStatus, PulseError> {
            Ok(P2pStatus {
                mode: "test".into(),
                peer_id: "fake".into(),
                listening: vec![],
                connected_peers: vec![],
                topics: vec![],
                mdns: false,
                kademlia: false,
                broadcasted_messages: self.block_calls(),
                publish_attempts: self.block_calls(),
                seen_message_ids: self.block_calls(),
                queued_messages: 0,
                queued_block_messages: 0,
                queued_non_block_messages: 0,
                queue_max_depth: 0,
                dequeued_block_messages: 0,
                dequeued_non_block_messages: 0,
                queue_block_priority_picks: 0,
                queue_priority_tx_lane_picks: 0,
                queue_standard_tx_lane_picks: 0,
                queue_non_block_fair_picks: 0,
                queue_starvation_relief_picks: 0,
                queue_backpressure_drops: 0,
                inbound_messages: 0,
                runtime_started: true,
                runtime_mode_detail: "unit-test".into(),
                swarm_events_seen: 0,
                subscriptions_active: 0,
                last_message_kind: Some("block".into()),
                last_swarm_event: None,
                per_topic_publishes: std::collections::HashMap::new(),
                inbound_decode_failed: 0,
                inbound_chain_mismatch_dropped: 0,
                inbound_duplicates_suppressed: 0,
                tx_outbound_duplicates_suppressed: 0,
                tx_outbound_first_seen_relayed: 0,
                tx_outbound_recovery_relayed: 0,
                tx_outbound_priority_relayed: 0,
                tx_outbound_budget_suppressed: 0,
                tx_outbound_recovery_budget_suppressed: 0,
                block_outbound_duplicates_suppressed: 0,
                block_outbound_first_seen_relayed: 0,
                block_outbound_recovery_relayed: 0,
                last_drop_reason: None,
                peer_reconnect_attempts: 0,
                peer_recovery_success_count: 0,
                last_peer_recovery_unix: None,
                peer_cooldown_suppressed_count: 0,
                peer_flap_suppressed_count: 0,
                peers_under_cooldown: 0,
                peers_under_flap_guard: 0,
                peer_lifecycle_healthy: 0,
                peer_lifecycle_watch: 0,
                peer_lifecycle_degraded: 0,
                peer_lifecycle_cooldown: 0,
                peer_lifecycle_recovering: 0,
                degraded_mode: "unknown".into(),
                connection_shaping_active: false,
                peer_recovery: vec![],
                sync_candidates: vec![],
                selected_sync_peer: None,
                connection_slot_budget: 0,
                connected_slots_in_use: 0,
                available_connection_slots: 0,
                sync_selection_sticky_until_unix: None,
                topology_bucket_count: 8,
                topology_distinct_buckets: 0,
                topology_dominant_bucket_share_bps: 0,
                topology_diversity_score_bps: 0,
            })
        }
    }

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        p2p: Option<Arc<dyn P2pHandle>>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }

        fn p2p(&self) -> Option<Arc<dyn P2pHandle>> {
            self.p2p.clone()
        }

        fn storage(&self) -> Arc<Storage> {
            self.storage.clone()
        }

        fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>> {
            self.runtime.clone()
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    fn build_state_with_fake_p2p() -> (TestState, FakeP2pHandle) {
        let path = temp_db_path("mining-submit-tests");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let fake = FakeP2pHandle::new();
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            p2p: Some(Arc::new(fake.clone())),
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        };
        (state, fake)
    }

    async fn build_mined_block(state: &TestState) -> Block {
        let chain = state.chain.read().await;
        let height = chain.dag.best_height + 1;
        let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        let difficulty =
            u32::try_from(dev_difficulty_snapshot(&chain).suggested_difficulty).unwrap_or(u32::MAX);
        let txs = vec![build_coinbase_transaction("kaspa:qptestminer", 50, height)];
        let mut block = build_candidate_block(parents, height, difficulty, txs);
        let (mined_header, mined, _, _) = dev_mine_header(block.header.clone(), 100_000);
        assert!(mined, "expected test block to satisfy dev pow");
        block.header = mined_header;
        block
    }

    async fn build_mined_template_block(state: &TestState) -> (String, Block) {
        let Json(template_response) = post_mining_template(
            State(state.clone()),
            Json(GetBlockTemplateRequest {
                miner_address: "kaspa:qptestminer".to_string(),
            }),
        )
        .await;
        assert!(template_response.ok);
        let template = template_response.data.expect("template expected");
        let mut block = template.block;
        let (mined_header, mined, _, _) = dev_mine_header(block.header.clone(), 100_000);
        assert!(mined, "expected mined template header");
        block.header = mined_header;
        (template.template_id, block)
    }

    #[tokio::test]
    async fn accepted_block_broadcasts_to_p2p() {
        let (state, fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;

        let Json(response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block,
            }),
        )
        .await;

        assert!(response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.accepted_mined_blocks, 1);
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_accepted, 1);
        assert_eq!(runtime.external_mining_submit_rejected, 0);
        drop(runtime);
        let events = state.storage.list_runtime_events(50).unwrap();
        assert!(events
            .iter()
            .any(|event| event.kind == "external_mining_submit_accepted"));
    }

    #[tokio::test]
    async fn valid_template_leads_to_acceptable_mined_block() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (template_id, block) = build_mined_template_block(&state).await;

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            }),
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(data.accepted);
        assert!(data.pow_accepted_dev);
        assert!(data.pow_hash_score_u64 <= data.target_u64);
    }

    #[tokio::test]
    async fn rejected_block_does_not_broadcast() {
        let (state, fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;

        let Json(first_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block: block.clone(),
            }),
        )
        .await;
        assert!(first_response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);

        let Json(second_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block,
            }),
        )
        .await;

        assert!(!second_response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.accepted_mined_blocks, 1);
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_accepted, 1);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_rejected_stale_template, 1);
    }

    #[tokio::test]
    async fn stale_template_rejected_when_mempool_changes() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let Json(template_response) = post_mining_template(
            State(state.clone()),
            Json(GetBlockTemplateRequest {
                miner_address: "kaspa:qptestminer".to_string(),
            }),
        )
        .await;
        assert!(template_response.ok);
        let template = template_response.data.expect("template expected");

        {
            let mut chain = state.chain.write().await;
            let tx =
                build_coinbase_transaction("kaspa:qptestmempool", 1, chain.dag.best_height + 1);
            chain.mempool.transactions.insert(tx.txid.clone(), tx);
        }

        let mut block = template.block;
        let (mined_header, mined, _, _) = dev_mine_header(block.header.clone(), 100_000);
        assert!(mined);
        block.header = mined_header;

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: Some(template.template_id),
                block,
            }),
        )
        .await;

        assert!(!submit_response.ok);
        let err = submit_response.error.expect("error expected");
        assert_eq!(err.code, "STALE_TEMPLATE");
        assert!(err.message.contains("reason_code=template_mempool_changed"));
        assert!(err.message.contains("mempool view changed"));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_stale_work_detected, 1);
    }

    #[tokio::test]
    async fn malformed_submit_is_rejected_cleanly() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;
        let mut malformed = block.clone();
        malformed.header.parents.clear();
        malformed.header.difficulty = 1;
        let (mined_header, mined, _, _) = dev_mine_header(malformed.header.clone(), 100_000);
        assert!(mined);
        malformed.header = mined_header;

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block: malformed,
            }),
        )
        .await;

        assert!(!submit_response.ok);
        let err = submit_response.error.expect("error expected");
        assert_eq!(err.code, "STALE_TEMPLATE");
        assert!(err
            .message
            .contains("reason_code=submitted_parents_mismatch"));
        assert!(err.message.contains("parents"));
    }

    #[tokio::test]
    async fn invalid_pow_submit_returns_diagnostic() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (_template_id, mut block) = build_mined_template_block(&state).await;
        block.header.difficulty = u32::MAX;
        block.header.nonce = 0;

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block,
            }),
        )
        .await;

        assert!(!submit_response.ok);
        let err = submit_response.error.expect("error expected");
        assert_eq!(err.code, "INVALID_POW");
        assert!(err.message.contains("score="));
        assert!(err.message.contains("target="));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.rejected_mined_blocks, 1);
        assert_eq!(runtime.external_mining_rejected_invalid_pow, 1);
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("invalid_pow")
        );
        assert!(runtime
            .external_mining_last_invalid_pow_reason
            .as_deref()
            .is_some_and(|msg| msg.contains("score=")));
    }

    #[tokio::test]
    async fn no_regression_in_mining_rpc_flow() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (template_id, block) = build_mined_template_block(&state).await;

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            }),
        )
        .await;
        assert!(submit_response.ok);
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_templates_emitted, 1);
        assert_eq!(runtime.accepted_mined_blocks, 1);
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_accepted, 1);
        assert_eq!(runtime.external_mining_submit_rejected, 0);
    }

    #[tokio::test]
    async fn stale_template_rejected_when_expired() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;
        let chain = state.chain.read().await;
        let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        drop(chain);

        let template_id = "expired-template".to_string();
        let selected_tip = {
            let chain = state.chain.read().await;
            preferred_tip_hash(&chain)
        };
        store_template(&StoredMiningTemplate {
            template_id: template_id.clone(),
            miner_address: "kaspa:qptestminer".to_string(),
            selected_tip,
            parent_hashes: parents,
            height: block.header.height,
            difficulty: block.header.difficulty,
            created_at_unix: 1,
            target_u64: dev_target_u64(block.header.difficulty as u64),
            mempool_fingerprint: "0:".to_string(),
            mempool_tx_count: 0,
            expires_at_unix: 1,
            template_txids: block
                .transactions
                .iter()
                .map(|tx| tx.txid.clone())
                .collect(),
        });

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            }),
        )
        .await;

        assert!(!submit_response.ok);
        let err = submit_response.error.expect("error expected");
        assert_eq!(err.code, "STALE_TEMPLATE");
        assert!(err.message.contains("reason_code=template_expired"));
        assert!(err.message.contains("freshness window elapsed"));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_rejected_stale_template, 1);
    }

    #[tokio::test]
    async fn stale_template_reason_codes_are_explicit_and_stable() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (template_id, mut block) = build_mined_template_block(&state).await;
        block.transactions.push(build_coinbase_transaction(
            "kaspa:qptest-extra",
            1,
            block.header.height,
        ));

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            }),
        )
        .await;

        assert!(!submit_response.ok);
        let err = submit_response.error.expect("error expected");
        assert_eq!(err.code, "STALE_TEMPLATE");
        assert!(err
            .message
            .contains("reason_code=submitted_transactions_mismatch"));
        let runtime = state.runtime.read().await;
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("stale_template")
        );
        assert!(runtime
            .external_mining_last_rejection_reason
            .as_deref()
            .is_some_and(|msg| msg.contains("reason_code=submitted_transactions_mismatch")));
    }

    #[tokio::test]
    async fn submit_rejection_classifies_invalid_block_explicitly() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let mut block = build_mined_block(&state).await;
        block.transactions.clear();

        let Json(submit_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block,
            }),
        )
        .await;

        assert!(!submit_response.ok);
        let err = submit_response.error.expect("error expected");
        assert_eq!(err.code, "SUBMIT_BLOCK_ERROR");
        assert!(err.message.contains("invalid block"));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.rejected_mined_blocks, 1);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_rejected_invalid_block, 1);
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("invalid_block")
        );
    }

    #[tokio::test]
    async fn template_invalidation_updates_runtime_metrics() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let Json(first) = post_mining_template(
            State(state.clone()),
            Json(GetBlockTemplateRequest {
                miner_address: "kaspa:qptestminer".to_string(),
            }),
        )
        .await;
        assert!(first.ok);
        {
            let mut chain = state.chain.write().await;
            let tx =
                build_coinbase_transaction("kaspa:qptestmempool", 1, chain.dag.best_height + 1);
            chain.mempool.transactions.insert(tx.txid.clone(), tx);
        }
        let Json(second) = post_mining_template(
            State(state.clone()),
            Json(GetBlockTemplateRequest {
                miner_address: "kaspa:qptestminer".to_string(),
            }),
        )
        .await;
        assert!(second.ok);
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_templates_emitted, 2);
        assert_eq!(runtime.external_mining_templates_invalidated, 1);
        assert_eq!(runtime.external_mining_stale_work_detected, 1);
        drop(runtime);
        let events = state.storage.list_runtime_events(50).unwrap();
        assert!(events
            .iter()
            .any(|event| event.kind == "external_mining_template_invalidated"));
    }

    #[test]
    fn persist_error_does_not_broadcast() {
        let path = temp_db_path("persist-error-tests");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let height = chain.dag.best_height + 1;
        let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        let difficulty =
            u32::try_from(dev_difficulty_snapshot(&chain).suggested_difficulty).unwrap_or(u32::MAX);
        let txs = vec![build_coinbase_transaction("kaspa:qptestminer", 50, height)];
        let block = build_candidate_block(parents, height, difficulty, txs);
        let broadcasts = Arc::new(AtomicUsize::new(0));
        let broadcast_counter = broadcasts.clone();

        let result = persist_then_broadcast_mined_block(
            &block,
            &chain,
            |_block, _chain| Err(PulseError::StorageError("forced persist failure".into())),
            |_block| {
                broadcast_counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_err());
        assert_eq!(broadcasts.load(Ordering::SeqCst), 0);
    }
}
