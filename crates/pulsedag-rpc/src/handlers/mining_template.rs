use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike},
    handlers::pow_metrics::PowMetricsData,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    build_candidate_block, build_coinbase_transaction, consensus_difficulty_snapshot,
    pow::{target_from_bits, target_hex},
    pow_preimage_bytes, preferred_tip_hash, refresh_block_consensus_ids_with_state,
    state::ChainState,
};
use pulsedag_p2p::mode_connected_peers_are_real_network;
use sha3::{Digest, Keccak256};
use tracing::info;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct StoredMiningTemplate {
    #[serde(default = "default_mining_protocol_version")]
    pub protocol_version: u32,
    pub template_id: String,
    pub miner_address: String,
    pub selected_tip: Option<String>,
    pub parent_hashes: Vec<String>,
    pub height: u64,
    pub difficulty: u32,
    pub created_at_unix: u64,
    pub target_u64: u64,
    #[serde(default)]
    pub mempool_fingerprint: String,
    #[serde(default)]
    pub mempool_tx_count: usize,
    #[serde(default)]
    pub expires_at_unix: u64,
    #[serde(default)]
    pub template_txids: Vec<String>,
    #[serde(default)]
    pub merkle_root: String,
    #[serde(default)]
    pub template_selected_parent: Option<String>,
    #[serde(default)]
    pub template_parent_count: usize,
    #[serde(default)]
    pub template_blue_score: u64,
    #[serde(default)]
    pub template_merge_set_size: usize,
    #[serde(default)]
    pub template_parallel_parents_enabled: bool,
    #[serde(default)]
    pub template_parallel_parent_exclusion_reasons: Vec<String>,
    #[serde(default)]
    pub duplicate_tx_filtered: u64,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateLifecycleState {
    pub height: u64,
    pub parent_hashes: Vec<String>,
    pub selected_tip: Option<String>,
    pub difficulty: u32,
    pub target_u64: u64,
    pub mempool_fingerprint: String,
    pub mempool_tx_count: usize,
    pub duplicate_tx_filtered: u64,
    pub parallel_parents_enabled: bool,
    pub parallel_parent_exclusion_reasons: Vec<String>,
}

pub(crate) const MINING_PROTOCOL_VERSION: u32 = 1;

fn default_mining_protocol_version() -> u32 {
    MINING_PROTOCOL_VERSION
}
pub(crate) const TEMPLATE_TTL_SECS: u64 = 30;
pub(crate) const TEMPLATE_FRESHNESS_GRACE_SECS: u64 = 2;
const POW_NONCE_OFFSET: usize = 1 + 4;
static TEMPLATE_STORE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn template_freshness_window(
    created_at_unix: u64,
    expires_at_unix: u64,
) -> (u64, u64, u64) {
    let expiry = if expires_at_unix == 0 {
        created_at_unix.saturating_add(TEMPLATE_TTL_SECS)
    } else {
        expires_at_unix
    };
    let hard_expiry = expiry.saturating_add(TEMPLATE_FRESHNESS_GRACE_SECS);
    (created_at_unix, expiry, hard_expiry)
}

fn invalidation_reason_codes(
    previous: &StoredMiningTemplate,
    lifecycle: &TemplateLifecycleState,
    now_unix: u64,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if previous.height != lifecycle.height {
        reasons.push("height_advanced");
    }
    if previous.parent_hashes != lifecycle.parent_hashes {
        reasons.push("parent_set_changed");
    }
    if previous.selected_tip != lifecycle.selected_tip {
        reasons.push("selected_tip_changed");
    }
    if previous.difficulty != lifecycle.difficulty || previous.target_u64 != lifecycle.target_u64 {
        reasons.push("difficulty_or_target_changed");
    }
    if previous.mempool_fingerprint != lifecycle.mempool_fingerprint {
        reasons.push("mempool_fingerprint_changed");
    }
    let (_, _, hard_expiry) =
        template_freshness_window(previous.created_at_unix, previous.expires_at_unix);
    if now_unix > hard_expiry {
        reasons.push("freshness_window_elapsed");
    }
    if reasons.is_empty() {
        reasons.push("lifecycle_changed");
    }
    reasons
}

fn parent_confirmed_txids(chain: &ChainState, parents: &[String]) -> HashSet<String> {
    parents
        .iter()
        .filter_map(|parent| chain.dag.blocks.get(parent))
        .flat_map(|block| block.transactions.iter().skip(1).map(|tx| tx.txid.clone()))
        .collect()
}

fn template_ordered_transactions(
    chain: &ChainState,
    parents: &[String],
) -> (Vec<pulsedag_core::types::Transaction>, u64) {
    let parent_txids = parent_confirmed_txids(chain, parents);
    let mut duplicate_tx_filtered = 0u64;
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
        let mut parent_count = 0usize;
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
            let fee = txs.get(txid).map(|tx| tx.fee).unwrap_or(0);
            ready.insert((u64::MAX.saturating_sub(fee), txid.clone()));
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
                        let fee = txs.get(child).map(|tx| tx.fee).unwrap_or(0);
                        ready.insert((u64::MAX.saturating_sub(fee), child.clone()));
                    }
                }
            }
        }
    }

    if !txs.is_empty() {
        let mut fallback = txs.into_values().collect::<Vec<_>>();
        fallback.sort_by(|a, b| b.fee.cmp(&a.fee).then_with(|| a.txid.cmp(&b.txid)));
        ordered.extend(fallback);
    }

    (ordered, duplicate_tx_filtered)
}

