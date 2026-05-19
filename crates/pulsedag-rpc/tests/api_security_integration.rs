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
    routes::{self, ApiExposureProfile, RpcHardeningLimits},
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
        readiness_body.contains("\"status\":\"degraded\"")
            || readiness_body.contains("\"status\":\"blocked\"")
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
    assert!(release_body.contains("private_operator"));

    unsafe {
        std::env::remove_var("PULSEDAG_RPC_BIND");
        std::env::remove_var("PULSEDAG_API_PROFILE");
        std::env::remove_var("PULSEDAG_ADMIN_ENABLED");
        std::env::remove_var("PULSEDAG_OPERATOR_AUTH_TOKEN");
    }
}
