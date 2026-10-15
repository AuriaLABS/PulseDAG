use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike},
    handlers::monetary_activation_guard::{
        ensure_legacy_mining_disabled_when_monetary_v3_active, MONETARY_V3_LEGACY_MINING_DISABLED,
        MONETARY_V3_MINING_INACTIVE,
    },
    handlers::pow_metrics::PowMetricsData,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    build_activated_v2_mining_template, build_monetary_mining_template_v3,
    consensus_difficulty_snapshot, derive_activated_v2_mining_parent_context,
    finalize_monetary_mining_template_v3, materialize_activated_v2_mining_overlay,
    materialize_activated_v2_mining_overlay_with_materializer,
    materialize_authoritative_pre_candidate_state_v3, materialize_authoritative_state_v3,
    ActivatedV2MiningTemplateSpec, ChainState, PowValidationPath, ProtocolActivationIdentity,
    ProtocolMonetaryActivationRecordV2, PulseError, TRANSACTION_VERSION_V2,
};
use pulsedag_p2p::mode_connected_peers_are_real_network;
use sha3::{Digest, Keccak256};

pub use super::mining_template_legacy::StoredMiningTemplate;
pub(crate) use super::mining_template_legacy::{
    current_template_state, load_template, store_template, template_freshness_window,
    template_id_matches_lifecycle, MINING_PROTOCOL_VERSION,
};

const PROTOCOL_V2_PRE_POW_VERSION: u8 = 0;
const PROTOCOL_V2_NONCE_OFFSET_NOT_APPLICABLE: usize = 0;

#[derive(Debug, serde::Serialize)]
pub struct MiningTemplateData {
    pub protocol_version: u32,
    pub mode: String,
    pub algorithm: String,
    pub pow_engine: String,
    pub miner_address: String,
    pub template_id: String,
    pub selected_tip: Option<String>,
    pub parent_tips: Vec<String>,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
    pub freshness_ttl_secs: u64,
    pub freshness_grace_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_identity: Option<ProtocolActivationIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_identity_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monetary_policy_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monetary_cadence_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monetary_binding_fingerprint: Option<String>,
    pub reward_settlement_deferred: bool,
    pub block: pulsedag_core::types::Block,
    pub target_u64: u64,
    pub target_hex: String,
    pub bits: u32,
    pub difficulty: u32,
    pub compact_target: u32,
    pub network_id: String,
    pub nonce_range: String,
    pub timestamp_min_unix: u64,
    pub timestamp_max_unix: u64,
    pub next_height: u64,
    pub blue_score: u64,
    pub mempool_tx_count: usize,
    pub metrics_hint: PowMetricsData,
    pub pow_preimage_hex: String,
    pub pre_pow_hash: String,
    pub pow_preimage_nonce_offset: usize,
    pub pow_header_preimage_version: u8,
    pub mutable_header_fields: Vec<String>,
    pub template_selected_parent: Option<String>,
    pub template_parent_count: usize,
    pub template_blue_score: u64,
    pub template_merge_set_size: usize,
    pub template_parallel_parents_enabled: bool,
    pub template_parallel_parent_exclusion_reasons: Vec<String>,
    pub duplicate_tx_filtered: u64,
    pub duplicate_tx_filtered_total: u64,
}

impl From<super::mining_template_legacy::MiningTemplateData> for MiningTemplateData {
    fn from(data: super::mining_template_legacy::MiningTemplateData) -> Self {
        Self {
            protocol_version: data.protocol_version,
            mode: data.mode,
            algorithm: data.algorithm,
            pow_engine: data.pow_engine,
            miner_address: data.miner_address,
            template_id: data.template_id,
            selected_tip: data.selected_tip,
            parent_tips: data.parent_tips,
            created_at_unix: data.created_at_unix,
            expires_at_unix: data.expires_at_unix,
            freshness_ttl_secs: data.freshness_ttl_secs,
            freshness_grace_secs: data.freshness_grace_secs,
            protocol_identity: None,
            protocol_identity_fingerprint: None,
            monetary_policy_fingerprint: None,
            monetary_cadence_fingerprint: None,
            monetary_binding_fingerprint: None,
            reward_settlement_deferred: false,
            block: data.block,
            target_u64: data.target_u64,
            target_hex: data.target_hex,
            bits: data.bits,
            difficulty: data.difficulty,
            compact_target: data.compact_target,
            network_id: data.network_id,
            nonce_range: data.nonce_range,
            timestamp_min_unix: data.timestamp_min_unix,
            timestamp_max_unix: data.timestamp_max_unix,
            next_height: data.next_height,
            blue_score: data.blue_score,
            mempool_tx_count: data.mempool_tx_count,
            metrics_hint: data.metrics_hint,
            pow_preimage_hex: data.pow_preimage_hex,
            pre_pow_hash: data.pre_pow_hash,
            pow_preimage_nonce_offset: data.pow_preimage_nonce_offset,
            pow_header_preimage_version: data.pow_header_preimage_version,
            mutable_header_fields: data.mutable_header_fields,
            template_selected_parent: data.template_selected_parent,
            template_parent_count: data.template_parent_count,
            template_blue_score: data.template_blue_score,
            template_merge_set_size: data.template_merge_set_size,
            template_parallel_parents_enabled: data.template_parallel_parents_enabled,
            template_parallel_parent_exclusion_reasons: data
                .template_parallel_parent_exclusion_reasons,
            duplicate_tx_filtered: data.duplicate_tx_filtered,
            duplicate_tx_filtered_total: data.duplicate_tx_filtered_total,
        }
    }
}