fn experimental_parallel_parents_enabled() -> bool {
    std::env::var("PULSEDAG_EXPERIMENTAL_PARALLEL_PARENTS")
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn current_template_state(chain: &ChainState) -> TemplateLifecycleState {
    let height = chain.dag.best_height + 1;
    let selected_tip = preferred_tip_hash(chain);
    let experimental_parallel = experimental_parallel_parents_enabled();
    let ghostdag_dev = chain.dag.consensus_mode == pulsedag_core::ConsensusMode::GhostdagDev;
    let parallel_parents_enabled = ghostdag_dev && experimental_parallel;
    let mut parallel_parent_exclusion_reasons = Vec::new();
    if !ghostdag_dev {
        parallel_parent_exclusion_reasons.push("consensus_mode_not_ghostdag_dev".to_string());
    }
    if !experimental_parallel {
        parallel_parent_exclusion_reasons
            .push("experimental_parallel_parents_flag_disabled".to_string());
    }
    let mut parent_hashes = selected_tip.iter().cloned().collect::<Vec<_>>();
    if parallel_parents_enabled {
        for tip in pulsedag_core::sorted_tip_hashes(chain) {
            if selected_tip.as_ref() == Some(&tip) {
                continue;
            }
            if parent_hashes.len() >= chain.dag.merge_set_k.saturating_add(1) {
                parallel_parent_exclusion_reasons.push("merge_set_k_limit_reached".to_string());
                break;
            }
            parent_hashes.push(tip);
        }
    }
    let snapshot = consensus_difficulty_snapshot(chain);
    let difficulty = snapshot.expected_difficulty;
    let target_u64 = snapshot.expected_target_u64;
    let mut tx_ids = chain
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    tx_ids.sort();
    let duplicate_tx_filtered = parent_confirmed_txids(chain, &parent_hashes)
        .into_iter()
        .filter(|txid| chain.mempool.transactions.contains_key(txid))
        .count() as u64;
    let mempool_tx_count = tx_ids.len().saturating_sub(duplicate_tx_filtered as usize);
    let mempool_fingerprint = format!("{mempool_tx_count}:{}", tx_ids.join(","));

    TemplateLifecycleState {
        height,
        parent_hashes,
        selected_tip,
        difficulty,
        target_u64,
        mempool_fingerprint,
        mempool_tx_count,
        duplicate_tx_filtered,
        parallel_parents_enabled,
        parallel_parent_exclusion_reasons,
    }
}

pub(crate) fn template_id_for_state(state: &TemplateLifecycleState) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        state.height,
        state.parent_hashes.join(","),
        state
            .selected_tip
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        state.difficulty,
        state.target_u64,
        state.mempool_fingerprint
    )
}

