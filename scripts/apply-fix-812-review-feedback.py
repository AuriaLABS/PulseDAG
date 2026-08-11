from pathlib import Path
import re


def sub_once(text: str, pattern: str, repl: str, label: str) -> str:
    out, count = re.subn(pattern, repl, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise SystemExit(f"{label} count={count}")
    return out


main_path = Path("apps/pulsedagd/src/main.rs")
main = main_path.read_text()

# P2-1: do not issue the next selected-header page from a hash that is only
# pending/in-flight. The continuation anchor itself must already exist in the
# locally accepted DAG set.
main = sub_once(
    main,
    r"fn selected_header_discovery_continuation_anchor\(\n"
    r"\s*session: Option<&SelectedSegmentSession>,\n"
    r"\s*headers: &\[HeaderInventory\],\n"
    r"\s*selected_requests: &\[String\],\n"
    r"\) -> Option<String> \{\n"
    r"\s*if !selected_requests\.is_empty\(\) \{\n"
    r"\s*return None;\n"
    r"\s*\}\n"
    r"\s*let session = session\?;\n"
    r"\s*let furthest = headers\.iter\(\)\.max_by\(\|a, b\| \{\n"
    r"\s*a\.header\n"
    r"\s*\.height\n"
    r"\s*\.cmp\(&b\.header\.height\)\n"
    r"\s*\.then_with\(\|\| a\.hash\.cmp\(&b\.hash\)\)\n"
    r"\s*\}\)\?;\n"
    r"\s*\(furthest\.header\.height < session\.remote_selected_height\)"
    r"\.then\(\|\| furthest\.hash\.clone\(\)\)\n"
    r"\}",
    """fn selected_header_discovery_continuation_anchor(
    session: Option<&SelectedSegmentSession>,
    headers: &[HeaderInventory],
    selected_requests: &[String],
    known_blocks: &HashSet<String>,
) -> Option<String> {
    if !selected_requests.is_empty() {
        return None;
    }
    let session = session?;
    let furthest = headers.iter().max_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.hash.cmp(&b.hash))
    })?;
    if !known_blocks.contains(&furthest.hash) {
        return None;
    }
    (furthest.header.height < session.remote_selected_height).then(|| furthest.hash.clone())
}""",
    "continuation helper",
)

main = sub_once(
    main,
    r"(selected_header_discovery_continuation_anchor\(\n"
    r"\s*selected_segment_session\.as_ref\(\),\n"
    r"\s*&headers,\n"
    r"\s*&selected_requests,\n)"
    r"(\s*\))",
    r"\1                                &known,\n\2",
    "continuation production call",
)

# P2-2: retained-history-gap classification is selected-chain logic. A taller
# side-DAG block must not mask a missing bridge above the selected tip.
main = sub_once(
    main,
    r"fn selected_locator_peer_for_priority_gap\(",
    """fn local_selected_tip_height(chain: &pulsedag_core::ChainState) -> u64 {
    chain
        .dag
        .selected_chain
        .last()
        .and_then(|hash| chain.dag.blocks.get(hash))
        .map(|block| block.header.height)
        .unwrap_or(chain.dag.best_height)
}

fn selected_locator_peer_for_priority_gap(""",
    "selected-height helper insertion",
)
main = sub_once(
    main,
    r"\(known, common, height, guard\.dag\.best_height\)",
    "(known, common, height, local_selected_tip_height(&guard))",
    "selected height use",
)

# P2-3: canonical status must depend on durable retained-gap state rather than
# whichever diagnostic string happened to be written last.
main = sub_once(
    main,
    r"(\n\s*)let mut rt = runtime\.write\(\)\.await;\n"
    r"(\s*)if issued_selected_header_continuation \{",
    r"\1let retained_history_gap_peer_count = {\n"
    r"\2    let guard = selected_segment_locator_state.lock().await;\n"
    r"\2    guard.retained_history_gap_peers.len()\n"
    r"\2};\n"
    r"\2let mut rt = runtime.write().await;\n"
    r"\2rt.selected_segment_retained_history_gap_peer_count =\n"
    r"\2    retained_history_gap_peer_count;\n"
    r"\2if issued_selected_header_continuation {",
    "runtime gap count mirror",
)
main = sub_once(
    main,
    r"(locator_state\.pending_locator = None;\n"
    r"\s*locator_state\.retained_history_gap_peers\.clear\(\);\n"
    r"\s*\})"
    r"(\n\s*if let Some\(ref p2p_handle\) = p2p \{)",
    r"\1\n"
    r"                                {\n"
    r"                                    let mut rt = runtime.write().await;\n"
    r"                                    rt.selected_segment_retained_history_gap_peer_count = 0;\n"
    r"                                }\2",
    "runtime gap count clear",
)

# Existing continuation regression now states which page hashes are accepted.
main = sub_once(
    main,
    r"session\.update_remote_target\(Some\(\"remote-700\"\), 700\);\n\n"
    r"\s*assert_eq!\(\n"
    r"\s*selected_header_discovery_continuation_anchor\(Some\(&session\), &headers, &\[\]\),\n"
    r"\s*Some\(\"b3\"\.to_string\(\)\)\n"
    r"\s*\);",
    """session.update_remote_target(Some("remote-700"), 700);
        let known = HashSet::from([
            "b1".to_string(),
            "b2".to_string(),
            "b3".to_string(),
        ]);

        assert_eq!(
            selected_header_discovery_continuation_anchor(Some(&session), &headers, &[], &known),
            Some("b3".to_string())
        );""",
    "existing continuation regression",
)

main = sub_once(
    main,
    r"    #\[test\]\n\s*fn unrelated_header_page_cannot_hijack_pending_selected_locator\(\) \{",
    """    #[test]
    fn selected_header_discovery_waits_for_pending_tail_before_continuing() {
        let headers = vec![
            selected_test_header("b1", "common", 1),
            selected_test_header("b2", "b1", 2),
            selected_test_header("b3", "b2", 3),
        ];
        let locator = vec!["common".to_string()];
        let mut session = SelectedSegmentSession::new(
            1,
            "peer-a".to_string(),
            "common".to_string(),
            0,
            &headers,
            &locator,
            1,
            100,
        )
        .expect("session");
        session.update_remote_target(Some("remote-700"), 700);
        let known = HashSet::from(["b1".to_string(), "b2".to_string()]);
        assert_eq!(
            selected_header_discovery_continuation_anchor(Some(&session), &headers, &[], &known),
            None
        );
    }

    #[test]
    fn unrelated_header_page_cannot_hijack_pending_selected_locator() {""",
    "pending-tail regression insertion",
)

main = sub_once(
    main,
    r"    #\[test\]\n\s*fn selected_segment_common_ancestor_prefers_pending_locator_parent\(\) \{",
    """    #[test]
    fn retained_gap_uses_selected_tip_height_not_taller_side_height() {
        let mut chain = pulsedag_core::genesis::init_chain_state("testnet-dev".to_string());
        let genesis = chain.dag.genesis_hash.clone();
        let selected = test_orphan("selected-5", vec![genesis.as_str()], 5);
        chain.dag.blocks.insert(selected.hash.clone(), selected.clone());
        chain.dag.selected_chain.push(selected.hash.clone());
        chain.dag.best_height = 10;
        let pending = PendingSelectedLocator {
            request_id: 1,
            peer_id: "peer-a".to_string(),
            locator: vec![selected.hash.clone()],
            requested_at_unix: 100,
            retained_history_gap: false,
        };
        let headers = vec![selected_test_header("remote-7", "pruned-6", 7)];
        assert_eq!(local_selected_tip_height(&chain), 5);
        assert!(selected_headers_indicate_retained_history_gap(
            &headers,
            local_selected_tip_height(&chain),
            Some(&pending),
            Some("peer-a"),
            None,
            false,
        ));
        assert!(!selected_headers_indicate_retained_history_gap(
            &headers,
            chain.dag.best_height,
            Some(&pending),
            Some("peer-a"),
            None,
            false,
        ));
    }

    #[test]
    fn selected_segment_common_ancestor_prefers_pending_locator_parent() {""",
    "selected-tip-height regression insertion",
)
main_path.write_text(main)

api_path = Path("crates/pulsedag-rpc/src/api.rs")
api = api_path.read_text()
api = sub_once(
    api,
    r"(\s*#\[serde\(default\)\]\n\s*pub selected_segment_uncorrelated_headers_total: u64,\n)"
    r"(\s*#\[serde\(default\)\]\n\s*pub active_session_id: Option<u64>,)",
    r"\1    #[serde(default)]\n"
    r"    pub selected_segment_retained_history_gap_peer_count: usize,\n"
    r"\2",
    "runtime stats dedicated gap field",
)
api_path.write_text(api)

canonical_path = Path("crates/pulsedag-rpc/src/handlers/canonical_sync.rs")
canonical = canonical_path.read_text()
canonical = sub_once(
    canonical,
    r"let retained_history_gap_active = runtime\s*"
    r"\.final_quiescence_selected_sync_blocked_reason\s*"
    r"\.as_deref\(\)\s*"
    r"== Some\(\"retained_history_gap\"\);",
    """let retained_history_gap_active =
        runtime.selected_segment_retained_history_gap_peer_count > 0;""",
    "canonical dedicated gap state",
)
canonical = sub_once(
    canonical,
    r"sync_state: \"selected_segment_failed\"\.into\(\),\n\s*"
    r"final_quiescence_selected_sync_blocked_reason: Some\(\"retained_history_gap\"\.into\(\)\),",
    """sync_state: "selected_segment_failed".into(),
            selected_segment_retained_history_gap_peer_count: 1,
            final_quiescence_selected_sync_blocked_reason: Some(
                "selected_segment_no_progress_rearm".into(),
            ),""",
    "canonical overwritten-reason regression",
)
canonical_path.write_text(canonical)

print("applied all three #815 P2 review fixes")