fn template_protocol_path(
    chain: &ChainState,
    local_identity: Option<&ProtocolActivationIdentity>,
) -> Result<PowValidationPath, PulseError> {
    match local_identity {
        Some(identity) => pulsedag_core::resolve_pow_validation_path(identity, chain),
        None => Ok(PowValidationPath::LegacyV1),
    }
}

/// Resolve a mining protocol only after refusing permanently inactive
/// monetary-v3 nonce work. A numeric header-v3 identity cannot be routed
/// through a v2-only resolver before the terminal sidecar error is returned.
fn template_protocol_path_with_monetary_guard(
    chain: &ChainState,
    local_identity: Option<&ProtocolActivationIdentity>,
    monetary_activation: Option<&ProtocolMonetaryActivationRecordV2>,
) -> Result<PowValidationPath, (&'static str, String)> {
    if let (Some(identity), Some(record)) = (local_identity, monetary_activation) {
        check_monetary_v3_mining_template_activation(identity, Some(record))
            .map_err(|(code, message)| (code, message.to_string()))?;
    }
    template_protocol_path(chain, local_identity).map_err(|error| {
        (
            "PROTOCOL_MISMATCH",
            format!("cannot select mining template protocol: {error}"),
        )
    })
}

/// A corrupt or unreadable monetary sidecar cannot be classified as absent:
/// treating that read failure as retryable can trap standalone miners in
/// endless nonce-template polling. Fail closed with a recognized terminal
/// monetary code BEFORE protocol-path selection or legacy fallback.
fn require_readable_monetary_sidecar(
    result: Result<Option<ProtocolMonetaryActivationRecordV2>, PulseError>,
) -> Result<Option<ProtocolMonetaryActivationRecordV2>, (&'static str, String)> {
    result.map_err(|error| {
        (
            MONETARY_V3_MINING_INACTIVE,
            format!(
                "cannot verify v3 monetary activation sidecar, so all seconds-based mining nonce work is inactive until operator repair; do not retry: {error}"
            ),
        )
    })
}

fn parent_confirmed_txids(chain: &ChainState, parents: &[String]) -> HashSet<String> {
    parents
        .iter()
        .filter_map(|parent| chain.dag.blocks.get(parent))
        .flat_map(|block| block.transactions.iter().skip(1).map(|tx| tx.txid.clone()))
        .collect()
}

fn protocol_ordered_transactions(
    chain: &ChainState,
    parents: &[String],
) -> (Vec<pulsedag_core::types::Transaction>, u64) {
    let parent_txids = parent_confirmed_txids(chain, parents);
    let mut duplicate_tx_filtered = 0_u64;
    let mut txs = chain
        .mempool
        .transactions
        .iter()
        .filter_map(|(txid, tx)| {
            if parent_txids.contains(txid) {
                duplicate_tx_filtered = duplicate_tx_filtered.saturating_add(1);
                None
            } else {
                Some((txid.clone(), tx.clone()))
            }
        })
        .collect::<HashMap<_, _>>();
    let mut remaining_parents = HashMap::<String, usize>::new();
    let mut children = HashMap::<String, Vec<String>>::new();

    for (txid, tx) in &txs {
        let mut parent_count = 0_usize;
        for input in &tx.inputs {
            if txs.contains_key(&input.previous_output.txid) {
                parent_count = parent_count.saturating_add(1);
                children
                    .entry(input.previous_output.txid.clone())
                    .or_default()
                    .push(txid.clone());
            }
        }
        remaining_parents.insert(txid.clone(), parent_count);
    }

    let mut ready = BTreeSet::<(u64, String)>::new();
    for (txid, count) in &remaining_parents {
        if *count == 0 {
            let first_seen = chain
                .mempool
                .first_seen
                .get(txid)
                .copied()
                .unwrap_or(u64::MAX);
            ready.insert((first_seen, txid.clone()));
        }
    }

    let mut ordered = Vec::with_capacity(txs.len());
    while let Some((_, txid)) = ready.pop_first() {
        let Some(tx) = txs.remove(&txid) else {
            continue;
        };
        ordered.push(tx);
        if let Some(kids) = children.get(&txid) {
            for child in kids {
                if let Some(parent_count) = remaining_parents.get_mut(child) {
                    *parent_count = parent_count.saturating_sub(1);
                    if *parent_count == 0 {
                        let first_seen = chain
                            .mempool
                            .first_seen
                            .get(child)
                            .copied()
                            .unwrap_or(u64::MAX);
                        ready.insert((first_seen, child.clone()));
                    }
                }
            }
        }
    }

    if !txs.is_empty() {
        let mut fallback = txs.into_values().collect::<Vec<_>>();
        fallback.sort_by_key(|tx| {
            (
                chain
                    .mempool
                    .first_seen
                    .get(&tx.txid)
                    .copied()
                    .unwrap_or(u64::MAX),
                tx.txid.clone(),
            )
        });
        ordered.extend(fallback);
    }

    (ordered, duplicate_tx_filtered)
}

