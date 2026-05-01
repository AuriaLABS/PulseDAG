use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::{ApiResponse, GetBlockTemplateRequest, RpcStateLike},
    handlers::pow_metrics::PowMetricsData,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    build_candidate_block, build_coinbase_transaction, dev_difficulty_snapshot, pow_preimage_bytes,
    preferred_tip_hash, state::ChainState,
};
use tracing::info;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct StoredMiningTemplate {
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
}

#[derive(Debug, serde::Serialize)]
pub struct MiningTemplateData {
    pub mode: String,
    pub algorithm: String,
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
    pub pow_preimage_nonce_offset: usize,
    pub pow_header_preimage_version: u8,
    pub mutable_header_fields: Vec<String>,
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
}

pub(crate) const TEMPLATE_TTL_SECS: u64 = 30;
pub(crate) const TEMPLATE_FRESHNESS_GRACE_SECS: u64 = 2;
const POW_NONCE_OFFSET: usize = 1 + 4;

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

fn template_ordered_transactions(chain: &ChainState) -> Vec<pulsedag_core::types::Transaction> {
    let mut txs = chain
        .mempool
        .transactions
        .iter()
        .map(|(txid, tx)| (txid.clone(), tx.clone()))
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

    ordered
}

pub(crate) fn current_template_state(chain: &ChainState) -> TemplateLifecycleState {
    let height = chain.dag.best_height + 1;
    let mut parent_hashes = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    parent_hashes.sort();
    let selected_tip = preferred_tip_hash(chain);
    let snapshot = dev_difficulty_snapshot(chain);
    let difficulty = u32::try_from(snapshot.suggested_difficulty).unwrap_or(u32::MAX);
    let target_u64 = snapshot.target_u64;
    let mut tx_ids = chain
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    tx_ids.sort();
    let mempool_tx_count = tx_ids.len();
    let mempool_fingerprint = format!("{mempool_tx_count}:{}", tx_ids.join(","));

    TemplateLifecycleState {
        height,
        parent_hashes,
        selected_tip,
        difficulty,
        target_u64,
        mempool_fingerprint,
        mempool_tx_count,
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
    let path = dir.join(format!("{}.json", sanitize(&record.template_id)));
    let _ = fs::write(path, serde_json::to_vec_pretty(record).unwrap_or_default());
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

pub async fn post_mining_template<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<GetBlockTemplateRequest>,
) -> Json<ApiResponse<MiningTemplateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = dev_difficulty_snapshot(&chain);
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
    txs.extend(template_ordered_transactions(&chain));
    let header_difficulty = lifecycle.difficulty;
    let block = build_candidate_block(parents.clone(), height, header_difficulty, txs);
    let target_u64 = lifecycle.target_u64;
    let compact_target = header_difficulty;
    let template_txids = block
        .transactions
        .iter()
        .map(|tx| tx.txid.clone())
        .collect::<Vec<_>>();
    let pow_preimage_hex = hex::encode(pow_preimage_bytes(&block.header));

    store_template(&StoredMiningTemplate {
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
    });
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.external_mining_templates_emitted =
            runtime.external_mining_templates_emitted.saturating_add(1);
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
        suggested_difficulty: snapshot.suggested_difficulty,
        target_u64,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        notes: vec!["Mining template uses centralized runtime retarget policy".to_string()],
    };

    Json(ApiResponse::ok(MiningTemplateData {
        mode: "external-miner-template".to_string(),
        algorithm: pulsedag_core::selected_pow_name().to_string(),
        miner_address: req.miner_address,
        template_id,
        selected_tip,
        parent_tips: block.header.parents.clone(),
        created_at_unix,
        expires_at_unix,
        freshness_ttl_secs: TEMPLATE_TTL_SECS,
        freshness_grace_secs: TEMPLATE_FRESHNESS_GRACE_SECS,
        block,
        target_u64,
        compact_target,
        network_id: chain.chain_id.clone(),
        nonce_range: "0..=18446744073709551615".to_string(),
        timestamp_min_unix: created_at_unix.saturating_sub(1),
        timestamp_max_unix: expires_at_unix.saturating_add(TEMPLATE_FRESHNESS_GRACE_SECS),
        next_height: height,
        blue_score: block.header.blue_score,
        mempool_tx_count: lifecycle.mempool_tx_count,
        metrics_hint,
        pow_preimage_hex,
        pow_preimage_nonce_offset: POW_NONCE_OFFSET,
        pow_header_preimage_version: pulsedag_core::POW_HEADER_PREIMAGE_VERSION,
        mutable_header_fields: vec!["nonce".to_string()],
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        current_template_state, template_freshness_window, template_id_for_state,
        template_ordered_transactions, TEMPLATE_FRESHNESS_GRACE_SECS, TEMPLATE_TTL_SECS,
    };
    use pulsedag_core::{
        genesis::init_chain_state,
        types::{OutPoint, Transaction, TxInput},
    };

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

        let ordered = template_ordered_transactions(&chain)
            .into_iter()
            .map(|tx| tx.txid)
            .collect::<Vec<_>>();
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
}
