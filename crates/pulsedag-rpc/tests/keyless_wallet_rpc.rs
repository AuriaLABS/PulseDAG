use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use pulsedag_core::state::ChainState;
use pulsedag_p2p::P2pHandle;
use pulsedag_rpc::{
    api::{NodeRuntimeStats, RpcStateLike},
    routes::{self, ApiExposureProfile},
};
use pulsedag_storage::Storage;
use tokio::sync::RwLock;
use tower::ServiceExt;

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

fn test_state(name: &str) -> TestState {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path: PathBuf = std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"));
    let storage = Arc::new(Storage::open(path.to_str().expect("temp path")).expect("storage"));
    let chain = storage
        .load_or_init_genesis("keyless-wallet-rpc-test".to_string())
        .expect("chain");
    TestState {
        chain: Arc::new(RwLock::new(chain)),
        storage,
        runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
    }
}

async fn response(app: axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.oneshot(request).await.expect("router response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (
        status,
        String::from_utf8(body.to_vec()).expect("utf8 response"),
    )
}

#[tokio::test]
async fn admin_router_keeps_removed_wallet_contract_fail_closed() {
    let token = "keyless-node-test-token";
    let app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        true,
        Some(token.to_string()),
        None,
    )
    .with_state(test_state("keyless-admin"));

    for path in [
        "/wallet/new",
        "/wallet/sign",
        "/wallet/transfer",
        "/admin/wallet/new",
        "/admin/wallet/sign",
        "/admin/wallet/transfer",
    ] {
        let (status, body) = response(
            app.clone(),
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "unexpected status for {path}"
        );
        assert!(
            body.contains("legacy_wallet_rpc_removed"),
            "missing removed-wallet tombstone error for {path}: {body}"
        );
        assert!(!body.contains(token));
        assert!(!body.contains("private_key"));
    }
}

#[tokio::test]
async fn release_metadata_advertises_keyless_node_and_signed_relay_only() {
    let app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        false,
        None,
        None,
    )
    .with_state(test_state("keyless-release"));
    let (status, body) = response(
        app,
        Request::builder()
            .method("GET")
            .uri("/release")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("keyless_node"));
    assert!(body.contains("signed_transaction_relay"));
    assert!(body.contains("/api/v1/tx/submit"));
    assert!(!body.contains("legacy_wallet_rpc_dev_only"));
    assert!(!body.contains("\"wallets\""));
    assert!(!body.contains("/wallet/new"));
    assert!(!body.contains("/wallet/sign"));
    assert!(!body.contains("/wallet/transfer"));
}
