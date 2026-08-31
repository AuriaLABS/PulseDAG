use std::{
    net::SocketAddr,
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
    routes::{self, ApiExposureProfile, RateLimitConfig, RpcHardeningLimits},
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
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
}

fn test_state(name: &str) -> TestState {
    let path = temp_db_path(name);
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

async fn call(app: axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn api_security_coverage_v2_2_17() {
    let token = "test-operator-token-123";

    let disabled_state = test_state("api-security-disabled");
    let disabled_app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        false,
        None,
        None,
    )
    .with_state(disabled_state);
    let (status, body) = call(
        disabled_app.clone(),
        Request::builder()
            .method("GET")
            .uri("/admin/diagnostics")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("admin endpoints are disabled"));

    let enabled_state = test_state("api-security-enabled");
    let enabled_app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        true,
        Some(token.to_string()),
        None,
    )
    .with_state(enabled_state);

    let (missing_status, missing_body) = call(
        enabled_app.clone(),
        Request::builder()
            .method("GET")
            .uri("/admin/diagnostics")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
    assert!(missing_body.contains("missing_auth"));

    let (invalid_status, invalid_body) = call(
        enabled_app.clone(),
        Request::builder()
            .method("GET")
            .uri("/admin/diagnostics")
            .header("authorization", "Bearer wrong")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(invalid_status, StatusCode::FORBIDDEN);
    assert!(invalid_body.contains("invalid_auth"));

    let (valid_status, _) = call(
        enabled_app.clone(),
        Request::builder()
            .method("GET")
            .uri("/admin/diagnostics")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(valid_status, StatusCode::OK);

    let (public_status, _) = call(
        enabled_app.clone(),
        Request::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(public_status, StatusCode::OK);

    let tiny_limits = RpcHardeningLimits {
        request_body_limit_bytes: 16,
        rate_limit: None,
    };
    let tiny_state = test_state("api-security-limit");
    let tiny_app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PrivateOperator,
        true,
        Some(token.to_string()),
        Some(tiny_limits),
    )
    .with_state(tiny_state);
    let (too_large_status, too_large_body) = call(
        tiny_app,
        Request::builder()
            .method("POST")
            .uri("/admin/snapshot/create")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .header("content-length", "128")
            .body(Body::from(
                "{\"force\":true,\"padding\":\"xxxxxxxxxxxxxxxx\"}",
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(too_large_status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(too_large_body.contains("request_too_large"));

    unsafe {
        std::env::set_var("PULSEDAG_RPC_BIND", "0.0.0.0:8080");
        std::env::set_var("PULSEDAG_API_PROFILE", "local_dev");
        std::env::set_var("PULSEDAG_ADMIN_ENABLED", "true");
        std::env::set_var("PULSEDAG_OPERATOR_AUTH_TOKEN", token);
    }
    let (readiness_status, readiness_body) = call(
        enabled_app.clone(),
        Request::builder()
            .method("GET")
            .uri("/readiness")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(readiness_status, StatusCode::OK);
    assert!(
        readiness_body.contains("\"overall_status\":\"warn\"")
            || readiness_body.contains("\"overall_status\":\"fail\"")
    );
    assert!(readiness_body.contains("api_profile_safety"));
    assert!(!readiness_body.contains(token));

    let (release_status, release_body) = call(
        enabled_app,
        Request::builder()
            .method("GET")
            .uri("/release")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(release_status, StatusCode::OK);
    assert!(!release_body.contains(token));
    assert!(release_body.contains("local_dev"));

    unsafe {
        std::env::remove_var("PULSEDAG_RPC_BIND");
        std::env::remove_var("PULSEDAG_API_PROFILE");
        std::env::remove_var("PULSEDAG_ADMIN_ENABLED");
        std::env::remove_var("PULSEDAG_OPERATOR_AUTH_TOKEN");
    }
}

fn request_with_peer(method: &str, uri: &str, body: Body, peer: SocketAddr) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .body(body)
        .expect("request");
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(peer));
    request
}

#[tokio::test]
async fn public_safe_router_enforces_negative_route_body_and_liveness_contract() {
    let tiny_state = test_state("public-safe-route-contract");
    let tiny_app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PublicSafe,
        false,
        None,
        Some(RpcHardeningLimits {
            request_body_limit_bytes: 16,
            rate_limit: None,
        }),
    )
    .with_state(tiny_state);

    for path in [
        "/tx/submit",
        "/api/v1/tx/submit",
        "/tx/build",
        "/api/v1/tx/build",
        "/mine",
        "/api/v1/mine",
        "/mining/template",
        "/mining/submit",
        "/mining/jobs/claim",
        "/wallet/new",
        "/wallet/sign",
        "/wallet/transfer",
        "/admin",
        "/admin/diagnostics",
        "/snapshot/create",
        "/prune",
        "/sync/rebuild",
        "/sync/reconcile-mempool",
        "/diagnostics",
        "/operator/query-pack",
    ] {
        let (status, body) = call(
            tiny_app.clone(),
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "unexpected status for {path}"
        );
        assert!(
            body.contains("public_route_forbidden"),
            "missing code for {path}"
        );
    }

    let (oversized_status, oversized_body) = call(
        tiny_app.clone(),
        Request::builder()
            .method("POST")
            .uri("/tx/submit")
            .header("content-type", "application/json")
            .body(Body::from("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))
            .expect("request"),
    )
    .await;
    assert_eq!(oversized_status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(oversized_body.contains("request_too_large"));

    let (oversized_health_status, oversized_health_body) = call(
        tiny_app,
        Request::builder()
            .method("POST")
            .uri("/health")
            .body(Body::from("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))
            .expect("request"),
    )
    .await;
    assert_eq!(oversized_health_status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(oversized_health_body.contains("request_too_large"));

    let limited_state = test_state("public-safe-rate-contract");
    let limited_app = routes::router_with_profile::<TestState>(
        ApiExposureProfile::PublicSafe,
        false,
        None,
        Some(RpcHardeningLimits {
            request_body_limit_bytes: 128 * 1024,
            rate_limit: Some(RateLimitConfig {
                requests_per_window: 1,
                window_secs: 60,
                per_ip: true,
            }),
        }),
    )
    .with_state(limited_state);
    let peer = SocketAddr::from(([198, 51, 100, 77], 45678));

    let (first_status, _) = call(
        limited_app.clone(),
        request_with_peer("GET", "/api/v1/version", Body::empty(), peer),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);

    let (limited_status, limited_body) = call(
        limited_app.clone(),
        request_with_peer("GET", "/api/v1/version", Body::empty(), peer),
    )
    .await;
    assert_eq!(limited_status, StatusCode::TOO_MANY_REQUESTS);
    assert!(limited_body.contains("rate_limited"));

    let (health_status, _) = call(
        limited_app,
        request_with_peer("GET", "/health", Body::empty(), peer),
    )
    .await;
    assert_eq!(health_status, StatusCode::OK);
}
