use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use serde_json::{json, Value};
use sha3::{Digest, Sha3_256};

pub(crate) use super::mining_submit_guard::bind_template_protocol;

pub(crate) const MINING_V3_MAX_INFLIGHT_SUBMITS: usize = 64;
pub(crate) const MINING_V3_MAX_RECONCILIATION_ENTRIES: usize = 4_096;
const MINING_V3_TEMPLATE_PREFIX: &str = "v3:";
const MINING_V3_SUBMIT_PREFIX: &str = "v3-submit-";
const MINING_V3_JOB_PREFIX: &str = "v3-job-";

static MINING_V3_INFLIGHT_SUBMITS: AtomicUsize = AtomicUsize::new(0);
static MINING_V3_NEXT_NODE_SCOPE: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone)]
struct JobObservation {
    job_id: String,
    issued_at_ms: u64,
}

#[derive(Debug, Clone)]
struct CachedSubmit {
    data: Value,
    candidate_fingerprint: String,
    inserted_at_ms: u64,
}

#[derive(Debug)]
struct NodeScope {
    id: usize,
    storage: Weak<pulsedag_storage::Storage>,
}

#[derive(Debug, Default)]
struct MiningV3Registry {
    node_scopes: Vec<NodeScope>,
    jobs: BTreeMap<String, JobObservation>,
    submits: BTreeMap<String, CachedSubmit>,
}

fn registry() -> &'static Mutex<MiningV3Registry> {
    static REGISTRY: OnceLock<Mutex<MiningV3Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(MiningV3Registry::default()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn sha3_hex(domain: &str, material: &str) -> String {
    let digest = Sha3_256::digest(format!("{domain}|{material}").as_bytes());
    hex::encode(digest)
}

fn candidate_fingerprint(block: &pulsedag_core::types::Block) -> Result<String, serde_json::Error> {
    let encoded = serde_json::to_vec(block)?;
    let mut hasher = Sha3_256::new();
    hasher.update(b"pulsedag:mining:v3:candidate\0");
    hasher.update(encoded);
    Ok(hex::encode(hasher.finalize()))
}

pub(crate) fn versioned_template_id(
    internal_template_id: &str,
    protocol_fingerprint: &str,
) -> String {
    if internal_template_id.starts_with(MINING_V3_TEMPLATE_PREFIX) {
        internal_template_id.to_string()
    } else {
        format!("{MINING_V3_TEMPLATE_PREFIX}{protocol_fingerprint}:{internal_template_id}")
    }
}

fn external_protocol_fingerprint(external_template_id: &str) -> Option<&str> {
    let rest = external_template_id.strip_prefix(MINING_V3_TEMPLATE_PREFIX)?;
    let (fingerprint, _) = rest.split_once(':')?;
    (fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(fingerprint)
}

fn internal_template_id(external_template_id: &str) -> &str {
    let Some(rest) = external_template_id.strip_prefix(MINING_V3_TEMPLATE_PREFIX) else {
        return external_template_id;
    };
    match rest.split_once(':') {
        Some((fingerprint, internal))
            if fingerprint.len() == 64
                && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            internal
        }
        _ => rest,
    }
}

fn issued_protocol_fingerprint(internal_template_id: &str) -> Result<String, String> {
    if let Some((_, fingerprint)) = internal_template_id.rsplit_once(':') {
        if internal_template_id.starts_with("v2:")
            && fingerprint.len() == 64
            && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Ok(fingerprint.to_string());
        }
    }
    let stored = super::mining_template_protocol::load_template(internal_template_id)
        .ok_or_else(|| "issued mining template record is unavailable".to_string())?;
    let fingerprint = stored.protocol_identity_fingerprint;
    if fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(fingerprint)
    } else {
        Err("issued mining template is missing its durable protocol fingerprint".to_string())
    }
}

pub(crate) fn job_id_for_template(external_template_id: &str) -> String {
    format!(
        "{MINING_V3_JOB_PREFIX}{}",
        sha3_hex("pulsedag:mining:v3:job", external_template_id)
    )
}

