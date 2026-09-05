use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
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

#[derive(Debug, Clone)]
struct JobObservation {
    job_id: String,
    issued_at_ms: u64,
}

#[derive(Debug, Clone)]
struct CachedSubmit {
    data: Value,
    inserted_at_ms: u64,
}

#[derive(Debug, Default)]
struct MiningV3Registry {
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

pub(crate) fn versioned_template_id(internal_template_id: &str) -> String {
    if internal_template_id.starts_with(MINING_V3_TEMPLATE_PREFIX) {
        internal_template_id.to_string()
    } else {
        format!("{MINING_V3_TEMPLATE_PREFIX}{internal_template_id}")
    }
}

fn internal_template_id(external_template_id: &str) -> &str {
    external_template_id
        .strip_prefix(MINING_V3_TEMPLATE_PREFIX)
        .unwrap_or(external_template_id)
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

pub(crate) fn register_v3_job(external_template_id: String, job_id: String, issued_at_ms: u64) {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.jobs.insert(
        external_template_id,
        JobObservation {
            job_id,
            issued_at_ms,
        },
    );
    prune_oldest_jobs(&mut registry);
}

fn cached_submit(submit_id: &str) -> Option<Value> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .submits
        .get(submit_id)
        .map(|cached| cached.data.clone())
}

fn cache_submit(submit_id: String, data: Value) {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.submits.insert(
        submit_id,
        CachedSubmit {
            data,
            inserted_at_ms: now_ms(),
        },
    );
    prune_oldest_submits(&mut registry);
}

fn job_observation(external_template_id: Option<&str>) -> Option<JobObservation> {
    external_template_id.and_then(|template_id| {
        registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .jobs
            .get(template_id)
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

async fn known_block_reconciliation<S: RpcStateLike>(
    state: &S,
    req: &SubmitMinedBlockRequest,
    external_template_id: Option<&str>,
    submit_id: &str,
    job: Option<&JobObservation>,
) -> Option<Value> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    if !chain.dag.blocks.contains_key(&req.block.hash) {
        return None;
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
    let job = job_observation(external_template_id.as_deref());

    if let Some(reconciled) = known_block_reconciliation(
        &state,
        &req,
        external_template_id.as_deref(),
        &submit_id,
        job.as_ref(),
    )
    .await
    {
        cache_submit(submit_id.clone(), reconciled.clone());
        return Json(ApiResponse::ok(reconciled));
    }

    if let Some(mut cached) = cached_submit(&submit_id) {
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

    cache_submit(submit_id.clone(), value.clone());
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
        let external = versioned_template_id(internal);
        assert_eq!(external, "v3:v1-work-abc");
        assert_eq!(internal_template_id(&external), internal);
        assert_eq!(versioned_template_id(&external), external);
    }

    #[test]
    fn task37_protocol_identity_golden_vectors_are_frozen() {
        assert_eq!(
            job_id_for_template("v3:v1-work-abc"),
            "v3-job-dc75926e7d0f69e205aac68bae505b0cae953de4b7242880d91033169c394fd3"
        );
        assert_eq!(
            submit_id_for(Some("v3:v1-work-abc"), "block-123"),
            "v3-submit-555a479be74c0128d02c67fdacfa98f0c923e51739fb534510a0e12208e9e675"
        );
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
                    inserted_at_ms: index as u64,
                },
            );
        }
        prune_oldest_submits(&mut registry);
        assert_eq!(
            registry.submits.len(),
            MINING_V3_MAX_RECONCILIATION_ENTRIES
        );
        assert!(!registry.submits.contains_key("submit-00000000"));
    }
}
