use super::mining_template::{
    current_template_state, load_template, template_freshness_window, template_id_for_state,
    MINING_PROTOCOL_VERSION,
};
use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_block_atomically, adopt_ready_orphans, expected_difficulty, pow_validation_result,
    preferred_tip_hash, AcceptSource,
};
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use tracing::{info, warn};

#[derive(Debug, serde::Serialize)]
pub struct MiningSubmitData {
    pub accepted: bool,
    pub reason: String,
    pub block_hash: Option<String>,
    pub block_id: Option<String>,
    pub height: Option<u64>,
    pub pow_algorithm: String,
    pub pow_accepted: bool,
    pub pow_accepted_dev: bool,
    pub protocol_version: u32,
    pub target_u64: u64,
    pub target_hex: String,
    pub pow_hash: Option<String>,
    pub template_id: Option<String>,
    pub invalid_pow: bool,
    pub stale: bool,
    pub duplicate: bool,
    pub stale_template: bool,
    pub reason_code: String,
    pub selected_tip: Option<String>,
    pub adopted_orphans: usize,
    pub pow_hash_score_u64: u64,
    pub pow_rejection_code: Option<String>,
    pub pow_rejection_reason: Option<String>,
}

#[cfg(test)]
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

const SUBMIT_CHAIN_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const SUBMIT_ORPHAN_ADOPTION_CHAIN_WRITE_TIMEOUT: Duration = Duration::from_millis(100);
const SUBMIT_POST_ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
const MINING_SUBMIT_ACTOR_QUEUE_SIZE: usize = 1;
const MINING_SUBMIT_ACTOR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
struct MiningSubmitState {
    chain: Arc<tokio::sync::RwLock<pulsedag_core::state::ChainState>>,
    storage: Arc<pulsedag_storage::Storage>,
    p2p: Option<Arc<dyn pulsedag_p2p::P2pHandle>>,
    runtime: Arc<tokio::sync::RwLock<crate::api::NodeRuntimeStats>>,
}

impl MiningSubmitState {
    fn from_rpc_state<S: RpcStateLike>(state: &S) -> Self {
        Self {
            chain: state.chain(),
            storage: state.storage(),
            p2p: state.p2p(),
            runtime: state.runtime(),
        }
    }
}

impl RpcStateLike for MiningSubmitState {
    fn chain(&self) -> Arc<tokio::sync::RwLock<pulsedag_core::state::ChainState>> {
        self.chain.clone()
    }

    fn p2p(&self) -> Option<Arc<dyn pulsedag_p2p::P2pHandle>> {
        self.p2p.clone()
    }

    fn storage(&self) -> Arc<pulsedag_storage::Storage> {
        self.storage.clone()
    }

    fn runtime(&self) -> Arc<tokio::sync::RwLock<crate::api::NodeRuntimeStats>> {
        self.runtime.clone()
    }
}

type SubmitBlockResponse = Json<ApiResponse<MiningSubmitData>>;

struct SubmitBlockCommand {
    state: MiningSubmitState,
    req: SubmitMinedBlockRequest,
    submit_started: Instant,
    response: oneshot::Sender<SubmitBlockResponse>,
}

#[derive(Clone)]
struct MiningSubmitActorHandle {
    sender: mpsc::Sender<SubmitBlockCommand>,
}

impl MiningSubmitActorHandle {
    fn spawn(queue_size: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel::<SubmitBlockCommand>(queue_size);
        tokio::spawn(async move {
            while let Some(command) = receiver.recv().await {
                let response =
                    process_mining_submit(command.state, command.req, command.submit_started).await;
                let _ = command.response.send(response);
            }
        });
        Self { sender }
    }

    fn queue_len(&self) -> u64 {
        self.sender
            .max_capacity()
            .saturating_sub(self.sender.capacity()) as u64
    }

    fn max_capacity(&self) -> usize {
        self.sender.max_capacity()
    }

    #[allow(clippy::result_large_err)]
    fn try_submit(
        &self,
        command: SubmitBlockCommand,
    ) -> Result<(), mpsc::error::TrySendError<SubmitBlockCommand>> {
        self.sender.try_send(command)
    }
}

