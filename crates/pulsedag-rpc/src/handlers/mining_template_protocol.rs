use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike},
    handlers::monetary_activation_guard::{
        ensure_legacy_mining_disabled_when_monetary_v3_active,
        MONETARY_V3_LEGACY_MINING_DISABLED,
    },
    handlers::pow_metrics::PowMetricsData,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    build_activated_v2_mining_template, build_monetary_mining_template_v3,
    consensus_difficulty_snapshot, derive_activated_v2_mining_parent_context,
    finalize_monetary_mining_template_v3, ActivatedV2MiningTemplateSpec, ChainState,
    PowValidationPath, ProtocolActivationIdentity, ProtocolMonetaryActivationRecordV2, PulseError,
    TRANSACTION_VERSION_V2,
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

fn activated_monetary_v3_template_data(
    chain: &ChainState,
    identity: &ProtocolActivationIdentity,
    monetary_activation: &ProtocolMonetaryActivationRecordV2,
    miner_address: String,
    created_at_unix: u64,
    duplicate_tx_filtered_total: u64,
) -> Result<(MiningTemplateData, u64), PulseError> {
    if monetary_activation.identity != *identity {
        return Err(PulseError::InvalidBlock(
            "v3 monetary activation identity does not match mining protocol identity".to_string(),
        ));
    }
    if monetary_activation.reward_finality_policy_version
        != pulsedag_core::GHOSTDAG_V1_FINALITY_POLICY_VERSION
    {
        return Err(PulseError::InvalidBlock(format!(
            "unsupported v3 reward-finality policy {}; implemented live policy is {}",
            monetary_activation.reward_finality_policy_version,
            pulsedag_core::GHOSTDAG_V1_FINALITY_POLICY_VERSION
        )));
    }
    pulsedag_core::validate_live_reward_settlement_v3(
        chain,
        &monetary_activation.monetary_cadence_segments,
        &monetary_activation.reward_finality_policy_version,
    )?;
    if chain.contracts.config.enabled {
        return Err(PulseError::InvalidBlock(
            "v3.0.0 monetary mining requires smart-contract execution to remain inactive".to_string(),
        ));
    }

    let parent_context = derive_activated_v2_mining_parent_context(chain, identity)?;
    let (transactions, duplicate_tx_filtered) =
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
    let block = finalized.block;
    let adapter = pulsedag_core::canonical_pow_v2_adapter();
    let material = adapter.pre_pow_material(&block.header, &identity.chain_id)?;
    let expires_at_unix =
        created_at_unix.saturating_add(super::mining_template_legacy::TEMPLATE_TTL_SECS);
    let template_id = format!(
        "v3m:{}:{}:{}",
        block.header.height, block.hash, monetary_activation.binding_fingerprint
    );
    let pre_pow_hash = hex::encode(Keccak256::digest(&material.pre_pow_bytes));
    let snapshot = consensus_difficulty_snapshot(chain);
    let exclusion_reasons = parent_context
        .excluded_parallel_parents
        .iter()
        .map(|excluded| format!("{}:{:?}", excluded.hash, excluded.reason))
        .collect::<Vec<_>>();
    let parallel_parents_enabled = !parent_context.included_parallel_parents.is_empty();
    let mempool_tx_count = block.transactions.len().saturating_sub(1);
    let header_difficulty = block.header.difficulty;
    let next_height = block.header.height;
    let blue_score = block.header.blue_score;
    let parent_tips = block.header.parents.clone();
    let selected_tip = Some(parent_context.selected_tip.clone());
    let selected_parent = Some(parent_context.selected_parent.clone());
    let template_parent_count = parent_tips.len();
    let template_merge_set_size = parent_context.merge_set.len();

    Ok((
        MiningTemplateData {
            protocol_version: MINING_PROTOCOL_VERSION,
            mode: "external-miner-template-v3-monetary".to_string(),
            algorithm: adapter.algorithm_name().to_string(),
            pow_engine: adapter.engine_name().to_string(),
            miner_address,
            template_id,
            selected_tip,
            parent_tips,
            created_at_unix,
            expires_at_unix,
            freshness_ttl_secs: super::mining_template_legacy::TEMPLATE_TTL_SECS,
            freshness_grace_secs: super::mining_template_legacy::TEMPLATE_FRESHNESS_GRACE_SECS,
            protocol_identity: Some(identity.clone()),
            protocol_identity_fingerprint: Some(
                identity
                    .fingerprint()
                    .map_err(|error| PulseError::InvalidBlock(error.to_string()))?,
            ),
            monetary_policy_fingerprint: Some(finalized.monetary_policy_fingerprint),
            monetary_cadence_fingerprint: Some(finalized.monetary_cadence_fingerprint),
            monetary_binding_fingerprint: Some(
                monetary_activation.binding_fingerprint.clone(),
            ),
            reward_settlement_deferred: finalized.reward_settlement_deferred,
            block,
            target_u64: material.target.target_u64,
            target_hex: material.target.target_hex,
            bits: material.target.bits,
            difficulty: header_difficulty,
            compact_target: material.target.bits,
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
                target_u64: material.target.target_u64,
                target_block_interval_secs: snapshot.target_block_interval_secs,
                retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
                notes: vec![
                    "Mining template is bound to the persisted v3 monetary activation".to_string(),
                    "Reward claim amount is zero until canonical ordered-DAG settlement".to_string(),
                ],
            },
            pow_preimage_hex: hex::encode(material.pre_pow_bytes),
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

    let path_and_legacy_identity = {
        let chain_handle = state.chain();
        let mut chain = chain_handle.write().await;
        pulsedag_core::normalize_production_mempool_resources_v1(&mut chain);
        let path = match template_protocol_path(&chain, local_identity.as_ref()) {
            Ok(path) => path,
            Err(error) => {
                return Json(ApiResponse::err(
                    "PROTOCOL_MISMATCH",
                    format!("cannot select mining template protocol: {error}"),
                ));
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
                return Json(ApiResponse::err(
                    MONETARY_V3_LEGACY_MINING_DISABLED,
                    error,
                ));
            }
            post_legacy_template(state, req, identity).await
        }
        (PowValidationPath::LegacyV1, None) => Json(ApiResponse::err(
            "PROTOCOL_IDENTITY_ERROR",
            "legacy mining template path did not resolve a legacy identity",
        )),
        (PowValidationPath::ActivatedV2, _) => {
            if let Some(reason) = mining_template_unavailable_reason(&state).await {
                return Json(ApiResponse::err("MINING_TEMPLATE_UNAVAILABLE", reason));
            }
            let identity = match local_identity {
                Some(identity) => identity,
                None => {
                    return Json(ApiResponse::err(
                        "PROTOCOL_IDENTITY_ERROR",
                        "activated-v2 mining template requires explicit local protocol identity",
                    ));
                }
            };
            let created_at_unix = pulsedag_core::current_ts();
            let duplicate_tx_filtered_total = {
                let runtime_handle = state.runtime();
                let value = runtime_handle.read().await.duplicate_tx_filtered_total;
                value
            };
            let data = {
                let chain_handle = state.chain();
                let chain = chain_handle.read().await;
                let monetary_activation = match state.storage().protocol_monetary_activation_record() {
                    Ok(record) => record,
                    Err(error) => {
                        return Json(ApiResponse::err(
                            "MINING_TEMPLATE_ERROR",
                            format!("cannot read v3 monetary activation sidecar: {error}"),
                        ));
                    }
                };
                if let Some(record) = monetary_activation.as_ref() {
                    if record.identity != identity {
                        return Json(ApiResponse::err(
                            "PROTOCOL_MISMATCH",
                            "v3 monetary activation identity does not match mining protocol identity",
                        ));
                    }
                }
                match activated_v2_template_data(
                    &chain,
                    &identity,
                    monetary_activation.as_ref(),
                    req.miner_address.clone(),
                    created_at_unix,
                    duplicate_tx_filtered_total,
                ) {
                    Ok((data, duplicate_tx_filtered)) => (data, duplicate_tx_filtered),
                    Err(error) => {
                        return Json(ApiResponse::err(
                            "MINING_TEMPLATE_ERROR",
                            format!("cannot build activated-v2 mining template: {error}"),
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
    fn monetary_activation_builds_amountless_claim_template() {
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
            target_interval_ns: 1_000_000_000,
        }];
        let record = ProtocolMonetaryActivationRecordV2::from_identity_and_cadence(
            identity.clone(),
            &cadence,
            pulsedag_core::GHOSTDAG_V1_FINALITY_POLICY_VERSION,
        )
        .unwrap();
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);

        let (data, duplicate_filtered) = activated_v2_template_data(
            &state,
            &identity,
            Some(&record),
            "pulse1task1045miner".to_string(),
            timestamp,
            0,
        )
        .unwrap();

        assert_eq!(duplicate_filtered, 0);
        assert_eq!(data.mode, "external-miner-template-v3-monetary");
        assert_eq!(data.block.transactions[0].outputs[0].amount, 0);
        assert!(data.reward_settlement_deferred);
        assert_eq!(
            data.monetary_policy_fingerprint.as_deref(),
            Some(pulsedag_core::MONETARY_POLICY_FINGERPRINT_V3)
        );
        assert_eq!(
            data.monetary_cadence_fingerprint.as_deref(),
            Some(record.monetary_cadence_fingerprint.as_str())
        );
        assert_eq!(
            data.monetary_binding_fingerprint.as_deref(),
            Some(record.binding_fingerprint.as_str())
        );
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
