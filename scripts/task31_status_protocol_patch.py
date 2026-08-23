#!/usr/bin/env python3
from pathlib import Path

path = Path("crates/pulsedag-rpc/src/handlers/status.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    text = text.replace(old, new, 1)


replace_once(
    "    pub consensus_mode: String,\n    pub ghostdag_metadata_active: bool,\n",
    "    pub consensus_mode: String,\n    pub protocol_consensus_mode: String,\n    pub ghostdag_metadata_active: bool,\n",
    "status response protocol field",
)

replace_once(
    "fn status_from_rpc_snapshot(snapshot: NodeRpcSnapshot) -> NodeStatusData {\n",
    "fn status_from_rpc_snapshot(\n    snapshot: NodeRpcSnapshot,\n    protocol_consensus_mode: String,\n) -> NodeStatusData {\n",
    "degraded status signature",
)

replace_once(
    "        consensus_mode: pulsedag_core::ConsensusMode::Legacy.to_string(),\n        ghostdag_metadata_active: false,\n",
    "        consensus_mode: pulsedag_core::ConsensusMode::Legacy.to_string(),\n        protocol_consensus_mode,\n        ghostdag_metadata_active: false,\n",
    "degraded status protocol value",
)

replace_once(
    '''pub async fn get_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<NodeStatusData>> {
    let liveness_snapshot = fresh_or_cached_node_rpc_snapshot(&state, "/status").await;
    if liveness_snapshot.degraded || liveness_snapshot.stale {
        return Json(ApiResponse::ok(status_from_rpc_snapshot(liveness_snapshot)));
    }
''',
    '''pub async fn get_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<NodeStatusData>> {
    let protocol_consensus_mode = match state.storage().protocol_activation_record() {
        Ok(Some(record)) => record.identity.consensus_mode.to_string(),
        Ok(None) => pulsedag_core::ProtocolConsensusMode::Legacy.to_string(),
        Err(error) => return Json(ApiResponse::err("STORAGE_ERROR", error.to_string())),
    };
    let liveness_snapshot = fresh_or_cached_node_rpc_snapshot(&state, "/status").await;
    if liveness_snapshot.degraded || liveness_snapshot.stale {
        return Json(ApiResponse::ok(status_from_rpc_snapshot(
            liveness_snapshot,
            protocol_consensus_mode,
        )));
    }
''',
    "status durable protocol identity",
)

replace_once(
    "        consensus_mode: chain_snapshot.consensus_mode,\n        ghostdag_metadata_active: chain_snapshot.ghostdag_metadata_active,\n",
    "        consensus_mode: chain_snapshot.consensus_mode,\n        protocol_consensus_mode,\n        ghostdag_metadata_active: chain_snapshot.ghostdag_metadata_active,\n",
    "live status protocol value",
)

replace_once(
    "    use pulsedag_core::ChainState;\n",
    '''    use pulsedag_core::{
        genesis_v2::init_chain_state_v2, ActivatedV2P2pRuntime, ChainState,
        ProtocolActivationIdentity,
    };
''',
    "status test protocol imports",
)

anchor = '''    fn mk_state(status: P2pStatus) -> TestState {
        let path = temp_db_path("status");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let runtime = NodeRuntimeStats {
            sync_state: "idle".to_string(),
            ..NodeRuntimeStats::default()
        };
        let snapshot = build_node_rpc_snapshot(&chain, &runtime, Some(&status));
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(runtime)),
            p2p: Some(Arc::new(TestP2pHandle { status })),
            rpc_snapshot: NodeRpcSnapshotStore::new(snapshot),
        }
    }

'''
if "fn mk_activated_v2_state()" not in text:
    helper = anchor + '''    fn mk_activated_v2_state() -> TestState {
        let path = temp_db_path("status-activated-v2");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let chain = init_chain_state_v2("pulsedag-private-v2.4.0".to_string()).unwrap();
        let identity = ProtocolActivationIdentity::activated_v2(
            chain.chain_id.clone(),
            chain.dag.genesis_hash.clone(),
            chain.dag.ordering_version.clone(),
        );
        storage
            .persist_activated_v2_p2p_runtime_snapshot(
                &identity,
                &chain,
                &ActivatedV2P2pRuntime::default(),
            )
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
            p2p: None,
            rpc_snapshot: NodeRpcSnapshotStore::new(snapshot),
        }
    }

'''
    count = text.count(anchor)
    if count != 1:
        raise SystemExit(f"activated status helper: expected one anchor, found {count}")
    text = text.replace(anchor, helper, 1)

if "status_reports_protocol_identity_separately_from_internal_consensus" not in text:
    test_anchor = '''    #[tokio::test]
    async fn status_returns_while_chain_lock_is_held() {
'''
    test = '''    #[tokio::test]
    async fn status_reports_protocol_identity_separately_from_internal_consensus() {
        let Json(resp) = get_status(State(mk_activated_v2_state())).await;
        let data = resp.data.expect("activated-v2 status data should exist");

        assert!(resp.ok);
        assert_eq!(data.consensus_mode, "legacy");
        assert_eq!(data.protocol_consensus_mode, "ghostdag_v1");
        assert!(!data.high_cadence_allowed);
    }

''' + test_anchor
    count = text.count(test_anchor)
    if count != 1:
        raise SystemExit(f"activated status test: expected one anchor, found {count}")
    text = text.replace(test_anchor, test, 1)

path.write_text(text, encoding="utf-8")