fn submit_id_for(external_template_id: Option<&str>, block_hash: &str) -> String {
    let template_id = external_template_id.unwrap_or("-");
    format!(
        "{MINING_V3_SUBMIT_PREFIX}{}",
        sha3_hex(
            "pulsedag:mining:v3:submit",
            &format!("{template_id}|{block_hash}")
        )
    )
}

fn node_scope_id<S: RpcStateLike>(state: &S) -> usize {
    let storage = state.storage();
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry
        .node_scopes
        .retain(|scope| scope.storage.strong_count() > 0);
    if let Some(id) = registry.node_scopes.iter().find_map(|scope| {
        let existing = scope.storage.upgrade()?;
        Arc::ptr_eq(&existing, &storage).then_some(scope.id)
    }) {
        return id;
    }
    let id = MINING_V3_NEXT_NODE_SCOPE.fetch_add(1, Ordering::Relaxed);
    registry.node_scopes.push(NodeScope {
        id,
        storage: Arc::downgrade(&storage),
    });
    id
}

fn scoped_registry_key(scope_id: usize, identity: &str) -> String {
    format!("{scope_id:016x}:{identity}")
}

fn prune_oldest_jobs(registry: &mut MiningV3Registry) {
    while registry.jobs.len() > MINING_V3_MAX_RECONCILIATION_ENTRIES {
        let oldest = registry
            .jobs
            .iter()
            .min_by_key(|(_, observation)| observation.issued_at_ms)
            .map(|(key, _)| key.clone());
        let Some(oldest) = oldest else {
            break;
        };
        registry.jobs.remove(&oldest);
    }
}

fn prune_oldest_submits(registry: &mut MiningV3Registry) {
    while registry.submits.len() > MINING_V3_MAX_RECONCILIATION_ENTRIES {
        let oldest = registry
            .submits
            .iter()
            .min_by_key(|(_, cached)| cached.inserted_at_ms)
            .map(|(key, _)| key.clone());
        let Some(oldest) = oldest else {
            break;
        };
        registry.submits.remove(&oldest);
    }
}

pub(crate) fn register_v3_job<S: RpcStateLike>(
    state: &S,
    external_template_id: String,
    job_id: String,
    issued_at_ms: u64,
) {
    let scope_id = node_scope_id(state);
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.jobs.insert(
        scoped_registry_key(scope_id, &external_template_id),
        JobObservation {
            job_id,
            issued_at_ms,
        },
    );
    prune_oldest_jobs(&mut registry);
}

fn cached_submit(
    scope_id: usize,
    submit_id: &str,
    candidate_fingerprint: &str,
) -> Option<(Value, bool)> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .submits
        .get(&scoped_registry_key(scope_id, submit_id))
        .map(|cached| {
            (
                cached.data.clone(),
                cached.candidate_fingerprint == candidate_fingerprint,
            )
        })
}

fn cache_submit(scope_id: usize, submit_id: String, candidate_fingerprint: String, data: Value) {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.submits.insert(
        scoped_registry_key(scope_id, &submit_id),
        CachedSubmit {
            data,
            candidate_fingerprint,
            inserted_at_ms: now_ms(),
        },
    );
    prune_oldest_submits(&mut registry);
}

fn job_observation(scope_id: usize, external_template_id: Option<&str>) -> Option<JobObservation> {
    external_template_id.and_then(|template_id| {
        registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .jobs
            .get(&scoped_registry_key(scope_id, template_id))
            .cloned()
    })
}

struct InflightSubmitGuard;

impl Drop for InflightSubmitGuard {
    fn drop(&mut self) {
        MINING_V3_INFLIGHT_SUBMITS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn try_enter_submit() -> Option<InflightSubmitGuard> {
    MINING_V3_INFLIGHT_SUBMITS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < MINING_V3_MAX_INFLIGHT_SUBMITS).then_some(current + 1)
        })
        .ok()
        .map(|_| InflightSubmitGuard)
}