fn mining_submit_actor() -> MiningSubmitActorHandle {
    static ACTOR: OnceLock<MiningSubmitActorHandle> = OnceLock::new();
    ACTOR
        .get_or_init(|| MiningSubmitActorHandle::spawn(MINING_SUBMIT_ACTOR_QUEUE_SIZE))
        .clone()
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

async fn record_submit_phase<S: RpcStateLike>(state: &S, phase: &'static str) {
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_last_submit_phase = Some(phase.to_string());
}

async fn record_submit_started<S: RpcStateLike>(state: &S) {
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_submit_started_total = runtime
        .external_mining_submit_started_total
        .saturating_add(1);
    runtime.external_mining_last_submit_phase = Some("received".to_string());
}

async fn record_submit_inflight<S: RpcStateLike>(state: &S) {
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_submit_inflight =
        runtime.external_mining_submit_inflight.saturating_add(1);
}

async fn record_submit_completed<S: RpcStateLike>(
    state: &S,
    started: Instant,
    phase: &'static str,
) {
    let duration = elapsed_ms(started);
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_submit_inflight =
        runtime.external_mining_submit_inflight.saturating_sub(1);
    runtime.external_mining_submit_completed_total = runtime
        .external_mining_submit_completed_total
        .saturating_add(1);
    runtime.external_mining_submit_actor_completed_total = runtime
        .external_mining_submit_actor_completed_total
        .saturating_add(1);
    runtime.external_mining_last_submit_phase = Some(phase.to_string());
    runtime.external_mining_last_submit_duration_ms = duration;
    runtime.external_mining_max_submit_duration_ms =
        runtime.external_mining_max_submit_duration_ms.max(duration);
}

#[derive(Clone, Copy)]
enum ExternalMiningRejectKind {
    InvalidPow,
    StaleTemplate,
    MissingTemplateId,
    UnknownTemplate,
    SubmitBlockError,
    DuplicateBlock,
    MissingParent,
    InvalidTimestamp,
    InvalidCoinbase,
    InvalidMerkleOrPayload,
    MalformedSerialization,
    UnknownValidationError,
    InternalError,
}

fn classify_rejected_validation_message(message: &str) -> (&'static str, ExternalMiningRejectKind) {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timestamp") {
        (
            "invalid_timestamp",
            ExternalMiningRejectKind::InvalidTimestamp,
        )
    } else if lower.contains("coinbase") || lower.contains("reward") {
        (
            "invalid_coinbase",
            ExternalMiningRejectKind::InvalidCoinbase,
        )
    } else if lower.contains("merkle")
        || lower.contains("transaction")
        || lower.contains("payload")
        || lower.contains("txid")
    {
        (
            "invalid_merkle_or_payload",
            ExternalMiningRejectKind::InvalidMerkleOrPayload,
        )
    } else if lower.contains("invalid state root")
        && lower.contains("classification=stale_template")
    {
        ("stale_template", ExternalMiningRejectKind::StaleTemplate)
    } else if lower.contains("invalid state root") {
        (
            "invalid_state_root",
            ExternalMiningRejectKind::UnknownValidationError,
        )
    } else if lower.contains("deserialize") || lower.contains("serialization") {
        (
            "malformed_serialization",
            ExternalMiningRejectKind::MalformedSerialization,
        )
    } else {
        (
            "unknown_validation_error",
            ExternalMiningRejectKind::UnknownValidationError,
        )
    }
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
    SubmittedTransactionsMismatch,
    SubmittedParentsMismatch,
    SubmittedMerkleRootMismatch,
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
            Self::SubmittedTransactionsMismatch => "submitted_transactions_mismatch",
            Self::SubmittedParentsMismatch => "submitted_parents_mismatch",
            Self::SubmittedMerkleRootMismatch => "submitted_merkle_root_mismatch",
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
    rejection_submit_response_with_pow(
        reason_code,
        detail,
        block_hash,
        height,
        stale_template,
        0,
        format!("{:064x}", 0u64),
        None,
        0,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn rejection_submit_response_with_pow(
    reason_code: impl Into<String>,
    detail: impl Into<String>,
    block_hash: Option<String>,
    height: Option<u64>,
    stale_template: bool,
    target_u64: u64,
    target_hex: String,
    pow_hash: Option<String>,
    pow_hash_score_u64: u64,
    pow_rejection_code: Option<String>,
) -> ApiResponse<MiningSubmitData> {
    let reason_code = reason_code.into();
    let reason_detail = detail.into();
    let duplicate = reason_code == "duplicate_block";
    ApiResponse::ok(MiningSubmitData {
        accepted: false,
        reason: reason_detail.clone(),
        block_hash,
        block_id: None,
        height,
        pow_algorithm: pulsedag_core::selected_pow_name().to_string(),
        pow_accepted: false,
        pow_accepted_dev: false,
        protocol_version: MINING_PROTOCOL_VERSION,
        target_u64,
        target_hex,
        pow_hash,
        template_id: None,
        invalid_pow: reason_code == "invalid_pow",
        stale: stale_template,
        duplicate,
        stale_template,
        reason_code,
        selected_tip: None,
        adopted_orphans: 0,
        pow_hash_score_u64,
        pow_rejection_code,
        pow_rejection_reason: Some(reason_detail),
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
            ExternalMiningRejectKind::MissingTemplateId => "missing_template_id",
            ExternalMiningRejectKind::UnknownTemplate => "unknown_template",
            ExternalMiningRejectKind::SubmitBlockError => "submit_block_error",
            ExternalMiningRejectKind::DuplicateBlock => "duplicate_block",
            ExternalMiningRejectKind::MissingParent => "missing_parent",
            ExternalMiningRejectKind::InvalidTimestamp => "invalid_timestamp",
            ExternalMiningRejectKind::InvalidCoinbase => "invalid_coinbase",
            ExternalMiningRejectKind::InvalidMerkleOrPayload => "invalid_merkle_or_payload",
            ExternalMiningRejectKind::MalformedSerialization => "malformed_serialization",
            ExternalMiningRejectKind::UnknownValidationError => "unknown_validation_error",
            ExternalMiningRejectKind::InternalError => "internal_error",
        }
        .to_string(),
    );
    runtime.external_mining_last_rejection_reason = Some(message.to_string());
    let reason_label = match kind {
        ExternalMiningRejectKind::InvalidPow => "invalid_pow",
        ExternalMiningRejectKind::StaleTemplate => "stale_template",
        ExternalMiningRejectKind::MissingTemplateId => "missing_template_id",
        ExternalMiningRejectKind::UnknownTemplate => "unknown_template",
        ExternalMiningRejectKind::SubmitBlockError => "submit_block_error",
        ExternalMiningRejectKind::DuplicateBlock => "duplicate_block",
        ExternalMiningRejectKind::MissingParent => "missing_parent",
        ExternalMiningRejectKind::InvalidTimestamp => "invalid_timestamp",
        ExternalMiningRejectKind::InvalidCoinbase => "invalid_coinbase",
        ExternalMiningRejectKind::InvalidMerkleOrPayload => "invalid_merkle_or_payload",
        ExternalMiningRejectKind::MalformedSerialization => "malformed_serialization",
        ExternalMiningRejectKind::UnknownValidationError => "unknown_validation_error",
        ExternalMiningRejectKind::InternalError => "internal_error",
    };
    runtime.record_rejected_block_reason(reason_label);
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
        ExternalMiningRejectKind::MissingTemplateId | ExternalMiningRejectKind::UnknownTemplate => {
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
        ExternalMiningRejectKind::MissingParent
        | ExternalMiningRejectKind::InvalidTimestamp
        | ExternalMiningRejectKind::InvalidCoinbase
        | ExternalMiningRejectKind::InvalidMerkleOrPayload
        | ExternalMiningRejectKind::MalformedSerialization
        | ExternalMiningRejectKind::UnknownValidationError => {
            runtime.external_mining_rejected_invalid_block = runtime
                .external_mining_rejected_invalid_block
                .saturating_add(1);
        }
        ExternalMiningRejectKind::InternalError => {
            runtime.external_mining_rejected_internal_error = runtime
                .external_mining_rejected_internal_error
                .saturating_add(1);
        }
    }
    drop(runtime);

    let kind_label = match kind {
        ExternalMiningRejectKind::InvalidPow => "invalid_pow",
        ExternalMiningRejectKind::StaleTemplate => "stale_template",
        ExternalMiningRejectKind::MissingTemplateId => "missing_template_id",
        ExternalMiningRejectKind::UnknownTemplate => "unknown_template",
        ExternalMiningRejectKind::SubmitBlockError => "submit_block_error",
        ExternalMiningRejectKind::DuplicateBlock => "duplicate_block",
        ExternalMiningRejectKind::MissingParent => "missing_parent",
        ExternalMiningRejectKind::InvalidTimestamp => "invalid_timestamp",
        ExternalMiningRejectKind::InvalidCoinbase => "invalid_coinbase",
        ExternalMiningRejectKind::InvalidMerkleOrPayload => "invalid_merkle_or_payload",
        ExternalMiningRejectKind::MalformedSerialization => "malformed_serialization",
        ExternalMiningRejectKind::UnknownValidationError => "unknown_validation_error",
        ExternalMiningRejectKind::InternalError => "internal_error",
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
    post_mining_submit_with_actor(state, req, mining_submit_actor()).await
}

async fn post_mining_submit_with_actor<S: RpcStateLike>(
    state: S,
    req: SubmitMinedBlockRequest,
    actor: MiningSubmitActorHandle,
) -> Json<ApiResponse<MiningSubmitData>> {
    let submit_started = Instant::now();
    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;
    record_submit_started(&state).await;

    if req.template_id.is_none() {
        let detail =
            "submit request missing required template_id; refresh template and retry".to_string();
        record_submit_phase(&state, "precheck_rejected").await;
        record_external_mining_rejection(
            &state,
            ExternalMiningRejectKind::MissingTemplateId,
            &detail,
        )
        .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(rejection_submit_response(
            "missing_template_id",
            detail,
            Some(block_hash),
            Some(height),
            true,
        ));
    }

    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.external_mining_submit_actor_queue_len = actor.queue_len();
    }

    let (response, receiver) = oneshot::channel();
    let command = SubmitBlockCommand {
        state: MiningSubmitState::from_rpc_state(&state),
        req,
        submit_started,
        response,
    };

    if let Err(err) = actor.try_submit(command) {
        let detail = format!(
            "mining submit rejected because bounded submit actor queue is full; queue_size={} queue_len={}",
            actor.max_capacity(),
            actor.queue_len()
        );
        {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.external_mining_submit_actor_queue_len = actor.queue_len();
            runtime.external_mining_submit_actor_queue_full_total = runtime
                .external_mining_submit_actor_queue_full_total
                .saturating_add(1);
        }
        let failed_command = match err {
            mpsc::error::TrySendError::Full(command)
            | mpsc::error::TrySendError::Closed(command) => command,
        };
        record_submit_phase(&state, "busy").await;
        record_external_mining_rejection(&state, ExternalMiningRejectKind::InternalError, &detail)
            .await;
        let _ = state.storage().append_runtime_event(
            "warn",
            "external_mining_submit_busy",
            &format!("block_hash={} height={} {}", block_hash, height, detail),
        );
        drop(failed_command);
        record_submit_completed(&state, submit_started, "busy").await;
        return Json(rejection_submit_response(
            "submit_busy",
            detail,
            Some(block_hash),
            Some(height),
            false,
        ));
    }

    match timeout(MINING_SUBMIT_ACTOR_RESPONSE_TIMEOUT, receiver).await {
        Ok(Ok(response)) => response,
        Ok(Err(_closed)) => {
            let detail = "mining submit actor stopped before returning a response".to_string();
            record_actor_timeout(&state, &detail).await;
            record_submit_completed(&state, submit_started, "timeout").await;
            Json(rejection_submit_response(
                "submit_timeout",
                detail,
                Some(block_hash),
                Some(height),
                false,
            ))
        }
        Err(_) => {
            let detail = format!(
                "mining submit actor did not return within {}ms",
                MINING_SUBMIT_ACTOR_RESPONSE_TIMEOUT.as_millis()
            );
            record_actor_timeout(&state, &detail).await;
            record_submit_completed(&state, submit_started, "timeout").await;
            Json(rejection_submit_response(
                "submit_timeout",
                detail,
                Some(block_hash),
                Some(height),
                false,
            ))
        }
    }
}

async fn record_actor_timeout<S: RpcStateLike>(state: &S, detail: &str) {
    let runtime_handle = state.runtime();
    let mut runtime = runtime_handle.write().await;
    runtime.external_mining_submit_actor_timeout_total = runtime
        .external_mining_submit_actor_timeout_total
        .saturating_add(1);
    runtime.external_mining_submit_timeout_total = runtime
        .external_mining_submit_timeout_total
        .saturating_add(1);
    runtime.external_mining_last_submit_phase = Some("timeout".to_string());
    runtime.external_mining_last_rejection_kind = Some("submit_timeout".to_string());
    runtime.external_mining_last_rejection_reason = Some(detail.to_string());
    runtime.record_rejected_block_reason("submit_timeout");
}

async fn process_mining_submit(
    state: MiningSubmitState,
    req: SubmitMinedBlockRequest,
    submit_started: Instant,
) -> SubmitBlockResponse {
    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;

    record_submit_phase(&state, "precheck").await;
    let pow = pow_validation_result(&req.block.header);
    let pow_accepted_dev = pow.accepted;
    let target_u64 = pow.target_u64;
    let pow_hash_score_u64 = pow.score_u64.unwrap_or(0);
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.pulsedag_mining_submits_total =
            runtime.pulsedag_mining_submits_total.saturating_add(1);
    }
    let chain_handle = state.chain();
    record_submit_phase(&state, "waiting_chain_write").await;
    record_submit_inflight(&state).await;
    let lock_wait = Instant::now();
    let mut chain = match timeout(SUBMIT_CHAIN_WRITE_TIMEOUT, chain_handle.write()).await {
        Ok(chain) => {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.external_mining_submit_lock_wait_ms = elapsed_ms(lock_wait);
            chain
        }
        Err(_) => {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.external_mining_submit_timeout_total = runtime
                .external_mining_submit_timeout_total
                .saturating_add(1);
            runtime.external_mining_last_submit_phase = Some("timeout".to_string());
            drop(runtime);
            record_submit_completed(&state, submit_started, "timeout").await;
            return Json(rejection_submit_response(
                "submit_timeout_before_acceptance",
                format!(
                    "submit_block could not acquire chain write lock within {}ms before acceptance",
                    SUBMIT_CHAIN_WRITE_TIMEOUT.as_millis()
                ),
                Some(block_hash),
                Some(height),
                false,
            ));
        }
    };

    if chain.dag.blocks.contains_key(&block_hash) {
        let detail = format!(
            "duplicate block submit: block_hash={} height={}",
            block_hash, height
        );
        drop(chain);
        record_external_mining_rejection(&state, ExternalMiningRejectKind::DuplicateBlock, &detail)
            .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(rejection_submit_response_with_pow(
            "duplicate_block",
            detail,
            Some(block_hash),
            Some(height),
            false,
            target_u64,
            pow.target_hex.clone(),
            pow.hash_hex.clone(),
            pow_hash_score_u64,
            pow.rejection_code.map(|v| v.to_string()),
        ));
    }

    let Some(template_id) = req.template_id.as_ref() else {
        let detail =
            "submit request missing required template_id; refresh template and retry".to_string();
        drop(chain);
        record_external_mining_rejection(
            &state,
            ExternalMiningRejectKind::MissingTemplateId,
            &detail,
        )
        .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(rejection_submit_response(
            "missing_template_id",
            detail,
            Some(block_hash.clone()),
            Some(height),
            true,
        ));
    };

    if height <= chain.dag.best_height {
        let best_height = chain.dag.best_height;
        let msg = format!(
            "reason_code={}; current best height is {} and submitted block height is {}",
            StaleTemplateReason::ChainHeightAdvanced.code(),
            best_height,
            height
        );
        drop(chain);
        record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
            .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(stale_template_error(
            StaleTemplateReason::ChainHeightAdvanced,
            format!(
                "current best height is {} and submitted block height is {}",
                best_height, height
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

    if let Some(stored) = load_template(template_id) {
        if req.block.header.height != stored.height {
            let detail = format!(
                "submitted block height {} does not match template height {}; refresh template and retry",
                req.block.header.height, stored.height
            );
            drop(chain);
            record_external_mining_rejection(
                &state,
                ExternalMiningRejectKind::StaleTemplate,
                &detail,
            )
            .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(rejection_submit_response(
                "stale_template",
                detail,
                Some(block_hash.clone()),
                Some(height),
                true,
            ));
        }
        let current_next_height = chain.dag.best_height + 1;
        if stored.height != current_next_height {
            let msg = format!(
                "reason_code={}; template height stale for current next height",
                StaleTemplateReason::TemplateHeightStale.code()
            );
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(stale_template_error(
                StaleTemplateReason::TemplateHeightStale,
                format!(
                    "template height {} is stale; current next height is {}",
                    stored.height, current_next_height
                ),
            ));
        }
        if stored.parent_hashes != current_parents {
            let msg = format!(
                "reason_code={}; template parents mismatch current tips",
                StaleTemplateReason::TemplateParentsMismatch.code()
            );
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
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
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(stale_template_error(
                StaleTemplateReason::TemplateSelectedTipMismatch,
                "template selected_tip no longer matches current preferred tip",
            ));
        }
        if stored.difficulty != lifecycle.difficulty || stored.target_u64 != lifecycle.target_u64 {
            let msg = format!(
                "reason_code={}; template difficulty/target mismatch node state",
                StaleTemplateReason::TemplateDifficultyTargetMismatch.code()
            );
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
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
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
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
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
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
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
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
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(stale_template_error(
                StaleTemplateReason::TemplateLifecycleChanged,
                "template no longer matches current lifecycle state",
            ));
        }
        if !stored.merkle_root.is_empty() && req.block.header.merkle_root != stored.merkle_root {
            let msg = format!(
                "reason_code={}; submitted header merkle root differs from template merkle root",
                StaleTemplateReason::SubmittedMerkleRootMismatch.code()
            );
            drop(chain);
            record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
                .await;
            record_submit_completed(&state, submit_started, "rejected").await;
            return Json(stale_template_error(
                StaleTemplateReason::SubmittedMerkleRootMismatch,
                "submitted merkle root does not match template merkle root",
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
                drop(chain);
                record_external_mining_rejection(
                    &state,
                    ExternalMiningRejectKind::StaleTemplate,
                    &msg,
                )
                .await;
                record_submit_completed(&state, submit_started, "rejected").await;
                return Json(stale_template_error(
                    StaleTemplateReason::SubmittedTransactionsMismatch,
                    "submitted transactions differ from template; refresh template and retry",
                ));
            }
        }
    } else {
        drop(chain);
        record_external_mining_rejection(
            &state,
            ExternalMiningRejectKind::UnknownTemplate,
            &format!("template_id {} not found", template_id),
        )
        .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(rejection_submit_response(
            "unknown_template",
            format!("template_id {} not found", template_id),
            None,
            Some(height),
            true,
        ));
    }

    let mut submitted_parents = req.block.header.parents.clone();
    submitted_parents.sort();
    if submitted_parents != current_parents {
        let msg = format!(
            "reason_code={}; submitted block parents mismatch current tips",
            StaleTemplateReason::SubmittedParentsMismatch.code()
        );
        drop(chain);
        record_external_mining_rejection(&state, ExternalMiningRejectKind::StaleTemplate, &msg)
            .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(stale_template_error(
            StaleTemplateReason::SubmittedParentsMismatch,
            "submitted block parents no longer match current tip set",
        ));
    }

    let expected_difficulty = expected_difficulty(&chain);
    if req.block.header.difficulty != expected_difficulty {
        let detail = format!(
            "submitted difficulty {} does not match expected consensus difficulty {}; score={} target={} height={} nonce={}",
            req.block.header.difficulty,
            expected_difficulty,
            pow_hash_score_u64,
            target_u64,
            req.block.header.height,
            req.block.header.nonce
        );
        drop(chain);
        {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.rejected_mined_blocks = runtime.rejected_mined_blocks.saturating_add(1);
            runtime.pulsedag_blocks_rejected_total =
                runtime.pulsedag_blocks_rejected_total.saturating_add(1);
        }
        record_external_mining_rejection(&state, ExternalMiningRejectKind::InvalidPow, &detail)
            .await;
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(rejection_submit_response_with_pow(
            "invalid_pow",
            detail,
            Some(block_hash),
            Some(height),
            false,
            target_u64,
            pow.target_hex.clone(),
            pow.hash_hex.clone(),
            pow_hash_score_u64,
            pow.rejection_code.map(|v| v.to_string()),
        ));
    }

    if !pow.accepted {
        drop(chain);
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
        record_submit_completed(&state, submit_started, "rejected").await;
        return Json(rejection_submit_response_with_pow(
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
            target_u64,
            pow.target_hex.clone(),
            pow.hash_hex.clone(),
            pow_hash_score_u64,
            pow.rejection_code.map(|v| v.to_string()),
        ));
    }

    let accept_started = Instant::now();
    let accept_ms;
    let acceptance = match accept_block_atomically(
        req.block.clone(),
        &mut chain,
        AcceptSource::Rpc,
        |block, chain| state.storage().persist_block_and_chain_state(block, chain),
        |_block| Ok(()),
    ) {
        Ok(acceptance) => {
            accept_ms = elapsed_ms(accept_started);
            acceptance.result
        }
        Err(e) => {
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
    };

    match acceptance {
        pulsedag_core::BlockAcceptanceResult::Accepted => {
            let selected_tip = preferred_tip_hash(&chain);
            drop(chain);

            record_submit_phase(&state, "accepted").await;
            let post_accept_started = Instant::now();
            record_submit_phase(&state, "orphan_adoption").await;

            // Bounded post-accept orphan adoption phase with its own short write-lock attempt.
            let adopted_orphans = match timeout(
                SUBMIT_POST_ACCEPT_TIMEOUT,
                perform_orphan_adoption(&state, &block_hash),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    warn!(
                        block_hash = %block_hash,
                        "orphan adoption exceeded timeout; proceeding without adoption"
                    );
                    0
                }
            };

            record_submit_phase(&state, "broadcasting").await;
            if let Some(p2p) = state.p2p() {
                let _ = p2p.broadcast_block(&req.block);
            }
            {
                let runtime_handle = state.runtime();
                let mut runtime = runtime_handle.write().await;
                runtime.external_mining_submit_accept_ms = accept_ms;
                runtime.external_mining_submit_post_accept_ms = elapsed_ms(post_accept_started);
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

            record_submit_phase(&state, "responding").await;
            record_submit_completed(&state, submit_started, "completed").await;
            Json(ApiResponse::ok(MiningSubmitData {
                accepted: true,
                reason: "accepted".to_string(),
                block_hash: Some(block_hash.clone()),
                block_id: Some(block_hash),
                height: Some(height),
                pow_algorithm: pow.algorithm.to_string(),
                pow_accepted: pow.accepted,
                pow_accepted_dev,
                protocol_version: MINING_PROTOCOL_VERSION,
                target_u64,
                target_hex: pow.target_hex.clone(),
                pow_hash: pow.hash_hex.clone(),
                template_id: req.template_id.clone(),
                invalid_pow: false,
                stale: false,
                duplicate: false,
                stale_template: false,
                reason_code: "accepted".to_string(),
                selected_tip,
                adopted_orphans,
                pow_hash_score_u64,
                pow_rejection_code: pow.rejection_code.map(|v| v.to_string()),
                pow_rejection_reason: None,
            }))
        }
        outcome => {
            let invalid_state_root_diagnostics = match &outcome {
                pulsedag_core::BlockAcceptanceResult::Rejected(message)
                    if message.contains("invalid state root") =>
                {
                    pulsedag_core::validation::compute_post_state_root(&req.block, &chain)
                        .ok()
                        .map(|computed| {
                            pulsedag_core::invalid_state_root_diagnostics(
                                &req.block, &chain, computed,
                            )
                        })
                }
                _ => None,
            };
            drop(chain);
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.rejected_mined_blocks += 1;
            runtime.pulsedag_blocks_rejected_total =
                runtime.pulsedag_blocks_rejected_total.saturating_add(1);
            if let Some(diagnostics) = &invalid_state_root_diagnostics {
                runtime.record_invalid_state_root(diagnostics);
            }
            drop(runtime);
            if let Some(diagnostics) = &invalid_state_root_diagnostics {
                warn!(
                    block_hash = %diagnostics.block_hash,
                    height = diagnostics.height,
                    parents = ?diagnostics.parent_hashes,
                    supplied_state_root = %diagnostics.supplied_state_root,
                    computed_state_root = %diagnostics.computed_state_root,
                    tx_count = diagnostics.tx_count,
                    coinbase_miner = ?diagnostics.coinbase_miner_address,
                    selected_tip = ?diagnostics.selected_tip,
                    selected_tip_height = ?diagnostics.selected_tip_height,
                    current_tips = ?diagnostics.current_tips,
                    stale_template = diagnostics.stale_template,
                    unknown_context = diagnostics.unknown_context,
                    classification = diagnostics.classification.as_str(),
                    "mining submit rejected with invalid state root"
                );
            }
            warn!(block_hash = %block_hash, height, outcome = ?outcome, "mining submit rejected");
            let (rejection_kind, reason_code) = match &outcome {
                pulsedag_core::BlockAcceptanceResult::Duplicate => {
                    (ExternalMiningRejectKind::DuplicateBlock, "duplicate_block")
                }
                pulsedag_core::BlockAcceptanceResult::InvalidPow => {
                    (ExternalMiningRejectKind::InvalidPow, "invalid_pow")
                }
                pulsedag_core::BlockAcceptanceResult::MissingParent => {
                    (ExternalMiningRejectKind::MissingParent, "missing_parent")
                }
                pulsedag_core::BlockAcceptanceResult::InvalidTransaction => (
                    ExternalMiningRejectKind::InvalidMerkleOrPayload,
                    "invalid_merkle_or_payload",
                ),
                pulsedag_core::BlockAcceptanceResult::Malformed => (
                    ExternalMiningRejectKind::MalformedSerialization,
                    "malformed_serialization",
                ),
                pulsedag_core::BlockAcceptanceResult::Rejected(message) => {
                    let (reason_code, kind) = classify_rejected_validation_message(message);
                    (kind, reason_code)
                }
                pulsedag_core::BlockAcceptanceResult::Accepted => {
                    (ExternalMiningRejectKind::InternalError, "internal_error")
                }
            };
            let detail = format!("block acceptance outcome: {:?}", outcome);
            record_external_mining_rejection(&state, rejection_kind, &detail).await;
            record_submit_completed(&state, submit_started, "rejected").await;
            Json(rejection_submit_response_with_pow(
                reason_code,
                detail,
                Some(block_hash),
                Some(height),
                false,
                target_u64,
                pow.target_hex.clone(),
                pow.hash_hex.clone(),
                pow_hash_score_u64,
                pow.rejection_code.map(|v| v.to_string()),
            ))
        }
    }
}

async fn perform_orphan_adoption<S: RpcStateLike>(state: &S, block_hash: &str) -> usize {
    let chain_handle = state.chain();
    let mut chain = match timeout(
        SUBMIT_ORPHAN_ADOPTION_CHAIN_WRITE_TIMEOUT,
        chain_handle.write(),
    )
    .await
    {
        Ok(chain) => chain,
        Err(_) => {
            warn!(
                block_hash = %block_hash,
                "skipping mining orphan adoption because chain write lock was busy"
            );
            let _ = state.storage().append_runtime_event(
                "warn",
                "external_mining_orphan_adoption_skipped_busy_chain",
                &format!(
                    "block_hash={} lock_timeout_ms={}",
                    block_hash,
                    SUBMIT_ORPHAN_ADOPTION_CHAIN_WRITE_TIMEOUT.as_millis()
                ),
            );
            return 0;
        }
    };

    let mut adopted_chain = chain.clone();
    let adopted = adopt_ready_orphans(&mut adopted_chain, AcceptSource::Rpc);
    if adopted > 0 {
        match state.storage().persist_chain_state(&adopted_chain) {
            Ok(()) => {
                *chain = adopted_chain;
                adopted
            }
            Err(e) => {
                warn!(error = %e, block_hash = %block_hash, adopted, "failed persisting chain state after mining orphan adoption; keeping orphans queued in memory");
                let _ = state.storage().append_runtime_event(
                    "warn",
                    "external_mining_orphan_adoption_persist_failed",
                    &format!(
                        "block_hash={} adopted_orphans={} error={}",
                        block_hash, adopted, e
                    ),
                );
                0
            }
        }
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        persist_then_broadcast_mined_block, post_mining_submit_with_actor, record_actor_timeout,
        record_submit_completed, record_submit_inflight, record_submit_phase,
        record_submit_started, MiningSubmitActorHandle, MiningSubmitData, MiningSubmitState,
        SubmitBlockCommand, SubmitBlockResponse,
    };
    use crate::{
        api::{
            ApiResponse, GetBlockTemplateRequest, NodeRuntimeStats, RpcStateLike,
            SubmitMinedBlockRequest,
        },
        handlers::mining_template::{post_mining_template, store_template, StoredMiningTemplate},
    };
    use axum::{extract::State, Json};
    use pulsedag_core::{
        build_candidate_block, build_coinbase_transaction, consensus_difficulty_snapshot,
        dev_mine_header, dev_target_u64,
        errors::PulseError,
        preferred_tip_hash, refresh_block_consensus_ids_with_state,
        state::ChainState,
        types::{Block, Transaction},
        AcceptSource, BlockAcceptanceResult,
    };
    use pulsedag_p2p::{P2pHandle, P2pStatus};
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::{
        sync::{mpsc, oneshot, RwLock},
        time::timeout,
    };

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
                chain_id: "testnet-dev".into(),
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
                outbound_duplicates_suppressed: 0,
                inv_blocks_received: 0,
                inv_hashes_known: 0,
                inv_hashes_requested: 0,
                header_requests_received: 0,
                header_requests_sent: 0,
                headers_received: 0,
                headers_sent: 0,
                headers_announced: 0,
                dependency_fetches_scheduled: 0,
                parent_first_fetches: 0,
                relay_loop_prevented: 0,
                seen_cache_ttl_secs: 120,
                recovery_rebroadcast_ttl_secs: 8,
                max_inventory_length: 512,
                max_request_fanout: 64,
                tx_inbound_received: 0,
                tx_inbound_accepted: 0,
                tx_inbound_duplicate: 0,
                tx_inbound_invalid: 0,
                tx_relayed: 0,
                tx_relay_suppressed_budget: 0,
                tx_relay_suppressed_duplicate: 0,
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
                peer_message_rate_limited_count: 0,
                peer_effective_count: 0,
                peer_min_target_missed_total: 0,
                peer_cooldown_bypassed_for_connectivity_total: 0,
                peer_rate_limit_recovery_suppressed_total: 0,
                peer_rate_limit_by_kind_total: Default::default(),
                peer_suppressed_dial_count: 0,
                peers_under_cooldown: 0,
                peers_under_flap_guard: 0,
                peer_lifecycle_healthy: 0,
                peer_lifecycle_watch: 0,
                peer_lifecycle_degraded: 0,
                peer_lifecycle_cooldown: 0,
                peer_lifecycle_recovering: 0,
                peer_retention_active_total: 0,
                peer_retention_recovering_total: 0,
                peer_retention_cooldown_total: 0,
                peer_sync_eligible_total: 0,
                peer_sync_suppressed_total: 0,
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
                blocks_requested: 0,
                blocks_received: 0,
                invalid_blocks_received: 0,
                orphan_blocks_received: 0,
                duplicate_blocks_received: 0,
                peer_penalties: 0,
                active_connections_by_peer: std::collections::HashMap::new(),
                active_connection_total: 0,
                last_connection_established_peer: None,
                last_connection_closed_peer: None,
                last_connection_closed_remaining_count: None,
                last_outgoing_connection_error_peer: None,
                last_incoming_connection_error_peer: None,
                last_dial_error: None,
                last_disconnect_reason: None,
                last_peer_state_transition: None,
                bootstrap_dial_attempts: 0,
                bootstrap_dial_successes: 0,
                bootstrap_dial_failures: 0,
                bootstrap_connected_peer_ids: vec![],
                bootnodes_configured: Vec::new(),
                bootnodes_connected: Vec::new(),
                pending_bootnode_dials: Vec::new(),
                bootnode_redial_attempts: 0,
                bootnode_redial_successes: 0,
                bootnode_redial_failures: 0,
                bootnode_reconnect_scheduled_total: 0,
                bootnode_reconnect_skipped_cooldown_total: 0,
                bootnode_reconnect_forced_from_cooldown_total: 0,
                bootnode_reconnect_success_total: 0,
                isolated_bootnode_reconnect_active: false,
                peer_zero_count_duration_seconds: 0,
                peer_zero_reconnect_attempt_total: 0,
                peer_zero_reconnect_success_total: 0,
                peer_reconnect_suppressed_by_cooldown_total: 0,
                peer_reconnect_suppressed_by_rate_limit_total: 0,
                peer_min_target_recovered_total: 0,
                last_peer_reconnect_blocked_reason: None,
                bootnode_next_redial_at: std::collections::HashMap::new(),
                bootnode_redial_backoff_secs: std::collections::HashMap::new(),
                last_bootnode_dial_error: None,
                gossipsub_peer_count: 0,
                subscribed_topics: Vec::new(),
                connection_established_total: 0,
                connection_closed_total: 0,
                last_connection_closed_reason: None,
                disconnect_reason_counts: std::collections::HashMap::new(),
                peer_lifecycle_event_counters: std::collections::HashMap::new(),
                last_error_by_peer: std::collections::HashMap::new(),
                inbound_peer_final_state: Vec::new(),
                outbound_peer_final_state: Vec::new(),
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

    fn submit_command_for_state(
        state: &TestState,
        block: Block,
    ) -> (SubmitBlockCommand, oneshot::Receiver<SubmitBlockResponse>) {
        let (response, receiver) = oneshot::channel();
        (
            SubmitBlockCommand {
                state: MiningSubmitState::from_rpc_state(state),
                req: SubmitMinedBlockRequest {
                    template_id: Some("unit-test-template".to_string()),
                    block,
                },
                submit_started: Instant::now(),
                response,
            },
            receiver,
        )
    }

    async fn post_mining_submit_for_test(
        state: TestState,
        req: SubmitMinedBlockRequest,
    ) -> Json<ApiResponse<MiningSubmitData>> {
        post_mining_submit_with_actor(state, req, MiningSubmitActorHandle::spawn(16)).await
    }

    async fn build_mined_block(state: &TestState) -> Block {
        let chain = state.chain.read().await;
        let height = chain.dag.best_height + 1;
        let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        let difficulty = consensus_difficulty_snapshot(&chain).expected_difficulty;
        let txs = vec![build_coinbase_transaction("kaspa:qptestminer", 50, height)];
        let mut block = build_candidate_block(parents, height, difficulty, txs);
        refresh_block_consensus_ids_with_state(&mut block, &chain).unwrap();
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
    async fn mining_submit_actor_queue_full_returns_submit_busy_path() {
        let (state, _) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;
        let (sender, _receiver) = mpsc::channel(1);
        let actor = MiningSubmitActorHandle { sender };
        let (first, _first_rx) = submit_command_for_state(&state, block.clone());
        assert!(
            actor.try_submit(first).is_ok(),
            "first command should queue"
        );
        let (second, _second_rx) = submit_command_for_state(&state, block);
        let err = actor
            .try_submit(second)
            .expect_err("second command should find bounded queue full");
        assert!(matches!(err, mpsc::error::TrySendError::Full(_)));
        assert_eq!(actor.queue_len(), 1);
    }

    #[tokio::test]
    async fn mining_submit_actor_timeout_records_bounded_error_metric() {
        let (state, _) = build_state_with_fake_p2p();
        let before = state
            .runtime
            .read()
            .await
            .external_mining_submit_actor_timeout_total;
        record_actor_timeout(&state, "unit-test timeout").await;
        let runtime = state.runtime.read().await;
        assert_eq!(
            runtime.external_mining_submit_actor_timeout_total,
            before + 1
        );
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("submit_timeout")
        );
    }

    #[tokio::test]
    async fn handler_queue_full_returns_submit_busy_schema() {
        let (state, _) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;
        let (sender, _receiver) = mpsc::channel(1);
        let actor = MiningSubmitActorHandle { sender };
        let (queued, _queued_rx) = submit_command_for_state(&state, block.clone());
        assert!(
            actor.try_submit(queued).is_ok(),
            "first command should queue"
        );

        let Json(response) = post_mining_submit_with_actor(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some("unit-test-template".to_string()),
                block,
            },
            actor,
        )
        .await;

        assert!(response.ok);
        let data = response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "submit_busy");
        assert!(data.reason.contains("bounded submit actor queue is full"));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_submit_actor_queue_full_total, 1);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
    }

    #[tokio::test]
    async fn handler_actor_response_closed_returns_submit_timeout_schema() {
        let (state, _) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;
        let (sender, mut receiver) = mpsc::channel(1);
        let actor = MiningSubmitActorHandle { sender };
        tokio::spawn(async move {
            let _command = receiver.recv().await.expect("command expected");
        });

        let Json(response) = post_mining_submit_with_actor(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some("unit-test-template".to_string()),
                block,
            },
            actor,
        )
        .await;

        assert!(response.ok);
        let data = response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "submit_timeout");
        assert!(data
            .pow_rejection_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("actor stopped")));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_submit_actor_timeout_total, 1);
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("submit_timeout")
        );
    }

    #[tokio::test]
    async fn handler_does_not_hold_chain_write_lock_while_waiting_for_actor_response() {
        let (state, _) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;
        let (sender, mut receiver) = mpsc::channel(1);
        let actor = MiningSubmitActorHandle { sender };
        let (got_command_tx, got_command_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let command = receiver.recv().await.expect("command expected");
            let _ = got_command_tx.send(());
            let _ = release_rx.await;
            drop(command);
        });

        let state_for_submit = state.clone();
        let submit_task = tokio::spawn(async move {
            post_mining_submit_with_actor(
                state_for_submit,
                SubmitMinedBlockRequest {
                    template_id: Some("unit-test-template".to_string()),
                    block,
                },
                actor,
            )
            .await
        });

        timeout(Duration::from_secs(1), got_command_rx)
            .await
            .expect("handler should enqueue command deterministically")
            .expect("actor should signal command receipt");
        let chain_write = state.chain.try_write();
        assert!(
            chain_write.is_ok(),
            "handler must not hold the chain write lock while awaiting actor response"
        );
        drop(chain_write);
        let _ = release_tx.send(());

        let Json(response) = submit_task.await.expect("submit task should finish");
        assert!(response.ok);
        let data = response.data.expect("submit data expected");
        assert_eq!(data.reason_code, "submit_timeout");
    }

    #[tokio::test]
    async fn submit_metrics_phase_progression_records_bounded_fields() {
        let (state, _fake) = build_state_with_fake_p2p();
        let started = Instant::now();

        record_submit_started(&state).await;
        record_submit_phase(&state, "precheck").await;
        record_submit_inflight(&state).await;
        record_submit_phase(&state, "accepting").await;
        record_submit_completed(&state, started, "completed").await;

        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_submit_started_total, 1);
        assert_eq!(runtime.external_mining_submit_completed_total, 1);
        assert_eq!(runtime.external_mining_submit_inflight, 0);
        assert_eq!(
            runtime.external_mining_last_submit_phase.as_deref(),
            Some("completed")
        );
    }
    #[tokio::test]
    async fn accepted_block_broadcasts_to_p2p() {
        let (state, fake_p2p) = build_state_with_fake_p2p();
        let (template_id, block) = build_mined_template_block(&state).await;

        let Json(response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
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

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(data.accepted);
        assert_eq!(data.reason, "accepted");
        assert!(data.pow_hash.is_some());
        assert!(data.pow_accepted_dev);
        assert!(data.pow_hash_score_u64 <= data.target_u64);
    }

    #[tokio::test]
    async fn submit_without_template_id_is_rejected() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: None,
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "missing_template_id");
        assert!(data.stale_template);
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("missing_template_id")
        );
    }

    #[tokio::test]
    async fn template_includes_target_hex() {
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
        assert_eq!(template.target_hex.len(), 64);
    }

    #[tokio::test]
    async fn rejected_block_does_not_broadcast() {
        let (state, fake_p2p) = build_state_with_fake_p2p();
        let (template_id, block) = build_mined_template_block(&state).await;

        let Json(first_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id.clone()),
                block: block.clone(),
            },
        )
        .await;
        assert!(first_response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);

        let Json(second_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
        )
        .await;

        assert!(second_response.ok);
        let second_data = second_response.data.expect("submit data expected");
        assert!(!second_data.accepted);
        assert_eq!(second_data.reason_code, "duplicate_block");
        assert!(second_data.duplicate);
        assert_eq!(fake_p2p.block_calls(), 1);
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.accepted_mined_blocks, 1);
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_accepted, 1);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_rejected_duplicate_block, 1);
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

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template.template_id),
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "stale_template");
        let reason = data
            .pow_rejection_reason
            .expect("rejection reason expected");
        assert!(reason.contains("reason_code=template_mempool_changed"));
        assert!(reason.contains("mempool view changed"));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_stale_work_detected, 1);
    }

    #[tokio::test]
    async fn malformed_submit_is_rejected_cleanly() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (template_id, block) = build_mined_template_block(&state).await;
        let mut malformed = block.clone();
        malformed.header.parents.clear();
        malformed.header.difficulty = 1;
        let (mined_header, mined, _, _) = dev_mine_header(malformed.header.clone(), 100_000);
        assert!(mined);
        malformed.header = mined_header;

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block: malformed,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "stale_template");
        let reason = data
            .pow_rejection_reason
            .expect("rejection reason expected");
        assert!(reason.contains("reason_code=submitted_parents_mismatch"));
        assert!(reason.contains("parents"));
    }

    #[tokio::test]
    async fn invalid_pow_submit_returns_diagnostic() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (template_id, mut block) = build_mined_template_block(&state).await;
        // Use a compact difficulty with a zero mantissa so the target is exactly
        // zero. This makes nonce 0 deterministically invalid instead of relying
        // on a high compact value that can randomly admit roughly half of hashes.
        block.header.difficulty = 0x0100_0000;
        block.header.nonce = 0;

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "invalid_pow");
        assert!(data.invalid_pow);
        assert!(!data.stale);
        let reason = data
            .pow_rejection_reason
            .expect("rejection reason expected");
        assert!(reason.contains("score="));
        assert!(reason.contains("target="));
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
    async fn mining_submit_rejection_taxonomy_is_stable() {
        let stable_codes = [
            "accepted",
            "stale_template",
            "missing_parent",
            "invalid_pow",
            "invalid_timestamp",
            "invalid_coinbase",
            "invalid_merkle_or_payload",
            "malformed_serialization",
            "duplicate_block",
            "storage_rejected",
            "unknown_validation_error",
            "internal_error",
        ];

        assert_eq!(stable_codes[0], "accepted");
        assert!(stable_codes.contains(&"stale_template"));
        assert!(stable_codes.contains(&"invalid_pow"));
        assert!(stable_codes.contains(&"duplicate_block"));
        assert!(stable_codes.contains(&"malformed_serialization"));
    }

    #[tokio::test]
    async fn no_regression_in_mining_rpc_flow() {
        let (state, _fake_p2p) = build_state_with_fake_p2p();
        let (template_id, block) = build_mined_template_block(&state).await;

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
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
            protocol_version: 1,
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
            merkle_root: block.header.merkle_root.clone(),
        });

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "stale_template");
        let reason = data
            .pow_rejection_reason
            .expect("rejection reason expected");
        assert!(reason.contains("reason_code=template_expired"));
        assert!(reason.contains("freshness window elapsed"));
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

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "stale_template");
        let reason = data
            .pow_rejection_reason
            .expect("rejection reason expected");
        assert!(reason.contains("reason_code=submitted_transactions_mismatch"));
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
        let (template_id, mut block) = build_mined_template_block(&state).await;
        block.transactions.clear();

        let Json(submit_response) = post_mining_submit_for_test(
            state.clone(),
            SubmitMinedBlockRequest {
                template_id: Some(template_id),
                block,
            },
        )
        .await;

        assert!(submit_response.ok);
        let data = submit_response.data.expect("submit data expected");
        assert!(!data.accepted);
        assert_eq!(data.reason_code, "stale_template");
        let reason = data
            .pow_rejection_reason
            .expect("rejection reason expected");
        assert!(reason.contains("reason_code=submitted_transactions_mismatch"));
        let runtime = state.runtime.read().await;
        assert_eq!(runtime.rejected_mined_blocks, 0);
        assert_eq!(runtime.external_mining_submit_rejected, 1);
        assert_eq!(runtime.external_mining_rejected_stale_template, 1);
        assert_eq!(
            runtime.external_mining_last_rejection_kind.as_deref(),
            Some("stale_template")
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
        let block = build_mined_block(&state).await;
        {
            let mut chain = state.chain.write().await;
            assert_eq!(
                pulsedag_core::accept_block_with_result(
                    block,
                    &mut chain,
                    AcceptSource::LocalMining
                ),
                BlockAcceptanceResult::Accepted
            );
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
        let difficulty = consensus_difficulty_snapshot(&chain).expected_difficulty;
        let txs = vec![build_coinbase_transaction("kaspa:qptestminer", 50, height)];
        let mut block = build_candidate_block(parents, height, difficulty, txs);
        refresh_block_consensus_ids_with_state(&mut block, &chain).unwrap();
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
