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

fn temp_db_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("pulsedag-based-app-rpc-{unique}"))
}

fn state() -> TestState {
    let path = temp_db_path();
    let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).expect("storage"));
    let chain = storage
        .load_or_init_genesis("testnet-dev".to_string())
        .expect("genesis");
    TestState {
        chain: Arc::new(RwLock::new(chain)),
        storage,
        runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
    }
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (
        status,
        String::from_utf8(body.to_vec()).expect("utf8 response"),
    )
}

#[tokio::test]
async fn based_app_state_and_events_are_wired_but_fail_closed() {
    let app =
        routes::router_with_profile::<TestState>(ApiExposureProfile::PublicSafe, false, None, None)
            .with_state(state());
    let app_id = "00".repeat(32);

    for uri in [
        format!("/api/v1/based-apps/{app_id}/state"),
        format!("/api/v1/based-apps/{app_id}/events?after=10&limit=256"),
    ] {
        let (status, body) = get(app.clone(), &uri).await;
        assert_eq!(status, StatusCode::OK, "unexpected status for {uri}");
        assert!(
            body.contains("based_app_surface_disabled"),
            "route did not fail closed: {uri}: {body}"
        );
        assert!(!body.contains("state_store_unavailable"));
        assert!(!body.contains("event_store_unavailable"));
    }
}
