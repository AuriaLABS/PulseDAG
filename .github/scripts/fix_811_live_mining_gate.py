from pathlib import Path
import re

path = Path("crates/pulsedag-rpc/src/handlers/mining_template.rs")
text = path.read_text()

pattern = re.compile(
    r"async fn mining_template_unavailable_reason<S: RpcStateLike>\(state: &S\) -> Option<String> \{.*?\n\}\n\npub async fn post_mining_template",
    re.S,
)
replacement = '''fn mining_recovery_active(
    sync_state: &str,
    pending_missing_parents: usize,
    orphan_backlog_waiting_missing_parent: usize,
) -> bool {
    matches!(
        sync_state,
        "missing_parent" | "missing_parent_recovery" | "orphan_recovery"
    ) || pending_missing_parents > 0
        || orphan_backlog_waiting_missing_parent > 0
}

async fn mining_template_unavailable_reason<S: RpcStateLike>(state: &S) -> Option<String> {
    let (sync_state, live_sync_error) = {
        let runtime_handle = state.runtime();
        let runtime = runtime_handle.read().await;
        (
            runtime.sync_state.clone(),
            runtime.sync_pipeline.last_error.is_some(),
        )
    };

    // Mining readiness must use live chain recovery state. Runtime counters are
    // telemetry mirrors and can briefly remain stale after orphan/missing-parent
    // cleanup has completed.
    let (pending_missing_parents, orphan_backlog_waiting_missing_parent) = {
        let chain_handle = state.chain();
        let chain = chain_handle.read().await;
        let orphan_backlog = pulsedag_core::classify_orphan_backlog(&chain);
        (
            pulsedag_core::pending_missing_parent_count(&chain),
            orphan_backlog.waiting_missing_parent,
        )
    };

    if mining_recovery_active(
        &sync_state,
        pending_missing_parents,
        orphan_backlog_waiting_missing_parent,
    ) {
        return Some(format!(
            "mining template unavailable while sync_state={} missing_parent/orphan recovery is active",
            sync_state
        ));
    }
    if sync_state == "degraded" || live_sync_error {
        return Some(format!(
            "mining template unavailable while readiness snapshot is degraded: sync_state={}",
            sync_state
        ));
    }

    let status = state.p2p()?.status().ok()?;
    (status.runtime_started
        && mode_connected_peers_are_real_network(&status.mode)
        && status.connected_peers.is_empty()
        && (!status.bootnodes_configured.is_empty() || !status.listening.is_empty()))
    .then(|| {
        format!(
            "mining template unavailable while p2p is enabled with peer_count=0; diagnostics={}",
            status.asymmetric_connectivity_diagnostics.join("|")
        )
    })
}

pub async fn post_mining_template'''
text, count = pattern.subn(replacement, text, count=1)
if count != 1:
    raise SystemExit(f"mining gate replacement expected 1 match, found {count}")

anchor_pattern = re.compile(
    r"(?P<indent>\s*)#\[tokio::test\]\n(?P=indent)async fn isolated_mining_node_does_not_get_template_when_p2p_zero_peer\(\) \{"
)
match = anchor_pattern.search(text)
if not match:
    raise SystemExit("test insertion anchor not found")
indent = match.group("indent")
addition = f'''{indent}#[test]
{indent}fn live_missing_parent_state_still_blocks_mining() {{
{indent}    assert!(super::mining_recovery_active("missing_parent_recovery", 0, 0));
{indent}    assert!(super::mining_recovery_active("requesting_blocks", 1, 0));
{indent}    assert!(super::mining_recovery_active("requesting_blocks", 0, 1));
{indent}    assert!(!super::mining_recovery_active("requesting_blocks", 0, 0));
{indent}}}

{indent}#[tokio::test]
{indent}async fn stale_runtime_missing_parent_counters_do_not_block_clean_chain_template() {{
{indent}    let state = test_state_with_status(P2pStatus::default());
{indent}    {{
{indent}        let mut runtime = state.runtime.write().await;
{indent}        runtime.sync_state = "requesting_blocks".to_string();
{indent}        runtime.pending_missing_parents = 3;
{indent}        runtime.orphan_backlog_waiting_missing_parent = 2;
{indent}    }}

{indent}    let reason = mining_template_unavailable_reason(&state).await;
{indent}    assert_eq!(reason, None);
{indent}}}

'''
text = text[: match.start()] + addition + text[match.start() :]
path.write_text(text)