async fn mining_template_unavailable_reason<S: RpcStateLike>(state: &S) -> Option<String> {
    {
        let runtime_handle = state.runtime();
        let runtime = runtime_handle.read().await;
        if matches!(
            runtime.sync_state.as_str(),
            "missing_parent" | "missing_parent_recovery" | "orphan_recovery"
        ) || runtime.pending_missing_parents > 0
            || runtime.orphan_backlog_waiting_missing_parent > 0
        {
            return Some(format!(
                "mining template unavailable while sync_state={} missing_parent/orphan recovery is active",
                runtime.sync_state
            ));
        }
        if runtime.sync_state == "degraded" || runtime.sync_pipeline.last_error.is_some() {
            return Some(format!(
                "mining template unavailable while readiness snapshot is degraded: sync_state={}",
                runtime.sync_state
            ));
        }
    }
    let status = state.p2p()?.status().ok()?;
    (status.runtime_started
        && mode_connected_peers_are_real_network(&status.mode)
        && status.connected_peers.is_empty()
        && (!status.bootnodes_configured.is_empty() || !status.listening.is_empty()))
    .then(|| {
        format!(
            "mining template unavailable while p2p is enabled with peer_count=0; diagnostics={}",
            status.asymmetric_connectivity_diagnostics.join("|")
        )
    })
}

fn activated_v2_template_data(
    chain: &ChainState,
    identity: &ProtocolActivationIdentity,
    monetary_activation: Option<&ProtocolMonetaryActivationRecordV2>,
    miner_address: String,
    created_at_unix: u64,
    duplicate_tx_filtered_total: u64,
) -> Result<(MiningTemplateData, u64), PulseError> {
    if let Some(record) = monetary_activation {
        return activated_monetary_v3_template_data(
            chain,
            identity,
            record,
            miner_address,
            created_at_unix,
            duplicate_tx_filtered_total,
        );
    }
    let parent_context = derive_activated_v2_mining_parent_context(chain, identity)?;
    let (transactions, duplicate_tx_filtered) =
        protocol_ordered_transactions(chain, &parent_context.parents);
    if transactions
        .iter()
        .any(|transaction| transaction.version != TRANSACTION_VERSION_V2)
    {
        return Err(PulseError::InvalidBlock(
            "activated-v2 mining template encountered a non-v2 mempool transaction".to_string(),
        ));
    }

    let template = build_activated_v2_mining_template(
        chain,
        identity,
        ActivatedV2MiningTemplateSpec {
            miner_address: miner_address.clone(),
            timestamp: created_at_unix,
            coinbase_nonce: created_at_unix,
            transactions,
        },
    )?;
    let expires_at_unix =
        created_at_unix.saturating_add(super::mining_template_legacy::TEMPLATE_TTL_SECS);
    let template_id = format!(
        "v2:{}:{}:{}",
        template.block.header.height, template.block.hash, template.protocol_identity_fingerprint
    );
    let pre_pow_bytes = hex::decode(&template.pre_pow_bytes_hex).map_err(|error| {
        PulseError::InvalidBlock(format!("activated-v2 pre-pow hex is invalid: {error}"))
    })?;
    let pre_pow_hash = hex::encode(Keccak256::digest(pre_pow_bytes));
    let snapshot = consensus_difficulty_snapshot(chain);
    let exclusion_reasons = template
        .parent_context
        .excluded_parallel_parents
        .iter()
        .map(|excluded| format!("{}:{:?}", excluded.hash, excluded.reason))
        .collect::<Vec<_>>();
    let parallel_parents_enabled = !template.parent_context.included_parallel_parents.is_empty();
    let mempool_tx_count = template.block.transactions.len().saturating_sub(1);
    let block = template.block;
    let header_difficulty = block.header.difficulty;
    let next_height = block.header.height;
    let blue_score = block.header.blue_score;
    let parent_tips = block.header.parents.clone();
    let selected_tip = Some(template.parent_context.selected_tip.clone());
    let selected_parent = Some(template.parent_context.selected_parent.clone());
    let template_parent_count = parent_tips.len();
    let template_merge_set_size = template.parent_context.merge_set.len();

    Ok((
        MiningTemplateData {
            protocol_version: MINING_PROTOCOL_VERSION,
            mode: "external-miner-template-v2".to_string(),
            algorithm: template.pow_algorithm,
            pow_engine: template.pow_engine,
            miner_address,
            template_id,
            selected_tip,
            parent_tips,
            created_at_unix,
            expires_at_unix,
            freshness_ttl_secs: super::mining_template_legacy::TEMPLATE_TTL_SECS,
            freshness_grace_secs: super::mining_template_legacy::TEMPLATE_FRESHNESS_GRACE_SECS,
            protocol_identity: Some(template.protocol_identity),
            protocol_identity_fingerprint: Some(template.protocol_identity_fingerprint),
            monetary_policy_fingerprint: None,
            monetary_cadence_fingerprint: None,
            monetary_binding_fingerprint: None,
            reward_settlement_deferred: false,
            block,
            target_u64: template.target_u64,
            target_hex: template.target_hex,
            bits: template.target_bits,
            difficulty: header_difficulty,
            compact_target: template.target_bits,
            network_id: chain.chain_id.clone(),
            nonce_range: "0..=18446744073709551615".to_string(),
            timestamp_min_unix: created_at_unix.saturating_sub(1),
            timestamp_max_unix: expires_at_unix
                .saturating_add(super::mining_template_legacy::TEMPLATE_FRESHNESS_GRACE_SECS),
            next_height,
            blue_score,
            mempool_tx_count,
            metrics_hint: PowMetricsData {
                algorithm: pulsedag_core::selected_pow_name().to_string(),
                best_height: chain.dag.best_height,
                window_size: snapshot.policy.window_size,
                observed_block_count: snapshot.observed_block_count,
                avg_block_interval_secs: snapshot.avg_block_interval_secs,
                suggested_difficulty: u64::from(header_difficulty),
                target_u64: template.target_u64,
                target_block_interval_secs: snapshot.target_block_interval_secs,
                retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
                notes: vec![
                    "Mining template uses explicit activated-v2 protocol identity".to_string(),
                ],
            },
            pow_preimage_hex: template.pre_pow_bytes_hex,
            pre_pow_hash,
            pow_preimage_nonce_offset: PROTOCOL_V2_NONCE_OFFSET_NOT_APPLICABLE,
            pow_header_preimage_version: PROTOCOL_V2_PRE_POW_VERSION,
            mutable_header_fields: vec!["nonce".to_string()],
            template_selected_parent: selected_parent,
            template_parent_count,
            template_blue_score: blue_score,
            template_merge_set_size,
            template_parallel_parents_enabled: parallel_parents_enabled,
            template_parallel_parent_exclusion_reasons: exclusion_reasons,
            duplicate_tx_filtered,
            duplicate_tx_filtered_total: duplicate_tx_filtered_total
                .saturating_add(duplicate_tx_filtered),
        },
        duplicate_tx_filtered,
    ))
}