fn finality_for(data: &Value) -> &'static str {
    let reason_code = data
        .get("reason_code")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if data
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || reason_code == "duplicate_block"
        || reason_code == "accepted_reconciled"
    {
        "accepted"
    } else if reason_code == "submit_finality_unknown" {
        "unknown_finality"
    } else if data
        .get("stale_template")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || reason_code == "stale_template"
    {
        "stale"
    } else {
        "rejected"
    }
}

fn decorate_submit_data(
    mut data: Value,
    external_template_id: Option<&str>,
    submit_id: &str,
    job: Option<&JobObservation>,
    reconciled: bool,
) -> Value {
    let finality = finality_for(&data);
    let object = data
        .as_object_mut()
        .expect("serialized mining submit data must be an object");
    object.insert("protocol_version".to_string(), json!(3));
    object.insert("submit_id".to_string(), json!(submit_id));
    object.insert("finality".to_string(), json!(finality));
    object.insert("reconciled".to_string(), json!(reconciled));
    object.insert(
        "template_id".to_string(),
        external_template_id.map_or(Value::Null, |value| json!(value)),
    );
    object.insert(
        "job_id".to_string(),
        job.map_or(Value::Null, |observation| json!(observation.job_id)),
    );
    object.insert(
        "template_to_submit_ms".to_string(),
        job.map_or(Value::Null, |observation| {
            json!(now_ms().saturating_sub(observation.issued_at_ms))
        }),
    );
    object.insert(
        "reconciliation".to_string(),
        json!({
            "identity": "sha3-256(template_id|block_hash)",
            "replay_policy": "return-cached-finality-without-rebroadcast",
            "unknown_finality_policy": "reconcile-chain-before-retry"
        }),
    );
    data
}

fn overload_data(
    req: &SubmitMinedBlockRequest,
    external_template_id: Option<&str>,
    submit_id: &str,
    job: Option<&JobObservation>,
) -> Value {
    decorate_submit_data(
        json!({
            "accepted": false,
            "reason": "mining submit concurrency limit reached; retry with the same submit_id",
            "block_hash": req.block.hash,
            "block_id": Value::Null,
            "height": req.block.header.height,
            "pow_algorithm": pulsedag_core::selected_pow_name(),
            "pow_accepted": false,
            "pow_accepted_dev": false,
            "target_u64": 0,
            "target_hex": format!("{:064x}", 0_u64),
            "pow_hash": Value::Null,
            "invalid_pow": false,
            "stale": false,
            "duplicate": false,
            "stale_template": false,
            "reason_code": "submit_overloaded",
            "selected_tip": Value::Null,
            "adopted_orphans": 0,
            "pow_hash_score_u64": 0,
            "pow_rejection_code": Value::Null,
            "pow_rejection_reason": "mining submit concurrency limit reached"
        }),
        external_template_id,
        submit_id,
        job,
        false,
    )
}

fn candidate_identity_mismatch_data(
    req: &SubmitMinedBlockRequest,
    external_template_id: Option<&str>,
    submit_id: &str,
    job: Option<&JobObservation>,
) -> Value {
    decorate_submit_data(
        json!({
            "accepted": false,
            "reason": "submit identity is already bound to different block material",
            "block_hash": req.block.hash,
            "block_id": Value::Null,
            "height": req.block.header.height,
            "pow_algorithm": pulsedag_core::selected_pow_name(),
            "pow_accepted": false,
            "pow_accepted_dev": false,
            "target_u64": 0,
            "target_hex": format!("{:064x}", 0_u64),
            "pow_hash": Value::Null,
            "invalid_pow": false,
            "stale": false,
            "duplicate": false,
            "stale_template": false,
            "reason_code": "candidate_identity_mismatch",
            "selected_tip": Value::Null,
            "adopted_orphans": 0,
            "pow_hash_score_u64": 0,
            "pow_rejection_code": Value::Null,
            "pow_rejection_reason": "same submit_id/block_hash presented with different block material"
        }),
        external_template_id,
        submit_id,
        job,
        true,
    )
}