pub(crate) fn store_template(record: &StoredMiningTemplate) {
    let dir = PathBuf::from("./data/mining_templates");
    let _ = fs::create_dir_all(&dir);
    let filename = format!("{}.json", sanitize(&record.template_id));
    let path = dir.join(&filename);
    let unique = TEMPLATE_STORE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = dir.join(format!(".{filename}.{}.{}.tmp", std::process::id(), unique));

    let bytes = serde_json::to_vec_pretty(record).unwrap_or_default();
    if fs::write(&tmp_path, bytes).is_ok() {
        if fs::rename(&tmp_path, &path).is_err() {
            let _ = fs::remove_file(&path);
            let _ = fs::rename(&tmp_path, &path);
        }
    } else {
        let _ = fs::remove_file(&tmp_path);
    }
}

pub(crate) fn load_template(template_id: &str) -> Option<StoredMiningTemplate> {
    let path =
        PathBuf::from("./data/mining_templates").join(format!("{}.json", sanitize(template_id)));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<StoredMiningTemplate>(&bytes).ok()
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
            return Some(format!("mining template unavailable while sync_state={} missing_parent/orphan recovery is active", runtime.sync_state));
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

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<MiningTemplateData>> {
    if let Some(reason) = mining_template_unavailable_reason(&state).await {
        return Json(ApiResponse::err("MINING_TEMPLATE_UNAVAILABLE", reason));
    }
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = consensus_difficulty_snapshot(&chain);
    let lifecycle = current_template_state(&chain);
    let height = lifecycle.height;
    let parents = lifecycle.parent_hashes.clone();
    let reward = 50;
    let template_id = template_id_for_state(&lifecycle);
    let selected_tip = lifecycle.selected_tip.clone();
    let created_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let expires_at_unix = created_at_unix + TEMPLATE_TTL_SECS;

    let mut txs = vec![build_coinbase_transaction(
        &req.miner_address,
        reward,
        height,
    )];
    let (ordered_txs, duplicate_tx_filtered) = template_ordered_transactions(&chain, &parents);
    txs.extend(ordered_txs);
    let header_difficulty = lifecycle.difficulty;
    let mut block = build_candidate_block(parents.clone(), height, header_difficulty, txs);
    let merge_classification = pulsedag_core::classify_merge_set(&block, &chain);
    block.header.blue_score = merge_classification.blue_score;
    if let Err(e) = refresh_block_consensus_ids_with_state(&mut block, &chain) {
        return Json(ApiResponse::err("STATE_ROOT_ERROR", e.to_string()));
    }
    let target_u64 = lifecycle.target_u64;
    let canonical_target_hex = target_hex(&target_from_bits(header_difficulty));
    let compact_target = header_difficulty;
    let template_txids = block
        .transactions
        .iter()
        .map(|tx| tx.txid.clone())
        .collect::<Vec<_>>();
    let pow_preimage = pow_preimage_bytes(&block.header);
    let pow_preimage_hex = hex::encode(&pow_preimage);
    let pre_pow_hash = hex::encode(Keccak256::digest(&pow_preimage));
    let parent_tips = block.header.parents.clone();
    let template_parent_count = parent_tips.len();
    let template_blue_score = block.header.blue_score;
    let template_merge_set_size = merge_classification.diagnostics.merge_set_size;

    store_template(&StoredMiningTemplate {
        protocol_version: MINING_PROTOCOL_VERSION,
        template_id: template_id.clone(),
        miner_address: req.miner_address.clone(),
        selected_tip: selected_tip.clone(),
        parent_hashes: parents,
        height,
        difficulty: header_difficulty,
        created_at_unix,
        target_u64,
        mempool_fingerprint: lifecycle.mempool_fingerprint.clone(),
        mempool_tx_count: lifecycle.mempool_tx_count,
        expires_at_unix,
        template_txids: template_txids.clone(),
        merkle_root: block.header.merkle_root.clone(),
        template_selected_parent: merge_classification.selected_parent.clone(),
        template_parent_count,
        template_blue_score,
        template_merge_set_size,
        template_parallel_parents_enabled: lifecycle.parallel_parents_enabled,
        template_parallel_parent_exclusion_reasons: lifecycle
            .parallel_parent_exclusion_reasons
            .clone(),
        duplicate_tx_filtered,
    });
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.external_mining_templates_emitted =
            runtime.external_mining_templates_emitted.saturating_add(1);
        runtime.template_selected_parent = merge_classification.selected_parent.clone();
        runtime.template_parent_count = template_parent_count as u64;
        runtime.template_blue_score = template_blue_score;
        runtime.template_merge_set_size = template_merge_set_size as u64;
        runtime.template_parallel_parents_enabled = lifecycle.parallel_parents_enabled;
        runtime.template_parallel_parent_exclusion_reasons =
            lifecycle.parallel_parent_exclusion_reasons.clone();
        runtime.duplicate_tx_filtered_total = runtime
            .duplicate_tx_filtered_total
            .saturating_add(duplicate_tx_filtered);
        if runtime
            .external_mining_last_template_id
            .as_ref()
            .is_some_and(|last| last != &template_id)
        {
            let previous_template_id = runtime
                .external_mining_last_template_id
                .clone()
                .unwrap_or_default();
            let reason_codes = load_template(&previous_template_id)
                .map(|stored| invalidation_reason_codes(&stored, &lifecycle, created_at_unix))
                .unwrap_or_else(|| vec!["previous_template_unavailable"]);
            runtime.external_mining_templates_invalidated = runtime
                .external_mining_templates_invalidated
                .saturating_add(1);
            runtime.external_mining_stale_work_detected = runtime
                .external_mining_stale_work_detected
                .saturating_add(1);
            let _ = state.storage().append_runtime_event(
                "warn",
                "external_mining_template_invalidated",
                &format!(
                    "previous={} current={} reason_codes={}",
                    previous_template_id,
                    template_id,
                    reason_codes.join(",")
                ),
            );
        }
        runtime.external_mining_last_template_id = Some(template_id.clone());
        runtime.pulsedag_mining_templates_total =
            runtime.pulsedag_mining_templates_total.saturating_add(1);
    }
    info!(template_id = %template_id, height, selected_tip = ?selected_tip, tx_count = template_txids.len(), "mining template created");
    let _ = state.storage().append_runtime_event(
        "info",
        "external_mining_template_emitted",
        &format!(
            "template_id={} height={} expires_at_unix={} miner={}",
            template_id, height, expires_at_unix, req.miner_address
        ),
    );

    let metrics_hint = PowMetricsData {
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        best_height: chain.dag.best_height,
        window_size: snapshot.policy.window_size,
        observed_block_count: snapshot.observed_block_count,
        avg_block_interval_secs: snapshot.avg_block_interval_secs,
        suggested_difficulty: u64::from(snapshot.expected_difficulty),
        target_u64,
        target_block_interval_secs: snapshot.target_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        notes: vec!["Mining template uses centralized runtime retarget policy".to_string()],
    };

    let blue_score = template_blue_score;

    Json(ApiResponse::ok(MiningTemplateData {
        protocol_version: MINING_PROTOCOL_VERSION,
        mode: "external-miner-template".to_string(),
        algorithm: "kHeavyHash".to_string(),
        pow_engine: "kaspa-kheavyhash".to_string(),
        miner_address: req.miner_address,
        template_id,
        selected_tip,
        parent_tips,
        created_at_unix,
        expires_at_unix,
        freshness_ttl_secs: TEMPLATE_TTL_SECS,
        freshness_grace_secs: TEMPLATE_FRESHNESS_GRACE_SECS,
        block,
        target_u64,
        target_hex: canonical_target_hex,
        bits: compact_target,
        difficulty: header_difficulty,
        compact_target,
        network_id: chain.chain_id.clone(),
        nonce_range: "0..=18446744073709551615".to_string(),
        timestamp_min_unix: created_at_unix.saturating_sub(1),
        timestamp_max_unix: expires_at_unix.saturating_add(TEMPLATE_FRESHNESS_GRACE_SECS),
        next_height: height,
        blue_score,
        mempool_tx_count: lifecycle.mempool_tx_count,
        metrics_hint,
        pow_preimage_hex,
        pre_pow_hash,
        pow_preimage_nonce_offset: POW_NONCE_OFFSET,
        pow_header_preimage_version: pulsedag_core::POW_HEADER_PREIMAGE_VERSION,
        mutable_header_fields: vec!["nonce".to_string()],
        template_selected_parent: merge_classification.selected_parent,
        template_parent_count,
        template_blue_score: blue_score,
        template_merge_set_size,
        template_parallel_parents_enabled: lifecycle.parallel_parents_enabled,
        template_parallel_parent_exclusion_reasons: lifecycle.parallel_parent_exclusion_reasons,
        duplicate_tx_filtered,
        duplicate_tx_filtered_total: {
            let runtime_handle = state.runtime();
            let duplicate_tx_filtered_total =
                runtime_handle.read().await.duplicate_tx_filtered_total;
            duplicate_tx_filtered_total
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        current_template_state, mining_template_unavailable_reason, template_freshness_window,
        template_id_for_state, template_ordered_transactions, TEMPLATE_FRESHNESS_GRACE_SECS,
        TEMPLATE_TTL_SECS,
    };
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use pulsedag_core::{
        genesis::init_chain_state,
        state::{ChainState, SelectedParentPolicy},
        types::{Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput},
        PulseError,
    };
    use pulsedag_p2p::{P2pHandle, P2pStatus, P2P_MODE_LIBP2P_REAL};
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
        p2p: Option<Arc<dyn P2pHandle>>,
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

    #[derive(Clone)]
    struct TestP2pHandle {
        status: P2pStatus,
    }

    impl P2pHandle for TestP2pHandle {
        fn broadcast_transaction(&self, _tx: &Transaction) -> Result<(), PulseError> {
            Ok(())
        }

        fn broadcast_block(&self, _block: &Block) -> Result<(), PulseError> {
            Ok(())
        }

        fn status(&self) -> Result<P2pStatus, PulseError> {
            Ok(self.status.clone())
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    fn test_state_with_status(status: P2pStatus) -> TestState {
        let path = temp_db_path("mining-template-connectivity");
        TestState {
            chain: Arc::new(RwLock::new(init_chain_state("testnet-dev".to_string()))),
            storage: Arc::new(Storage::open(path.to_str().unwrap()).unwrap()),
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
            p2p: Some(Arc::new(TestP2pHandle { status })),
        }
    }

    fn tip_block(hash: &str, height: u64, blue_score: u64, txs: Vec<Transaction>) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: vec!["genesis-block".to_string()],
                timestamp: height,
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("merkle-{hash}"),
                state_root: format!("state-{hash}"),
                blue_score,
                height,
            },
            transactions: txs,
        }
    }

    fn non_coinbase_tx(txid: &str, fee: u64) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: format!("utxo-{txid}"),
                    index: 0,
                },
                public_key: "pk".to_string(),
                signature: "sig".to_string(),
            }],
            outputs: vec![TxOutput {
                address: "kaspa:qptest".to_string(),
                amount: 1,
            }],
            fee,
            nonce: fee,
        }
    }

    #[test]
    fn parallel_parents_disabled_by_default() {
        let mut chain = init_chain_state("testnet-dev".to_string());
        chain.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
        chain.dag.tips.clear();
        chain.dag.blocks.insert(
            "lower-blue".to_string(),
            tip_block("lower-blue", 10, 10, vec![]),
        );
        chain.dag.blocks.insert(
            "selected-blue".to_string(),
            tip_block("selected-blue", 9, 20, vec![]),
        );
        chain.dag.tips.insert("lower-blue".to_string());
        chain.dag.tips.insert("selected-blue".to_string());

        let state = current_template_state(&chain);

        assert_eq!(state.selected_tip, Some("selected-blue".to_string()));
        assert_eq!(state.parent_hashes, vec!["selected-blue".to_string()]);
        assert!(!state.parallel_parents_enabled);
        assert!(state
            .parallel_parent_exclusion_reasons
            .contains(&"experimental_parallel_parents_flag_disabled".to_string()));
    }

    #[test]
    fn parallel_parents_require_ghostdag_dev_and_explicit_flag() {
        let mut chain = init_chain_state("testnet-dev".to_string());
        chain.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
        chain.dag.tips.clear();
        chain.dag.blocks.insert(
            "lower-blue".to_string(),
            tip_block("lower-blue", 10, 10, vec![]),
        );
        chain.dag.blocks.insert(
            "selected-blue".to_string(),
            tip_block("selected-blue", 9, 20, vec![]),
        );
        chain.dag.tips.insert("lower-blue".to_string());
        chain.dag.tips.insert("selected-blue".to_string());

        std::env::set_var("PULSEDAG_EXPERIMENTAL_PARALLEL_PARENTS", "true");
        let legacy = current_template_state(&chain);
        assert_eq!(legacy.parent_hashes, vec!["selected-blue".to_string()]);
        assert!(!legacy.parallel_parents_enabled);

        chain.dag.consensus_mode = pulsedag_core::ConsensusMode::GhostdagDev;
        let ghostdag = current_template_state(&chain);
        std::env::remove_var("PULSEDAG_EXPERIMENTAL_PARALLEL_PARENTS");
        assert!(ghostdag.parallel_parents_enabled);
        assert_eq!(ghostdag.parent_hashes[0], "selected-blue");
        assert!(ghostdag.parent_hashes.contains(&"lower-blue".to_string()));
    }

    #[test]
    fn template_filters_duplicate_transactions_already_in_parallel_parents() {
        let mut chain = init_chain_state("testnet-dev".to_string());
        chain.dag.tips.clear();
        let duplicate = non_coinbase_tx("duplicate-tx", 10);
        let fresh = non_coinbase_tx("fresh-tx", 9);
        chain.dag.blocks.insert(
            "selected-blue".to_string(),
            tip_block("selected-blue", 2, 2, vec![]),
        );
        chain.dag.blocks.insert(
            "parallel".to_string(),
            tip_block(
                "parallel",
                1,
                1,
                vec![
                    pulsedag_core::build_coinbase_transaction("kaspa:qptest", 1, 1),
                    duplicate.clone(),
                ],
            ),
        );
        chain.dag.tips.insert("selected-blue".to_string());
        chain.dag.tips.insert("parallel".to_string());
        chain.dag.consensus_mode = pulsedag_core::ConsensusMode::GhostdagDev;
        std::env::set_var("PULSEDAG_EXPERIMENTAL_PARALLEL_PARENTS", "true");
        chain
            .mempool
            .transactions
            .insert(duplicate.txid.clone(), duplicate);
        chain
            .mempool
            .transactions
            .insert(fresh.txid.clone(), fresh.clone());

        let state = current_template_state(&chain);
        std::env::remove_var("PULSEDAG_EXPERIMENTAL_PARALLEL_PARENTS");
        let (selected, filtered) = template_ordered_transactions(&chain, &state.parent_hashes);

        assert_eq!(filtered, 1);
        assert_eq!(state.duplicate_tx_filtered, 1);
        assert_eq!(
            selected.into_iter().map(|tx| tx.txid).collect::<Vec<_>>(),
            vec![fresh.txid]
        );
    }
    #[test]
    fn template_id_changes_when_mempool_changes() {
        let mut chain = init_chain_state("testnet-dev".to_string());
        let state_before = current_template_state(&chain);
        let before = template_id_for_state(&state_before);

        let tx = pulsedag_core::build_coinbase_transaction("kaspa:qptest", 1, 1);
        chain.mempool.transactions.insert(tx.txid.clone(), tx);
        let state_after = current_template_state(&chain);
        let after = template_id_for_state(&state_after);

        assert_ne!(before, after);
        assert_eq!(state_after.mempool_tx_count, 1);
    }

    #[test]
    fn template_ordering_keeps_parent_before_child_even_when_child_fee_is_higher() {
        let mut chain = init_chain_state("testnet-dev".to_string());
        let parent = Transaction {
            txid: "parent".to_string(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "utxo-parent".to_string(),
                    index: 0,
                },
                public_key: "pk".to_string(),
                signature: "sig".to_string(),
            }],
            outputs: vec![],
            fee: 1,
            nonce: 1,
        };
        let child = Transaction {
            txid: "child".to_string(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: parent.txid.clone(),
                    index: 0,
                },
                public_key: "pk".to_string(),
                signature: "sig".to_string(),
            }],
            outputs: vec![],
            fee: 100,
            nonce: 2,
        };

        chain
            .mempool
            .transactions
            .insert(child.txid.clone(), child.clone());
        chain
            .mempool
            .transactions
            .insert(parent.txid.clone(), parent.clone());

        let (ordered_txs, filtered) = template_ordered_transactions(&chain, &[]);
        let ordered = ordered_txs
            .into_iter()
            .map(|tx| tx.txid)
            .collect::<Vec<_>>();
        assert_eq!(filtered, 0);
        assert_eq!(ordered, vec![parent.txid, child.txid]);
    }

    #[test]
    fn template_freshness_windows_are_coherent() {
        let created = 1_700_000_000;
        let (not_before, expiry, hard_expiry) = template_freshness_window(created, 0);
        assert_eq!(not_before, created);
        assert_eq!(expiry, created + TEMPLATE_TTL_SECS);
        assert_eq!(hard_expiry, expiry + TEMPLATE_FRESHNESS_GRACE_SECS);

        let explicit_expiry = created + 5;
        let (_, expiry_explicit, hard_expiry_explicit) =
            template_freshness_window(created, explicit_expiry);
        assert_eq!(expiry_explicit, explicit_expiry);
        assert_eq!(
            hard_expiry_explicit,
            explicit_expiry + TEMPLATE_FRESHNESS_GRACE_SECS
        );
    }
    #[tokio::test]
    async fn isolated_mining_node_does_not_get_template_when_p2p_zero_peer() {
        let state = test_state_with_status(P2pStatus {
            mode: P2P_MODE_LIBP2P_REAL.to_string(),
            runtime_started: true,
            listening: vec!["/ip4/127.0.0.1/tcp/19080".to_string()],
            bootnodes_configured: vec!["/ip4/127.0.0.1/tcp/19081/p2p/peer-a".to_string()],
            asymmetric_connectivity_diagnostics: vec![
                "bootnode_peer_accounting_mismatch".to_string()
            ],
            ..P2pStatus::default()
        });

        let reason = mining_template_unavailable_reason(&state).await.unwrap();
        assert!(reason.contains("peer_count=0"));
        assert!(reason.contains("bootnode_peer_accounting_mismatch"));
    }
}
