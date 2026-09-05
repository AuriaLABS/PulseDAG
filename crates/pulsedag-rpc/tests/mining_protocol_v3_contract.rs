use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use pulsedag_api::{GetBlockTemplateRequest, SubmitMinedBlockRequest};
use pulsedag_core::{
    state::ChainState,
    types::{compute_block_hash, Block},
};
use pulsedag_miner::{CpuMiningBackend, MiningBackend};
use pulsedag_p2p::P2pHandle;
use pulsedag_rpc::{
    api::{NodeRuntimeStats, RpcStateLike},
    routes,
};
use pulsedag_storage::Storage;
use serde_json::Value;
use tokio::sync::RwLock;
use tower::ServiceExt;

static TEMP_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct TestState {
    chain: Arc<RwLock<ChainState>>,
    storage: Arc<Storage>,
    runtime: Arc<RwLock<NodeRuntimeStats>>,
}

impl RpcStateLike for TestState {
    fn chain(&self) -> Arc<RwLock<ChainState>> {
        self.chain.clone()
    }

    fn p2p(&self) -> Option<Arc<dyn P2pHandle>> {
        None
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
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = TEMP_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("pulsedag-{name}-{unique}-{counter}"))
}

fn test_state() -> TestState {
    let path = temp_db_path("mining-protocol-v3-contract");
    let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
    let chain = storage
        .load_or_init_genesis("testnet-dev".to_string())
        .unwrap();
    TestState {
        chain: Arc::new(RwLock::new(chain)),
        storage,
        runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
    }
}

async fn post_json(state: &TestState, uri: &str, body: Value) -> Value {
    for attempt in 0..20 {
        let app = routes::router::<TestState>().with_state(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            assert!(attempt < 19, "test route stayed rate-limited after retries");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        }
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        return serde_json::from_slice(&body).unwrap();
    }
    unreachable!("retry loop returns or panics before exhaustion")
}

async fn request_template(state: &TestState, miner_address: &str) -> Value {
    post_json(
        state,
        "/mining/template",
        serde_json::to_value(GetBlockTemplateRequest {
            miner_address: miner_address.to_string(),
        })
        .unwrap(),
    )
    .await
}

fn template_block(template: &Value) -> Block {
    serde_json::from_value(template["data"]["block"].clone()).unwrap()
}

fn template_id(template: &Value) -> String {
    template["data"]["template_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn mine_with_canonical_cpu(mut block: Block, compact_target: u32) -> Block {
    let result = CpuMiningBackend
        .mine_header(block.header.clone(), 100_000, 2, compact_target)
        .expect("canonical CPU mining should not fail");
    assert!(result.accepted, "test difficulty should mine deterministically");
    block.header = result.header;
    block.hash = compute_block_hash(&block.header);
    block
}

async fn submit_block(state: &TestState, template_id: String, block: Block) -> Value {
    let request = serde_json::to_value(SubmitMinedBlockRequest {
        template_id: Some(template_id),
        block,
    })
    .unwrap();

    for attempt in 0..100 {
        let response = post_json(state, "/mining/submit", request.clone()).await;
        if response["data"]["reason_code"] != "submit_busy" {
            return response;
        }
        assert!(attempt < 99, "bounded submit actor stayed busy after retries");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    unreachable!("retry loop returns or panics before exhaustion")
}

#[tokio::test]
async fn task37_cpu_miner_contract_reconciles_reconnect_without_rebroadcast() {
    let state = test_state();
    let template = request_template(&state, "kaspa:qptask37miner").await;
    let data = &template["data"];

    assert_eq!(template["ok"], true);
    assert_eq!(data["protocol_version"], 3);
    assert!(data["template_id"]
        .as_str()
        .is_some_and(|value| value.starts_with("v3:")));
    assert!(data["job_id"]
        .as_str()
        .is_some_and(|value| value.starts_with("v3-job-")));
    assert!(data["work_token"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
    assert_eq!(
        data["new_work_notification"]["max_outstanding_snapshots"],
        1
    );
    assert_eq!(data["resource_limits"]["max_inflight_submits"], 64);

    let compact_target = data["compact_target"].as_u64().unwrap() as u32;
    let block = mine_with_canonical_cpu(template_block(&template), compact_target);
    let first = submit_block(&state, template_id(&template), block.clone()).await;
    assert_eq!(first["ok"], true);
    assert_eq!(first["data"]["accepted"], true);
    assert_eq!(first["data"]["reason_code"], "accepted");
    assert_eq!(first["data"]["finality"], "accepted");
    assert_eq!(first["data"]["reconciled"], false);
    let submit_id = first["data"]["submit_id"].as_str().unwrap().to_string();
    assert!(submit_id.starts_with("v3-submit-"));

    // Reconnect/retry of the exact candidate must reconcile against chain state
    // before any lower-layer submit/broadcast path can run again.
    let replay = submit_block(&state, template_id(&template), block).await;
    assert_eq!(replay["ok"], true);
    assert_eq!(replay["data"]["reason_code"], "accepted_reconciled");
    assert_eq!(replay["data"]["finality"], "accepted");
    assert_eq!(replay["data"]["reconciled"], true);
    assert_eq!(replay["data"]["submit_id"], submit_id);
}

#[tokio::test]
async fn task37_stale_template_maps_to_frozen_stale_finality() {
    let state = test_state();
    let template = request_template(&state, "kaspa:qptask37stale").await;
    let compact_target = template["data"]["compact_target"].as_u64().unwrap() as u32;
    let block = mine_with_canonical_cpu(template_block(&template), compact_target);

    {
        let mut chain = state.chain.write().await;
        let tx = pulsedag_core::build_coinbase_transaction(
            "kaspa:qptask37mempool",
            1,
            chain.dag.best_height + 1,
        );
        chain.mempool.transactions.insert(tx.txid.clone(), tx);
    }

    let submit = submit_block(&state, template_id(&template), block).await;
    assert_eq!(submit["ok"], true);
    assert_eq!(submit["data"]["accepted"], false);
    assert_eq!(submit["data"]["reason_code"], "stale_template");
    assert_eq!(submit["data"]["stale_template"], true);
    assert_eq!(submit["data"]["finality"], "stale");
}

#[tokio::test]
async fn task37_multi_miner_work_has_distinct_stable_job_identity_and_bounded_notifications() {
    let state = test_state();
    let miner_a = request_template(&state, "kaspa:qptask37minera");
    let miner_b = request_template(&state, "kaspa:qptask37minerb");
    let (template_a, template_b) = tokio::join!(miner_a, miner_b);

    for template in [&template_a, &template_b] {
        assert_eq!(template["ok"], true);
        assert_eq!(template["data"]["protocol_version"], 3);
        assert_eq!(
            template["data"]["new_work_notification"]["max_outstanding_snapshots"],
            1
        );
        assert_eq!(template["data"]["resource_limits"]["max_inflight_submits"], 64);
    }

    let template_a_id = template_id(&template_a);
    let template_b_id = template_id(&template_b);
    let job_a = template_a["data"]["job_id"].as_str().unwrap();
    let job_b = template_b["data"]["job_id"].as_str().unwrap();
    assert_ne!(template_a_id, template_b_id);
    assert_ne!(job_a, job_b);
}
