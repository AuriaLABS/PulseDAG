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
    routes::{ApiExposureProfile, RateLimitConfig, RpcHardeningLimits},
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

fn temp_db_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("pulsedag-task30-{name}-{unique}"))
}

fn test_state(name: &str) -> TestState {
    let path = temp_db_path(name);
    let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
    let chain = storage
        .load_or_init_genesis("task30-relay-bounds".to_string())
        .unwrap();
    TestState {
        chain: Arc::new(RwLock::new(chain)),
        storage,
        runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
    }
}

async fn call(app: axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

fn json_post(uri: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn task30_public_signed_relay_and_mutable_rpc_abuse_are_bounded() {
    let public_limits = RpcHardeningLimits {
        request_body_limit_bytes: 128 * 1024,
        rate_limit: None,
    };
    let public_app = pulsedag_rpc::routes::router_with_profile::<TestState>(
        ApiExposureProfile::PublicSafe,
        false,
        None,
        Some(public_limits),
    )
    .with_state(test_state("public-safe"));

    for uri in ["/tx/submit", "/api/v1/tx/submit"] {
        let (status, _) = call(public_app.clone(), json_post(uri, "{}")).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "public-safe profile must not expose signed transaction submission at {uri}"
        );
    }

    let body_limited_app = pulsedag_rpc::routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        false,
        None,
        Some(RpcHardeningLimits {
            request_body_limit_bytes: 16,
            rate_limit: None,
        }),
    )
    .with_state(test_state("body-limit"));
    let oversized = Request::builder()
        .method("POST")
        .uri("/tx/submit")
        .header("content-type", "application/json")
        .header("content-length", "128")
        .body(Body::from(
            "{\"transaction\":{\"padding\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}}",
        ))
        .unwrap();
    let (status, body) = call(body_limited_app, oversized).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(body.contains("request_too_large"));

    let rate_limited_app = pulsedag_rpc::routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        false,
        None,
        Some(RpcHardeningLimits {
            request_body_limit_bytes: 1024,
            rate_limit: Some(RateLimitConfig {
                requests_per_window: 2,
                window_secs: 60,
                per_ip: false,
            }),
        }),
    )
    .with_state(test_state("rate-limit"));

    for attempt in 1..=2 {
        let (status, _) = call(rate_limited_app.clone(), json_post("/tx/submit", "{}")).await;
        assert_ne!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "attempt {attempt} must remain inside the configured mutable-RPC budget"
        );
    }

    let (status, body) = call(rate_limited_app.clone(), json_post("/tx/submit", "{}")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body.contains("rate_limited"));

    let (status, _) = call(
        rate_limited_app,
        Request::builder()
            .method("GET")
            .uri("/status")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "liveness/read-plane status must remain available after mutable-RPC rate limiting"
    );
}