/// Fail-closed v3 monetary RPC gate. Its retained monetary builder/finalizer
/// calls are audited for issuance authority, but their seconds-based Block
/// output can never be advertised as a native 500ms v3 mining template.
fn activated_monetary_v3_template_data(
    chain: &ChainState,
    identity: &ProtocolActivationIdentity,
    monetary_activation: &ProtocolMonetaryActivationRecordV2,
    miner_address: String,
    created_at_unix: u64,
    _duplicate_tx_filtered_total: u64,
) -> Result<(MiningTemplateData, u64), PulseError> {
    if monetary_activation.identity != *identity {
        return Err(PulseError::InvalidBlock(
            "v3 monetary activation identity does not match mining protocol identity".to_string(),
        ));
    }
    if monetary_activation.reward_finality_policy_version
        != pulsedag_core::REWARD_FINALITY_POLICY_VERSION_V3
    {
        return Err(PulseError::InvalidBlock(format!(
            "unsupported v3 reward-finality policy {}; implemented live policy is {}",
            monetary_activation.reward_finality_policy_version,
            pulsedag_core::REWARD_FINALITY_POLICY_VERSION_V3
        )));
    }
    if chain.contracts.config.enabled {
        return Err(PulseError::InvalidBlock(
            "v3.0.0 monetary mining requires smart-contract execution to remain inactive"
                .to_string(),
        ));
    }

    // A monetary-v3 activation record cannot authorize v2 seconds/60s
    // PoW as native 500ms mining work. Even if startup gates are bypassed,
    // reject the current seconds-bound protocol identity before emission.
    if identity.block_header_protocol_version != pulsedag_core::protocol::BLOCK_HEADER_VERSION_V3 {
        return Err(PulseError::InvalidBlock(
            "monetary-v3 mining template is inactive: native v3 nanosecond headers, 500ms selected-parent retarget and mined/P2P/replay/storage authority are not integrated; refusing seconds-based v2 PoW template".to_string(),
        ));
    }

    let parent_context = derive_activated_v2_mining_parent_context(chain, identity)?;
    let (transactions, _duplicate_tx_filtered) =
        protocol_ordered_transactions(chain, &parent_context.parents);
    if transactions
        .iter()
        .any(|transaction| transaction.version != TRANSACTION_VERSION_V2)
    {
        return Err(PulseError::InvalidBlock(
            "monetary-v3 mining template encountered a non-v2 mempool transaction".to_string(),
        ));
    }

    let template = build_monetary_mining_template_v3(
        chain,
        identity,
        &monetary_activation.monetary_cadence_segments,
        &miner_address,
        created_at_unix,
        created_at_unix,
        transactions,
    )?;
    let finalized = finalize_monetary_mining_template_v3(
        chain,
        identity,
        &monetary_activation.monetary_cadence_segments,
        &template,
    )?;
    // A claimed v3 identity alone cannot authorize a finalized v2 BlockHeader.
    // The current finalizer creates version-2 seconds-based headers and must
    // never supply nonce-search-ready RPC work.
    if finalized.block.header.version != pulsedag_core::protocol::BLOCK_HEADER_VERSION_V3 {
        return Err(PulseError::InvalidBlock(
            "monetary-v3 mining template refuses a finalized seconds-based v2 header".to_string(),
        ));
    }
    // Even a numeric version 3 in the old BlockHeader would remain a
    // seconds-based v2 data type, not a native nanosecond BlockHeaderV3.
    // Never fall through to canonical_pow_v2_adapter or issue v3 nonce
    // work from this RPC response type. A future native-v3 RPC must use a
    // distinct BlockEnvelopeV3/PoW schema and independent end-to-end tests.
    Err(PulseError::InvalidBlock(
        "monetary-v3 mining template remains inactive: legacy BlockHeader cannot represent native v3 nanosecond PoW work".to_string(),
    ))
}

