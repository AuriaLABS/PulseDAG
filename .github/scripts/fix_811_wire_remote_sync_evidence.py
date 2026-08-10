from pathlib import Path

path = Path("crates/pulsedag-rpc/src/handlers/sync.rs")
s = path.read_text()


def replace_once(old: str, new: str, label: str) -> None:
    global s
    count = s.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match, found {count}")
    s = s.replace(old, new, 1)


replace_once(
    "use super::canonical_sync::build_canonical_sync_state;\n",
    "use super::canonical_sync::{\n"
    "    build_canonical_sync_state_with_remote_evidence, remote_sync_evidence_from_p2p_status,\n"
    "    CanonicalSyncState,\n"
    "};\n",
    "canonical sync imports",
)

needle = '''fn cache_sync_status_response(data: &SyncStatusData) {
    if let Ok(mut cache) = SYNC_STATUS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *cache = Some(data.clone());
    }
}
'''
helper = needle + '''
fn build_sync_status_canonical_state(
    chain: &pulsedag_core::ChainState,
    runtime: &NodeRuntimeStats,
    persisted_block_count: usize,
    now_unix: u64,
    p2p_status: Option<&pulsedag_p2p::P2pStatus>,
) -> CanonicalSyncState {
    let remote_evidence = remote_sync_evidence_from_p2p_status(p2p_status, now_unix);
    build_canonical_sync_state_with_remote_evidence(
        chain,
        runtime,
        persisted_block_count,
        now_unix,
        p2p_status.and_then(|status| status.selected_sync_peer.clone()),
        &remote_evidence,
    )
}
'''
replace_once(needle, helper, "sync status canonical helper")

old_call = '''    let canonical_sync = build_canonical_sync_state(
        &chain,
        &runtime,
        persisted_block_count,
        now_unix,
        p2p_status
            .as_ref()
            .and_then(|snapshot| snapshot.status.selected_sync_peer.clone()),
    );
'''
new_call = '''    let canonical_sync = build_sync_status_canonical_state(
        &chain,
        &runtime,
        persisted_block_count,
        now_unix,
        p2p_status.as_ref().map(|snapshot| &snapshot.status),
    );
'''
replace_once(old_call, new_call, "sync status canonical call")

replace_once(
    "    use super::{get_sync_missing, get_sync_status};\n",
    "    use super::{build_sync_status_canonical_state, get_sync_missing, get_sync_status};\n",
    "test imports",
)

needle_test = '''    #[tokio::test]
    async fn sync_status_derives_catchup_stage_and_recovery_reason_coherently() {
'''
new_test = '''    #[test]
    fn sync_status_canonical_state_uses_fresh_remote_tip_inventory() {
        let chain = pulsedag_core::genesis::init_chain_state("testnet-dev".to_string());
        let runtime = NodeRuntimeStats::default();
        let status = pulsedag_p2p::P2pStatus {
            chain_id: "testnet-dev".to_string(),
            connected_peers: vec!["peer-a".to_string()],
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "peer-a".to_string(),
                connection_generation: 1,
                chain_id: "testnet-dev".to_string(),
                selected_tip: Some("remote-3".to_string()),
                selected_height: 3,
                selected_blue_score: Some(3),
                ordered_dag_tip: Some("ordered-3".to_string()),
                state_root_digest: Some("root-3".to_string()),
                observed_at_unix: 1_000,
                inventory_generation: 1,
                age_secs: 0,
                direct_request_capable: true,
                connected: true,
            }],
            ..pulsedag_p2p::P2pStatus::default()
        };

        let canonical = build_sync_status_canonical_state(
            &chain,
            &runtime,
            chain.dag.blocks.len(),
            1_000,
            Some(&status),
        );

        assert_eq!(canonical.best_remote_selected_height, Some(3));
        assert_eq!(canonical.network_selected_height_gap, 3);
        assert_eq!(canonical.sync_state, "locating_common_ancestor");
        assert_eq!(canonical.catchup_stage, "discovering");
        assert_ne!(canonical.sync_state, "synced");
    }

''' + needle_test
replace_once(needle_test, new_test, "remote inventory wiring regression")

path.write_text(s)
