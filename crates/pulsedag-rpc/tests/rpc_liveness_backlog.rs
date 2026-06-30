use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use pulsedag_core::state::ChainState;
use pulsedag_p2p::P2pHandle;
use pulsedag_rpc::{
    api::{build_node_rpc_snapshot, NodeRpcSnapshotStore, NodeRuntimeStats, RpcStateLike},
    routes,
};
use pulsedag_storage::Storage;
use serde_json::Value;
use tokio::sync::RwLock;
use tower::ServiceExt;

#[derive(Clone)]
struct TestState {
    chain: Arc<RwLock<ChainState>>,
    storage: Arc<Storage>,
    runtime: Arc<RwLock<NodeRuntimeStats>>,
    rpc_snapshot: NodeRpcSnapshotStore,
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

    fn rpc_snapshot(&self) -> NodeRpcSnapshotStore {
        self.rpc_snapshot.clone()
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
    let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
    let chain = storage
        .load_or_init_genesis("testnet-dev".to_string())
        .unwrap();
    let runtime = NodeRuntimeStats {
        sync_state: "idle".to_string(),
        ..NodeRuntimeStats::default()
    };
    let snapshot = build_node_rpc_snapshot(&chain, &runtime, None);
    TestState {
        chain: Arc::new(RwLock::new(chain)),
        storage,
        runtime: Arc::new(RwLock::new(runtime)),
        rpc_snapshot: NodeRpcSnapshotStore::new(snapshot),
    }
}

async fn get_json(state: &TestState, uri: &str) -> (StatusCode, Value) {
    let app = routes::router::<TestState>().with_state(state.clone());
    let response = tokio::time::timeout(
        Duration::from_millis(500),
        app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()),
    )
    .await
    .expect("liveness endpoint must not timeout")
    .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn rpc_liveness_endpoints_return_degraded_when_runtime_state_is_blocked() {
    let state = test_state("rpc-liveness-blocked-runtime");
    let _runtime_writer = state.runtime.write().await;

    for endpoint in [
        "/status",
        "/readiness",
        "/p2p/status",
        "/sync/status",
        "/sync/missing",
        "/orphans",
        "/metrics",
        "/release",
    ] {
        let (status, body) = get_json(&state, endpoint).await;
        assert_eq!(status, StatusCode::OK, "{endpoint} returned {body}");
        assert_eq!(body["ok"], true, "{endpoint} returned {body}");
        if endpoint != "/release" {
            assert!(
                body["data"]["rpc_response_degraded"]
                    .as_bool()
                    .unwrap_or(false)
                    || body["data"]["rpc_handler_degraded_total"]
                        .as_u64()
                        .unwrap_or(0)
                        > 0
                    || body["data"]["overall_status"] == "warn"
                    || body["data"]["overall_status"] == "fail",
                "{endpoint} should expose degraded liveness state: {body}"
            );
        }
    }
}

#[tokio::test]
async fn repeated_liveness_polling_drains_inflight_handlers_under_blocked_runtime() {
    let state = test_state("rpc-liveness-backlog-drains");
    let _runtime_writer = state.runtime.write().await;

    for _ in 0..64 {
        let (status, body) = get_json(&state, "/status").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["ok"], true, "{body}");
    }

    let (status, metrics) = get_json(&state, "/metrics").await;
    assert_eq!(status, StatusCode::OK, "{metrics}");
    assert_eq!(
        metrics["data"]["oldest_inflight_rpc_handler_age_ms"], 0,
        "liveness handlers should finish rather than accumulating indefinitely: {metrics}"
    );
    assert!(
        metrics["data"]["rpc_accept_backlog_observed"]
            .as_u64()
            .unwrap_or(0)
            < 64,
        "sequential polling should not leave an ever-growing inflight backlog: {metrics}"
    );
}
