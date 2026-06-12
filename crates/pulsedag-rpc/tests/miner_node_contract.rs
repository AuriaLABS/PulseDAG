use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, LazyLock},
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
static MINING_SUBMIT_CONTRACT_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

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
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = TEMP_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("pulsedag-{name}-{unique}-{counter}"))
}

fn test_state() -> TestState {
    let path = temp_db_path("miner-node-contract");
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

async fn request_template(state: &TestState) -> Value {
    post_json(
        state,
        "/mining/template",
        serde_json::to_value(GetBlockTemplateRequest {
            miner_address: "kaspa:qptestminer".to_string(),
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
    assert!(
        result.accepted,
        "test difficulty should mine deterministically"
    );
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
        assert!(
            attempt < 99,
            "bounded submit actor stayed busy after deterministic retries"
        );
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    unreachable!("retry loop returns or panics before exhaustion")
}

#[tokio::test]
async fn mining_contract_routes_cover_accept_reject_codes_serially() {
    scenario_node_rejects_invalid_pow_with_stable_code().await;
    scenario_node_rejects_stale_submit_with_stable_code().await;
    scenario_node_rejects_malformed_block_with_stable_code().await;
    scenario_miner_requests_template_mines_with_canonical_cpu_and_node_accepts_submit().await;
}

async fn scenario_miner_requests_template_mines_with_canonical_cpu_and_node_accepts_submit() {
    // These route-level contract tests use production singleton mining template
    // storage and submit actor state. Serialize each full test so focused
    // accept/reject assertions do not race the bounded actor queue or template
    // files; unit tests cover the explicit queue-full `submit_busy` schema.
    let _contract_guard = MINING_SUBMIT_CONTRACT_LOCK.lock().await;
    let state = test_state();

    let template = request_template(&state).await;
    let data = &template["data"];
    assert_eq!(template["ok"], true);
    assert_eq!(
        data["algorithm"].as_str().unwrap(),
        pulsedag_core::selected_pow_name()
    );
    assert!(data["template_id"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    assert!(data["block"].is_object());
    assert!(data["compact_target"]
        .as_u64()
        .is_some_and(|target| target > 0));
    assert!(data["created_at_unix"]
        .as_u64()
        .is_some_and(|created| created > 0));
    assert!(data["expires_at_unix"]
        .as_u64()
        .is_some_and(|expires| expires > 0));

    let compact_target = data["compact_target"].as_u64().unwrap() as u32;
    let block = mine_with_canonical_cpu(template_block(&template), compact_target);
    let submit = submit_block(&state, template_id(&template), block).await;

    let submit_data = &submit["data"];
    assert_eq!(submit["ok"], true);
    assert_eq!(submit_data["accepted"], true);
    assert_eq!(submit_data["reason_code"], "accepted");
    assert_eq!(submit_data["pow_accepted"], true);
}

async fn scenario_node_rejects_stale_submit_with_stable_code() {
    // These route-level contract tests use production singleton mining template
    // storage and submit actor state. Serialize each full test so focused
    // accept/reject assertions do not race the bounded actor queue or template
    // files; unit tests cover the explicit queue-full `submit_busy` schema.
    let _contract_guard = MINING_SUBMIT_CONTRACT_LOCK.lock().await;
    let state = test_state();
    let template = request_template(&state).await;
    let compact_target = template["data"]["compact_target"].as_u64().unwrap() as u32;
    let block = mine_with_canonical_cpu(template_block(&template), compact_target);

    {
        let mut chain = state.chain.write().await;
        let tx = pulsedag_core::build_coinbase_transaction(
            "kaspa:qptestmempool",
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
}

async fn scenario_node_rejects_invalid_pow_with_stable_code() {
    // These route-level contract tests use production singleton mining template
    // storage and submit actor state. Serialize each full test so focused
    // accept/reject assertions do not race the bounded actor queue or template
    // files; unit tests cover the explicit queue-full `submit_busy` schema.
    let _contract_guard = MINING_SUBMIT_CONTRACT_LOCK.lock().await;
    let state = test_state();
    let template = request_template(&state).await;
    let mut block = template_block(&template);
    block.header.difficulty = 0x0100_0000;
    block.header.nonce = 0;
    block.hash = compute_block_hash(&block.header);

    let submit = submit_block(&state, template_id(&template), block).await;
    assert_eq!(submit["ok"], true);
    assert_eq!(submit["data"]["accepted"], false);
    assert_eq!(submit["data"]["reason_code"], "invalid_pow");
    assert_eq!(submit["data"]["invalid_pow"], true);
}

async fn scenario_node_rejects_malformed_block_with_stable_code() {
    // These route-level contract tests use production singleton mining template
    // storage and submit actor state. Serialize each full test so focused
    // accept/reject assertions do not race the bounded actor queue or template
    // files; unit tests cover the explicit queue-full `submit_busy` schema.
    let _contract_guard = MINING_SUBMIT_CONTRACT_LOCK.lock().await;
    let state = test_state();
    let template = request_template(&state).await;
    let compact_target = template["data"]["compact_target"].as_u64().unwrap() as u32;
    let mut block = mine_with_canonical_cpu(template_block(&template), compact_target);
    block.hash = "not-the-canonical-block-hash".to_string();

    let submit = submit_block(&state, template_id(&template), block).await;
    assert_eq!(submit["ok"], true);
    assert_eq!(submit["data"]["accepted"], false);
    assert_eq!(submit["data"]["reason_code"], "malformed_serialization");
}
