use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use pulsedag_core::{
    address_from_public_key, compute_txid,
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    ChainState,
};
use pulsedag_p2p::P2pHandle;
use pulsedag_rpc::{
    api::{NodeRuntimeStats, RpcStateLike},
    routes::{self, ApiExposureProfile, RateLimitConfig, RpcHardeningLimits},
};
use pulsedag_storage::Storage;
use serde_json::Value;
use tokio::sync::RwLock;
use tower::ServiceExt;

const PUBLIC_KEY: &str = "197f6b23e16c8532c6abc838facd5ea789be0c76b2920334039bfa8b3d368d61";
const FUNDED_ADDRESS: &str = "pulse11eab85bfc34210bc555cc9cd0ce0b8b0dd8c530e";
const VALID_SIGNATURE: &str = "083c115a52851be83a0286650e063ee1bb60b6bd4e10763bd4ff0d8e26413d2234f41072cd910dea12b753f658eddc78cec7b5d9731ec31fad981a8e8f50c40c";
const VALID_TXID: &str = "c848725764358e84cc6096d3eb16eb33c4d92fb034a642da3ac3cd409e0d0cea";

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
    std::env::temp_dir().join(format!("pulsedag-public-relay-{name}-{unique}"))
}

fn funded_state(name: &str) -> TestState {
    let path = temp_db_path(name);
    let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
    let mut chain = storage
        .load_or_init_genesis("testnet-dev".to_string())
        .unwrap();

    assert_eq!(address_from_public_key(PUBLIC_KEY), FUNDED_ADDRESS);
    let outpoint = OutPoint {
        txid: "funding".to_string(),
        index: 0,
    };
    chain.utxo.utxos.insert(
        outpoint.clone(),
        Utxo {
            outpoint: outpoint.clone(),
            address: FUNDED_ADDRESS.to_string(),
            amount: 10,
            coinbase: false,
            height: 1,
        },
    );
    chain
        .utxo
        .address_index
        .entry(FUNDED_ADDRESS.to_string())
        .or_default()
        .push(outpoint);
    storage.persist_chain_state(&chain).unwrap();

    TestState {
        chain: Arc::new(RwLock::new(chain)),
        storage,
        runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
    }
}

fn valid_signed_transaction() -> Transaction {
    let tx = Transaction {
        txid: VALID_TXID.to_string(),
        version: 1,
        inputs: vec![TxInput {
            previous_output: OutPoint {
                txid: "funding".to_string(),
                index: 0,
            },
            public_key: PUBLIC_KEY.to_string(),
            signature: VALID_SIGNATURE.to_string(),
        }],
        outputs: vec![TxOutput {
            address: FUNDED_ADDRESS.to_string(),
            amount: 9,
        }],
        fee: 1,
        nonce: 1,
    };
    assert_eq!(compute_txid(&tx), VALID_TXID);
    tx
}

fn public_relay_app(
    state: TestState,
    limits: Option<RpcHardeningLimits>,
) -> axum::Router {
    routes::router_with_profile::<TestState>(
        ApiExposureProfile::PublicSafe,
        false,
        None,
        limits,
    )
    .with_state(state)
}

async fn call(app: axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

fn submit_request(transaction: Transaction) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/tx/submit")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({ "transaction": transaction })).unwrap(),
        ))
        .unwrap()
}

#[tokio::test]
async fn public_safe_relay_accepts_fully_signed_transaction() {
    let state = funded_state("accept");
    let tx = valid_signed_transaction();
    let txid = tx.txid.clone();
    let app = public_relay_app(state.clone(), None);

    let (status, body) = call(app, submit_request(tx)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["accepted"], true);
    assert_eq!(body["data"]["txid"], txid);

    let chain = state.chain.read().await;
    assert!(chain.mempool.transactions.contains_key(&txid));
}

#[tokio::test]
async fn public_safe_relay_rejects_invalid_signature_fail_closed() {
    let state = funded_state("invalid-signature");
    let mut tx = valid_signed_transaction();
    tx.inputs[0].signature = "00".repeat(64);
    tx.txid = compute_txid(&tx);
    let rejected_txid = tx.txid.clone();
    let app = public_relay_app(state.clone(), None);

    let (status, body) = call(app, submit_request(tx)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("TX_REJECTED"), "{body}");

    let chain = state.chain.read().await;
    assert!(!chain.mempool.transactions.contains_key(&rejected_txid));
}

#[tokio::test]
async fn public_safe_relay_rejects_secret_bearing_unknown_fields() {
    let state = funded_state("secret-field");
    let app = public_relay_app(state, None);
    let secret = "raw-private-key-must-never-be-accepted";
    let body = serde_json::to_vec(&serde_json::json!({
        "transaction": valid_signed_transaction(),
        "private_key": secret
    }))
    .unwrap();

    let (status, body) = call(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/tx/submit")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("invalid_transaction_payload"), "{body}");
    assert!(!body.contains(secret));
}

#[tokio::test]
async fn public_safe_relay_keeps_other_write_surfaces_forbidden() {
    let state = funded_state("isolation");
    let app = public_relay_app(state, None);

    for path in [
        "/tx/submit",
        "/api/v1/tx/build",
        "/wallet/new",
        "/api/v1/wallet/new",
        "/admin/diagnostics",
        "/mine",
        "/api/v1/mining/submit",
        "/snapshot/create",
        "/prune",
        "/sync/rebuild",
    ] {
        let (status, body) = call(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        assert!(body.contains("public_route_forbidden"), "{path}: {body}");
    }
}

#[tokio::test]
async fn public_safe_relay_enforces_declared_body_limit() {
    let state = funded_state("body-limit");
    let app = public_relay_app(
        state,
        Some(RpcHardeningLimits {
            request_body_limit_bytes: 32,
            rate_limit: None,
        }),
    );

    let (status, body) = call(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/tx/submit")
            .header("content-type", "application/json")
            .header("content-length", "128")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert!(body.contains("request_too_large"), "{body}");
}

#[tokio::test]
async fn public_safe_relay_enforces_per_ip_rate_limit() {
    let state = funded_state("rate-limit");
    let app = public_relay_app(
        state,
        Some(RpcHardeningLimits {
            request_body_limit_bytes: 128 * 1024,
            rate_limit: Some(RateLimitConfig {
                requests_per_window: 1,
                window_secs: 60,
                per_ip: true,
            }),
        }),
    );
    let remote = ConnectInfo(SocketAddr::from((Ipv4Addr::LOCALHOST, 40000)));

    let first = Request::builder()
        .method("POST")
        .uri("/api/v1/tx/submit")
        .header("content-type", "application/json")
        .extension(remote)
        .body(Body::from("{}"))
        .unwrap();
    let (first_status, first_body) = call(app.clone(), first).await;
    assert_eq!(first_status, StatusCode::BAD_REQUEST, "{first_body}");

    let second = Request::builder()
        .method("POST")
        .uri("/api/v1/tx/submit")
        .header("content-type", "application/json")
        .extension(remote)
        .body(Body::from("{}"))
        .unwrap();
    let (second_status, second_body) = call(app, second).await;
    assert_eq!(second_status, StatusCode::TOO_MANY_REQUESTS, "{second_body}");
    assert!(second_body.contains("rate_limited"), "{second_body}");
}

#[tokio::test]
async fn public_safe_relay_denies_unlisted_browser_origin() {
    let state = funded_state("cors-deny");
    let app = public_relay_app(state, None);

    let (status, body) = call(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/tx/submit")
            .header("origin", "https://evil.example")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("cors_origin_denied"), "{body}");
}