/// Terminal, pre-availability v3 mining gate for the current seconds-based
/// RPC block schema. This must run before readiness/peer checks or replay.
fn check_monetary_v3_mining_template_activation(
    identity: &ProtocolActivationIdentity,
    monetary_activation: Option<&ProtocolMonetaryActivationRecordV2>,
) -> Result<(), (&'static str, &'static str)> {
    let Some(record) = monetary_activation else {
        return Ok(());
    };
    if record.identity != *identity {
        return Err((
            "PROTOCOL_MISMATCH",
            "v3 monetary activation identity does not match mining protocol identity",
        ));
    }
    // This RPC always returns the legacy seconds-based types::Block, even
    // if a supplied/persisted identity happens to carry numeric header-v3.
    // No version bit can make the response schema native-nanosecond work.
    // Reject before peer availability, expensive replay, or staged overlays.
    Err((
        MONETARY_V3_MINING_INACTIVE,
        "monetary-v3 mining template is inactive: the current RPC block schema is seconds-based regardless of the advertised header version; refusing native 500ms mining work",
    ))
}

async fn post_legacy_template<S: RpcStateLike>(
    state: S,
    req: GetBlockTemplateRequest,
    identity: ProtocolActivationIdentity,
) -> Json<ApiResponse<MiningTemplateData>> {
    let fingerprint = match identity.fingerprint() {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot bind mining template to protocol identity: {error}"),
            ));
        }
    };
    let response =
        super::mining_template_legacy::post_mining_template(State(state), Json(req)).await;
    let ApiResponse {
        ok,
        data,
        error,
        meta,
    } = response.0;
    let data = match data {
        Some(data) => {
            let internal_template_id = data.template_id.clone();
            if let Err(error) = super::mining_submit::bind_template_protocol(
                internal_template_id.clone(),
                identity.clone(),
                fingerprint.clone(),
            ) {
                return Json(ApiResponse::err(
                    "PROTOCOL_IDENTITY_ERROR",
                    format!("cannot store mining template protocol binding: {error}"),
                ));
            }
            let mut stored = match load_template(&internal_template_id) {
                Some(stored) => stored,
                None => {
                    return Json(ApiResponse::err(
                        "PROTOCOL_IDENTITY_ERROR",
                        "cannot reload issued mining template before protocol binding",
                    ));
                }
            };
            stored.protocol_identity_fingerprint = fingerprint.clone();
            store_template(&stored);
            let durable = load_template(&internal_template_id)
                .is_some_and(|stored| stored.protocol_identity_fingerprint == fingerprint);
            if !durable {
                return Json(ApiResponse::err(
                    "PROTOCOL_IDENTITY_ERROR",
                    "issued mining template protocol fingerprint was not durably persisted",
                ));
            }
            let mut data: MiningTemplateData = data.into();
            data.protocol_identity = Some(identity);
            data.protocol_identity_fingerprint = Some(fingerprint);
            Some(data)
        }
        None => None,
    };
    Json(ApiResponse {
        ok,
        data,
        error,
        meta,
    })
}

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<MiningTemplateData>> {
    let local_identity = match super::mining_submit_protocol::rpc_protocol_identity(&state) {
        Ok(identity) => identity,
        Err(error) => {
            return Json(ApiResponse::err(
                "PROTOCOL_IDENTITY_ERROR",
                format!("cannot resolve mining template protocol identity: {error}"),
            ));
        }
    };

    // Read the monetary sidecar BEFORE selecting the PoW protocol path.
    // A matched numeric header-v3 record is still permanently inactive under
    // this seconds-based RPC; v2-only path resolution must not mask the
    // terminal code with a retryable PROTOCOL_MISMATCH.
    let early_monetary_activation = if local_identity.is_some() {
        match require_readable_monetary_sidecar(
            state.storage().protocol_monetary_activation_record(),
        ) {
            Ok(record) => record,
            Err((code, message)) => {
                return Json(ApiResponse::err(code, message));
            }
        }
    } else {
        None
    };

    let path_and_legacy_identity = {
        let chain_handle = state.chain();
        let mut chain = chain_handle.write().await;
        pulsedag_core::normalize_production_mempool_resources_v1(&mut chain);
        let path = match template_protocol_path_with_monetary_guard(
            &chain,
            local_identity.as_ref(),
            early_monetary_activation.as_ref(),
        ) {
            Ok(path) => path,
            Err((code, message)) => {
                return Json(ApiResponse::err(code, message));
            }
        };
        let legacy_identity = (path == PowValidationPath::LegacyV1).then(|| {
            local_identity
                .clone()
                .unwrap_or_else(|| ProtocolActivationIdentity::legacy_from_state(&chain))
        });
        (path, legacy_identity)
    };

    match path_and_legacy_identity {
        (PowValidationPath::LegacyV1, Some(identity)) => {
            if let Err(error) = ensure_legacy_mining_disabled_when_monetary_v3_active(
                &state,
                "/mining/template legacy fallback",
            ) {
                return Json(ApiResponse::err(MONETARY_V3_LEGACY_MINING_DISABLED, error));
            }
            post_legacy_template(state, req, identity).await
        }
        (PowValidationPath::LegacyV1, None) => Json(ApiResponse::err(
            "PROTOCOL_IDENTITY_ERROR",
            "legacy mining template path did not resolve a legacy identity",
        )),
        (PowValidationPath::ActivatedV2, _) => {
            let identity = match local_identity {
                Some(identity) => identity,
                None => {
                    return Json(ApiResponse::err(
                        "PROTOCOL_IDENTITY_ERROR",
                        "activated-v2 mining template requires explicit local protocol identity",
                    ));
                }
            };
            // The terminal monetary-v3 sidecar check must precede readiness,
            // peer availability and any durable replay/snapshot materialization.
            // Otherwise missing-parent recovery or zero peers masks a permanent
            // v3 nonce-work denial with retryable MINING_TEMPLATE_UNAVAILABLE.
            let storage = state.storage();
            let monetary_activation = match require_readable_monetary_sidecar(
                storage.protocol_monetary_activation_record(),
            ) {
                Ok(record) => record,
                Err((code, message)) => {
                    return Json(ApiResponse::err(code, message));
                }
            };
            if let Err((code, message)) = check_monetary_v3_mining_template_activation(
                &identity,
                monetary_activation.as_ref(),
            ) {
                return Json(ApiResponse::err(code, message));
            }
            if let Some(reason) = mining_template_unavailable_reason(&state).await {
                return Json(ApiResponse::err("MINING_TEMPLATE_UNAVAILABLE", reason));
            }
            let created_at_unix = pulsedag_core::current_ts();
            let duplicate_tx_filtered_total = {
                let runtime_handle = state.runtime();
                let value = runtime_handle.read().await.duplicate_tx_filtered_total;
                value
            };
            let data = {
                let chain_handle = state.chain();
                let chain = chain_handle.read().await;
                if let Some(record) = monetary_activation.as_ref() {
                    if let Err(error) = pulsedag_core::validate_live_reward_settlement_v3(
                        &chain,
                        &record.monetary_cadence_segments,
                        &record.reward_finality_policy_version,
                    ) {
                        return Json(ApiResponse::err(
                            "MINING_TEMPLATE_ERROR",
                            format!("live v3 monetary snapshot is invalid: {error}"),
                        ));
                    }
                }

                let durable_runtime_snapshot = if let Some(record) = monetary_activation.as_ref() {
                    storage.load_monetary_v3_p2p_runtime_snapshot(
                        &identity,
                        &record.monetary_cadence_segments,
                        &record.reward_finality_policy_version,
                    )
                } else {
                    storage.load_activated_v2_p2p_runtime_snapshot(&identity)
                };
                let template_chain = match durable_runtime_snapshot {
                    Ok((durable_chain, runtime))
                        if durable_chain.chain_state_generation == chain.chain_state_generation
                            && durable_chain.dag.best_height == chain.dag.best_height
                            && pulsedag_core::preferred_tip_hash(&durable_chain)
                                == pulsedag_core::preferred_tip_hash(&chain) =>
                    {
                        if runtime.staging().is_empty() {
                            chain.clone()
                        } else {
                            let overlay = if let Some(record) = monetary_activation.as_ref() {
                                materialize_activated_v2_mining_overlay_with_materializer(
                                    &chain,
                                    runtime.staging(),
                                    &identity,
                                    &|prepared: &ChainState| {
                                        materialize_authoritative_state_v3(
                                            prepared,
                                            &record.monetary_cadence_segments,
                                        )
                                    },
                                    &|context: &ChainState, candidate_hash: &pulsedag_core::Hash| {
                                        materialize_authoritative_pre_candidate_state_v3(
                                            context,
                                            candidate_hash,
                                            &record.monetary_cadence_segments,
                                        )
                                    },
                                )
                            } else {
                                materialize_activated_v2_mining_overlay(
                                    &chain,
                                    runtime.staging(),
                                    &identity,
                                )
                            };
                            match overlay {
                                Ok(overlay) => overlay,
                                Err(error) => {
                                    return Json(ApiResponse::err(
                                        "MINING_TEMPLATE_ERROR",
                                        format!(
                                            "cannot build activated-v2 staged mining overlay: {error}"
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                    Ok(_) => {
                        return Json(ApiResponse::err(
                            "MINING_TEMPLATE_ERROR",
                            "activated-v2 durable runtime snapshot does not match the current in-memory chain state",
                        ));
                    }
                    Err(error) => {
                        return Json(ApiResponse::err(
                            "MINING_TEMPLATE_ERROR",
                            format!(
                                "activated-v2 runtime sidecar is unavailable or invalid: {error}"
                            ),
                        ));
                    }
                };

                match activated_v2_template_data(
                    &template_chain,
                    &identity,
                    monetary_activation.as_ref(),
                    req.miner_address.clone(),
                    created_at_unix,
                    duplicate_tx_filtered_total,
                ) {
                    Ok((data, duplicate_tx_filtered)) => (data, duplicate_tx_filtered),
                    Err(error) => {
                        // A monetary sidecar must never produce retryable
                        // nonce work errors while the RPC block type is v2.
                        let code = if monetary_activation.is_some() {
                            MONETARY_V3_MINING_INACTIVE
                        } else {
                            "MINING_TEMPLATE_ERROR"
                        };
                        return Json(ApiResponse::err(
                            code,
                            format!("cannot build mining template: {error}"),
                        ));
                    }
                }
            };
            let (data, duplicate_tx_filtered) = data;
            let fingerprint = data
                .protocol_identity_fingerprint
                .clone()
                .expect("activated-v2 template must carry a fingerprint");
            if let Err(error) = super::mining_submit::bind_template_protocol(
                data.template_id.clone(),
                identity,
                fingerprint,
            ) {
                return Json(ApiResponse::err(
                    "PROTOCOL_IDENTITY_ERROR",
                    format!("cannot store mining template protocol binding: {error}"),
                ));
            }

            {
                let runtime_handle = state.runtime();
                let mut runtime = runtime_handle.write().await;
                runtime.external_mining_templates_emitted =
                    runtime.external_mining_templates_emitted.saturating_add(1);
                runtime.template_selected_parent = data.template_selected_parent.clone();
                runtime.template_parent_count = data.template_parent_count as u64;
                runtime.template_blue_score = data.template_blue_score;
                runtime.template_merge_set_size = data.template_merge_set_size as u64;
                runtime.template_parallel_parents_enabled = data.template_parallel_parents_enabled;
                runtime.template_parallel_parent_exclusion_reasons =
                    data.template_parallel_parent_exclusion_reasons.clone();
                runtime.duplicate_tx_filtered_total = runtime
                    .duplicate_tx_filtered_total
                    .saturating_add(duplicate_tx_filtered);
                if runtime
                    .external_mining_last_template_id
                    .as_ref()
                    .is_some_and(|last| last != &data.template_id)
                {
                    runtime.external_mining_templates_invalidated = runtime
                        .external_mining_templates_invalidated
                        .saturating_add(1);
                    runtime.external_mining_stale_work_detected = runtime
                        .external_mining_stale_work_detected
                        .saturating_add(1);
                }
                runtime.external_mining_last_template_id = Some(data.template_id.clone());
                runtime.pulsedag_mining_templates_total =
                    runtime.pulsedag_mining_templates_total.saturating_add(1);
            }
            let _ = state.storage().append_runtime_event(
                "info",
                "external_mining_template_emitted_v2",
                &format!(
                    "template_id={} height={} expires_at_unix={} miner={} protocol_identity_fingerprint={}",
                    data.template_id,
                    data.next_height,
                    data.expires_at_unix,
                    data.miner_address,
                    data.protocol_identity_fingerprint.as_deref().unwrap_or("-")
                ),
            );
            Json(ApiResponse::ok(data))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, genesis_v3::init_chain_state_v3, MonetaryCadenceSegment,
        ProtocolMonetaryActivationRecordV2, BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2,
        GHOSTDAG_V1_ORDERING_VERSION,
    };

    #[test]
    fn malformed_monetary_sidecar_is_terminal_not_retryable() {
        let (code, message) = require_readable_monetary_sidecar(Err(PulseError::StorageError(
            "malformed on-disk activation record".to_string(),
        )))
        .expect_err("sidecar read failure must terminate both v2 and legacy nonce requests");
        assert_eq!(code, MONETARY_V3_MINING_INACTIVE);
        assert!(message.contains("malformed on-disk activation record"));
        assert!(message.contains("do not retry"));

        // A successful initial read must not mask corruption discovered by
        // the activated-v2 route's second read (concurrent sidecar change).
        let first_read = require_readable_monetary_sidecar(Ok(None)).unwrap();
        assert!(first_read.is_none());
        let (second_code, second_message) =
            require_readable_monetary_sidecar(Err(PulseError::StorageError(
                "second read became unreadable".to_string(),
            )))
        .expect_err("activated-v2 reread failure must also terminate the miner");
        assert_eq!(second_code, MONETARY_V3_MINING_INACTIVE);
        assert!(second_message.contains("second read became unreadable"));
    }

    #[test]
    fn default_and_explicit_legacy_select_v1() {
        let state = init_chain_state("task28-rpc-template-legacy".to_string());
        assert_eq!(
            template_protocol_path(&state, None).unwrap(),
            PowValidationPath::LegacyV1
        );
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        assert_eq!(
            template_protocol_path(&state, Some(&legacy)).unwrap(),
            PowValidationPath::LegacyV1
        );
        assert_eq!(
            legacy.block_header_protocol_version,
            BLOCK_HEADER_VERSION_V1
        );
    }

    #[test]
    fn activated_identity_builds_chain_bound_v2_rpc_template() {
        let state = init_chain_state("task28-rpc-template-v2".to_string());
        let activated = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        assert_eq!(
            template_protocol_path(&state, Some(&activated)).unwrap(),
            PowValidationPath::ActivatedV2
        );

        let (data, duplicate_filtered) = activated_v2_template_data(
            &state,
            &activated,
            None,
            "pulse1task28rpcminer".to_string(),
            pulsedag_core::current_ts(),
            0,
        )
        .unwrap();

        assert_eq!(duplicate_filtered, 0);
        assert_eq!(data.block.header.version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(data.protocol_identity.as_ref(), Some(&activated));
        let fingerprint = activated.fingerprint().unwrap();
        assert_eq!(
            data.protocol_identity_fingerprint.as_deref(),
            Some(fingerprint.as_str())
        );
        assert_eq!(data.compact_target, data.block.header.difficulty);
        assert_eq!(data.target_u64, data.metrics_hint.target_u64);
        assert_eq!(data.parent_tips, data.block.header.parents);
        assert_eq!(data.template_selected_parent, data.selected_tip);
        assert_eq!(data.mempool_tx_count, 0);
    }

    #[test]
    fn monetary_sidecar_denial_is_terminal_before_peer_or_sync_availability() {
        let chain = init_chain_state_v3(
            "task1045-rpc-terminal-preavailability".into(),
            1_800_000_000,
        )
        .unwrap();
        let identity = ProtocolActivationIdentity::activated_v2(
            chain.chain_id.clone(),
            chain.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let cadence = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 500_000_000,
        }];
        let record = ProtocolMonetaryActivationRecordV2::from_identity_and_cadence(
            identity.clone(),
            &cadence,
            pulsedag_core::REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap();

        // The same permanently disabled monetary sidecar must report the
        // terminal error regardless of a later readiness/peer precondition.
        // This helper runs BEFORE mining_template_unavailable_reason; it
        // cannot be masked by missing-parent recovery or zero peers.
        let (code, message) =
            check_monetary_v3_mining_template_activation(&identity, Some(&record))
                .expect_err("v2 monetary sidecar cannot produce native v3 nonce work");
        assert_eq!(code, MONETARY_V3_MINING_INACTIVE);
        assert!(message.contains("refusing native 500ms mining work"));
        assert!(check_monetary_v3_mining_template_activation(&identity, None).is_ok());

        // A future or spoofed numeric header-v3 identity cannot promote
        // the old seconds-based RPC response into native nanosecond work.
        // The early gate must remain terminal even before the independent
        // monetary-record/protocol-identity validator runs.
        let mut numeric_v3_identity = identity.clone();
        numeric_v3_identity.block_header_protocol_version =
            pulsedag_core::protocol::BLOCK_HEADER_VERSION_V3;
        let numeric_v3_record = ProtocolMonetaryActivationRecordV2::from_identity_and_cadence(
            numeric_v3_identity.clone(),
            &cadence,
            pulsedag_core::REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap();
        let (code, message) = check_monetary_v3_mining_template_activation(
            &numeric_v3_identity,
            Some(&numeric_v3_record),
        )
        .expect_err("numeric header-v3 must not bypass native mining inactivity");
        assert_eq!(code, MONETARY_V3_MINING_INACTIVE);
        assert!(message.contains("seconds-based"));
        assert!(message.contains("refusing native 500ms mining work"));
        assert!(check_monetary_v3_mining_template_activation(&numeric_v3_identity, None).is_ok());

        let wrong_identity = ProtocolActivationIdentity::activated_v2(
            "another-network",
            chain.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let (code, _) =
            check_monetary_v3_mining_template_activation(&wrong_identity, Some(&record))
                .expect_err("mismatched monetary sidecar must fail before availability");
        assert_eq!(code, "PROTOCOL_MISMATCH");
    }

    #[test]
    fn numeric_header_v3_sidecar_rejects_before_v2_only_path_resolution() {
        let chain = init_chain_state_v3(
            "task1045-numeric-v3-path-ordering".to_string(),
            1_800_000_000,
        )
        .unwrap();
        let mut identity = ProtocolActivationIdentity::activated_v2(
            chain.chain_id.clone(),
            chain.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        identity.block_header_protocol_version = pulsedag_core::protocol::BLOCK_HEADER_VERSION_V3;
        let cadence = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 500_000_000,
        }];
        let record = ProtocolMonetaryActivationRecordV2::from_identity_and_cadence(
            identity.clone(),
            &cadence,
            pulsedag_core::REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap();

        // Reproduce the actual old route: the v2-only resolver rejects the
        // numeric-v3 header before the late monetary guard runs.
        assert!(template_protocol_path(&chain, Some(&identity)).is_err());

        // The RPC now checks the sidecar FIRST and yields the typed terminal
        // error consumed by the standalone miner; no availability/replay.
        let (code, message) =
            template_protocol_path_with_monetary_guard(&chain, Some(&identity), Some(&record))
                .expect_err("sidecar must block before v2-only path routing");
        assert_eq!(code, MONETARY_V3_MINING_INACTIVE);
        assert!(message.contains("refusing native 500ms mining work"));

        let wrong_identity = ProtocolActivationIdentity::activated_v2(
            "another-network",
            chain.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let (code, _) = template_protocol_path_with_monetary_guard(
            &chain,
            Some(&wrong_identity),
            Some(&record),
        )
        .expect_err("mismatched sidecar identity must reject before path");
        assert_eq!(code, "PROTOCOL_MISMATCH");
    }

    #[test]
    fn monetary_activation_must_not_export_seconds_based_pow_as_v3() {
        let frozen_ts = pulsedag_core::current_ts().saturating_sub(10).max(1);
        let state =
            init_chain_state_v3("task1045-rpc-monetary-template".to_string(), frozen_ts).unwrap();
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let cadence = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 500_000_000,
        }];
        let record = ProtocolMonetaryActivationRecordV2::from_identity_and_cadence(
            identity.clone(),
            &cadence,
            pulsedag_core::REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap();
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);

        let error = activated_v2_template_data(
            &state,
            &identity,
            Some(&record),
            "pulse1task1045miner".to_string(),
            timestamp,
            0,
        )
        .expect_err("monetary-v3 must fail closed before exporting v2 PoW work");
        let message = error.to_string();
        assert!(message.contains("monetary-v3 mining template is inactive"));
        assert!(message.contains("refusing seconds-based v2 PoW template"));
        assert!(message.contains("500ms selected-parent retarget"));
    }

    #[test]
    fn mixed_protocol_identity_fails_before_template_build() {
        let state = init_chain_state("task28-rpc-template-mixed".to_string());
        let mut identity = ProtocolActivationIdentity::legacy_from_state(&state);
        identity.block_header_protocol_version = BLOCK_HEADER_VERSION_V2;

        assert!(template_protocol_path(&state, Some(&identity)).is_err());
    }

    #[test]
    fn v2_rpc_template_serializes_identity_and_fingerprint_together() {
        let state = init_chain_state("task28-rpc-template-json".to_string());
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let (data, _) = activated_v2_template_data(
            &state,
            &identity,
            None,
            "pulse1task28rpcminer".to_string(),
            pulsedag_core::current_ts(),
            0,
        )
        .unwrap();
        let value = serde_json::to_value(data).unwrap();

        assert!(value.get("protocol_identity").is_some());
        assert_eq!(
            value["protocol_identity_fingerprint"],
            identity.fingerprint().unwrap()
        );
        assert_eq!(value["block"]["header"]["version"], BLOCK_HEADER_VERSION_V2);
    }
}