fn protocol_identity_mismatch_data(
    req: &SubmitMinedBlockRequest,
    external_template_id: Option<&str>,
    submit_id: &str,
    job: Option<&JobObservation>,
    expected: &str,
    claimed: &str,
) -> Value {
    decorate_submit_data(
        json!({
            "accepted": false,
            "reason": "external mining template protocol identity does not match issued work",
            "block_hash": req.block.hash,
            "block_id": Value::Null,
            "height": req.block.header.height,
            "pow_algorithm": pulsedag_core::selected_pow_name(),
            "pow_accepted": false,
            "pow_accepted_dev": false,
            "target_u64": 0,
            "target_hex": format!("{:064x}", 0_u64),
            "pow_hash": Value::Null,
            "invalid_pow": false,
            "stale": false,
            "duplicate": false,
            "stale_template": false,
            "reason_code": "protocol_identity_mismatch",
            "selected_tip": Value::Null,
            "adopted_orphans": 0,
            "pow_hash_score_u64": 0,
            "pow_rejection_code": "protocol_identity_mismatch",
            "pow_rejection_reason": format!("expected protocol fingerprint {expected}, got {claimed}")
        }),
        external_template_id,
        submit_id,
        job,
        false,
    )
}

async fn known_block_reconciliation<S: RpcStateLike>(
    state: &S,
    req: &SubmitMinedBlockRequest,
    external_template_id: Option<&str>,
    submit_id: &str,
    job: Option<&JobObservation>,
    incoming_candidate_fingerprint: &str,
) -> Option<Value> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let stored_block = chain.dag.blocks.get(&req.block.hash)?;
    let stored_candidate_fingerprint = match candidate_fingerprint(stored_block) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            drop(chain);
            return Some(candidate_identity_mismatch_data(
                req,
                external_template_id,
                submit_id,
                job,
            ));
        }
    };
    if stored_candidate_fingerprint != incoming_candidate_fingerprint {
        drop(chain);
        return Some(candidate_identity_mismatch_data(
            req,
            external_template_id,
            submit_id,
            job,
        ));
    }
    let selected_tip = pulsedag_core::preferred_tip_hash(&chain);
    drop(chain);
    Some(decorate_submit_data(
        json!({
            "accepted": true,
            "reason": "accepted_reconciled",
            "block_hash": req.block.hash,
            "block_id": req.block.hash,
            "height": req.block.header.height,
            "pow_algorithm": pulsedag_core::selected_pow_name(),
            "pow_accepted": true,
            "pow_accepted_dev": true,
            "target_u64": 0,
            "target_hex": format!("{:064x}", 0_u64),
            "pow_hash": Value::Null,
            "invalid_pow": false,
            "stale": false,
            "duplicate": true,
            "stale_template": false,
            "reason_code": "accepted_reconciled",
            "selected_tip": selected_tip,
            "adopted_orphans": 0,
            "pow_hash_score_u64": 0,
            "pow_rejection_code": Value::Null,
            "pow_rejection_reason": Value::Null
        }),
        external_template_id,
        submit_id,
        job,
        true,
    ))
}

pub async fn post_mining_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(mut req): Json<SubmitMinedBlockRequest>,
) -> Json<ApiResponse<Value>> {
    let external_template_id = req.template_id.clone();
    let submit_id = submit_id_for(external_template_id.as_deref(), &req.block.hash);
    let scope_id = node_scope_id(&state);
    let job = job_observation(scope_id, external_template_id.as_deref());
    if let Some(claimed) = external_template_id
        .as_deref()
        .and_then(external_protocol_fingerprint)
    {
        let internal = external_template_id
            .as_deref()
            .map(internal_template_id)
            .unwrap_or_default();
        let expected = match issued_protocol_fingerprint(internal) {
            Ok(expected) => expected,
            Err(error) => {
                return Json(ApiResponse::err(
                    "MINING_PROTOCOL_V3_PROTOCOL_IDENTITY",
                    format!("cannot resolve issued mining protocol identity: {error}"),
                ));
            }
        };
        if claimed != expected {
            return Json(ApiResponse::ok(protocol_identity_mismatch_data(
                &req,
                external_template_id.as_deref(),
                &submit_id,
                job.as_ref(),
                &expected,
                claimed,
            )));
        }
    }
    let candidate_fingerprint = match candidate_fingerprint(&req.block) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return Json(ApiResponse::err(
                "MINING_PROTOCOL_V3_CANDIDATE_IDENTITY",
                format!("cannot bind mining submit candidate identity: {error}"),
            ));
        }
    };

    if let Some(reconciled) = known_block_reconciliation(
        &state,
        &req,
        external_template_id.as_deref(),
        &submit_id,
        job.as_ref(),
        &candidate_fingerprint,
    )
    .await
    {
        return Json(ApiResponse::ok(reconciled));
    }

    if let Some((mut cached, exact_candidate)) =
        cached_submit(scope_id, &submit_id, &candidate_fingerprint)
    {
        if !exact_candidate {
            return Json(ApiResponse::ok(candidate_identity_mismatch_data(
                &req,
                external_template_id.as_deref(),
                &submit_id,
                job.as_ref(),
            )));
        }
        if let Some(object) = cached.as_object_mut() {
            object.insert("reconciled".to_string(), json!(true));
        }
        return Json(ApiResponse::ok(cached));
    }

    let Some(_inflight_guard) = try_enter_submit() else {
        return Json(ApiResponse::ok(overload_data(
            &req,
            external_template_id.as_deref(),
            &submit_id,
            job.as_ref(),
        )));
    };

    if let Some(template_id) = req.template_id.as_mut() {
        *template_id = internal_template_id(template_id).to_string();
    }

    let response =
        super::mining_submit_guard::post_mining_submit(State(state.clone()), Json(req)).await;
    let ApiResponse {
        ok,
        data,
        error,
        meta,
    } = response.0;

    let Some(data) = data else {
        return Json(ApiResponse {
            ok,
            data: None,
            error,
            meta,
        });
    };

    let value = match serde_json::to_value(data) {
        Ok(value) => decorate_submit_data(
            value,
            external_template_id.as_deref(),
            &submit_id,
            job.as_ref(),
            false,
        ),
        Err(error) => {
            return Json(ApiResponse::err(
                "MINING_PROTOCOL_V3_SERIALIZATION",
                format!("cannot serialize mining submit response: {error}"),
            ));
        }
    };

    cache_submit(
        scope_id,
        submit_id.clone(),
        candidate_fingerprint,
        value.clone(),
    );
    let _ = state.storage().append_runtime_event(
        "info",
        "external_mining_v3_submit",
        &format!(
            "submit_id={} job_id={} template_id={} block_hash={} finality={} template_to_submit_ms={}",
            submit_id,
            job.as_ref().map(|value| value.job_id.as_str()).unwrap_or("-"),
            external_template_id.as_deref().unwrap_or("-"),
            value.get("block_hash").and_then(Value::as_str).unwrap_or("-"),
            value.get("finality").and_then(Value::as_str).unwrap_or("-"),
            value
                .get("template_to_submit_ms")
                .and_then(Value::as_u64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
    );

    Json(ApiResponse {
        ok,
        data: Some(value),
        error,
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task37_versioned_template_ids_round_trip_without_losing_internal_identity() {
        let internal = "v1-work-abc";
        let fingerprint = "11".repeat(32);
        let external = versioned_template_id(internal, &fingerprint);
        assert_eq!(external, format!("v3:{fingerprint}:v1-work-abc"));
        assert_eq!(
            external_protocol_fingerprint(&external),
            Some(fingerprint.as_str())
        );
        assert_eq!(internal_template_id(&external), internal);
        assert_eq!(versioned_template_id(&external, &fingerprint), external);
        assert_eq!(internal_template_id("v3:v1-work-abc"), internal);
    }

    #[test]
    fn task37_protocol_identity_golden_vectors_are_frozen() {
        let template_id = concat!(
            "v3:",
            "1111111111111111111111111111111111111111111111111111111111111111",
            ":v1-work-abc"
        );
        assert_eq!(
            job_id_for_template(template_id),
            "v3-job-6f2746fc48959fd03d4b11c2cc24704ab522f2eac07a1bdc5dc2466926c41ae6"
        );
        assert_eq!(
            submit_id_for(Some(template_id), "block-123"),
            "v3-submit-48f2e0e1cbd16c5245b720cfc87d3959a36176be6345aad3a5b1f0561970536d"
        );
        let other = versioned_template_id("v1-work-abc", &"22".repeat(32));
        assert_ne!(
            job_id_for_template(template_id),
            job_id_for_template(&other)
        );
    }

    #[test]
    fn task37_reconciliation_cache_keys_are_node_scoped_without_changing_submit_identity() {
        let submit_id = submit_id_for(Some("v3:v1-work-abc"), "block-123");
        let first = scoped_registry_key(1, &submit_id);
        let second = scoped_registry_key(2, &submit_id);
        assert_ne!(first, second);
        assert!(first.ends_with(&submit_id));
        assert!(second.ends_with(&submit_id));
    }

    #[test]
    fn task37_candidate_fingerprint_binds_full_block_material() {
        use pulsedag_core::types::{Block, BlockHeader, Transaction, TxOutput};

        let block = Block {
            hash: "same-declared-hash".to_string(),
            header: BlockHeader {
                version: 1,
                parents: vec!["parent".to_string()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: "merkle".to_string(),
                state_root: "state".to_string(),
                blue_score: 1,
                height: 2,
            },
            transactions: vec![Transaction {
                txid: "txid".to_string(),
                version: 1,
                inputs: Vec::new(),
                outputs: vec![TxOutput {
                    address: "pulse1candidate".to_string(),
                    amount: 7,
                }],
                fee: 0,
                nonce: 1,
            }],
        };
        let mut changed = block.clone();
        changed.transactions[0].outputs[0].amount = 8;

        assert_ne!(
            candidate_fingerprint(&block).unwrap(),
            candidate_fingerprint(&changed).unwrap()
        );
        assert_eq!(block.hash, changed.hash);
    }

    #[test]
    fn task37_finality_states_are_frozen() {
        assert_eq!(finality_for(&json!({"accepted": true})), "accepted");
        assert_eq!(
            finality_for(&json!({"accepted": false, "reason_code": "duplicate_block"})),
            "accepted"
        );
        assert_eq!(
            finality_for(&json!({"accepted": false, "reason_code": "stale_template"})),
            "stale"
        );
        assert_eq!(
            finality_for(&json!({"accepted": false, "reason_code": "submit_finality_unknown"})),
            "unknown_finality"
        );
        assert_eq!(
            finality_for(&json!({"accepted": false, "reason_code": "invalid_pow"})),
            "rejected"
        );
    }

    #[test]
    fn task37_submit_registry_is_hard_bounded() {
        let mut registry = MiningV3Registry::default();
        for index in 0..=MINING_V3_MAX_RECONCILIATION_ENTRIES {
            registry.submits.insert(
                format!("submit-{index:08}"),
                CachedSubmit {
                    data: json!({"index": index}),
                    candidate_fingerprint: format!("candidate-{index}"),
                    inserted_at_ms: index as u64,
                },
            );
        }
        prune_oldest_submits(&mut registry);
        assert_eq!(registry.submits.len(), MINING_V3_MAX_RECONCILIATION_ENTRIES);
        assert!(!registry.submits.contains_key("submit-00000000"));
    }
}
