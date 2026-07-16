#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one {label} match, found {count}")
    path.write_text(text.replace(old, new, 1))


def transform_section(path: Path, start: str, end: str, label: str, transform) -> None:
    text = path.read_text()
    start_count = text.count(start)
    end_count = text.count(end)
    if start_count != 1 or end_count < 1:
        raise SystemExit(
            f"{path}: invalid {label} boundaries start={start_count} end={end_count}"
        )
    begin = text.index(start)
    finish = text.index(end, begin)
    section = text[begin:finish]
    updated = transform(section)
    if updated == section:
        raise SystemExit(f"{path}: {label} transform made no change")
    path.write_text(text[:begin] + updated + text[finish:])


# The miner mutates the nonce returned by the backend. The block identifier must
# be recomputed from that final header before serializing the submit payload.
miner = ROOT / "apps/pulsedag-miner/src/main.rs"
replace_once(
    miner,
    "use pulsedag_core::types::{Block, BlockHeader};\n",
    "use pulsedag_core::types::{compute_block_hash, Block, BlockHeader};\n",
    "miner canonical hash import",
)
replace_once(
    miner,
    "#[tokio::main]\nasync fn main() -> Result<()> {\n",
    "fn apply_mined_header(block: &mut Block, mined_header: BlockHeader) {\n"
    "    block.header = mined_header;\n"
    "    block.hash = compute_block_hash(&block.header);\n"
    "}\n\n"
    "#[tokio::main]\nasync fn main() -> Result<()> {\n",
    "mined-header canonical hash helper",
)
replace_once(
    miner,
    "    let mut verified_header = block.header.clone();\n"
    "    verified_header.nonce = mining.header.nonce;\n"
    "    block.header = verified_header;\n",
    "    let mut verified_header = block.header.clone();\n"
    "    verified_header.nonce = mining.header.nonce;\n"
    "    apply_mined_header(&mut block, verified_header);\n",
    "mined nonce application",
)
replace_once(
    miner,
    "        default_worker_id, evaluate_template_freshness, loop_refresh_decision_after_outcome,\n"
    "        mining_backend, parse_args_from, should_skip_stale_submit, submit_rejection_action, usage,\n",
    "        apply_mined_header, default_worker_id, evaluate_template_freshness,\n"
    "        loop_refresh_decision_after_outcome, mining_backend, parse_args_from,\n"
    "        should_skip_stale_submit, submit_rejection_action, usage,\n",
    "miner test imports",
)
replace_once(
    miner,
    "    #[test]\n"
    "    fn submit_payload_serializes_with_template_id_and_block() {\n"
    "        let block = Block {\n"
    "            header: BlockHeader {\n"
    "                version: 1,\n"
    "                parents: vec![\"p\".into()],\n"
    "                timestamp: 1,\n"
    "                nonce: 1,\n"
    "                difficulty: 1,\n"
    "                merkle_root: \"m\".into(),\n"
    "                state_root: \"s\".into(),\n"
    "                blue_score: 1,\n"
    "                height: 1,\n"
    "            },\n"
    "            transactions: vec![],\n"
    "            hash: \"h\".into(),\n"
    "        };\n"
    "        let req = SubmitRequest {\n"
    "            template_id: \"tpl-1\".into(),\n"
    "            block,\n"
    "        };\n"
    "        let v = serde_json::to_value(&req).expect(\"serialize\");\n"
    "        assert_eq!(v[\"template_id\"], \"tpl-1\");\n"
    "        assert!(v[\"block\"].is_object());\n"
    "    }\n",
    "    #[test]\n"
    "    fn submit_payload_serializes_with_template_id_and_block() {\n"
    "        let block = Block {\n"
    "            header: BlockHeader {\n"
    "                version: 1,\n"
    "                parents: vec![\"p\".into()],\n"
    "                timestamp: 1,\n"
    "                nonce: 1,\n"
    "                difficulty: 1,\n"
    "                merkle_root: \"m\".into(),\n"
    "                state_root: \"s\".into(),\n"
    "                blue_score: 1,\n"
    "                height: 1,\n"
    "            },\n"
    "            transactions: vec![],\n"
    "            hash: \"h\".into(),\n"
    "        };\n"
    "        let req = SubmitRequest {\n"
    "            template_id: \"tpl-1\".into(),\n"
    "            block,\n"
    "        };\n"
    "        let v = serde_json::to_value(&req).expect(\"serialize\");\n"
    "        assert_eq!(v[\"template_id\"], \"tpl-1\");\n"
    "        assert!(v[\"block\"].is_object());\n"
    "    }\n\n"
    "    #[test]\n"
    "    fn nonzero_mined_nonce_recomputes_canonical_block_hash() {\n"
    "        let header = BlockHeader {\n"
    "            version: 1,\n"
    "            parents: vec![\"p\".into()],\n"
    "            timestamp: 1,\n"
    "            nonce: 0,\n"
    "            difficulty: 1,\n"
    "            merkle_root: \"m\".into(),\n"
    "            state_root: \"s\".into(),\n"
    "            blue_score: 1,\n"
    "            height: 1,\n"
    "        };\n"
    "        let template_hash = pulsedag_core::types::compute_block_hash(&header);\n"
    "        let mut block = Block { hash: template_hash.clone(), header: header.clone(), transactions: vec![] };\n"
    "        let mut mined_header = header;\n"
    "        mined_header.nonce = 1;\n"
    "        apply_mined_header(&mut block, mined_header);\n"
    "        assert_eq!(block.hash, pulsedag_core::types::compute_block_hash(&block.header));\n"
    "        assert_ne!(block.hash, template_hash);\n"
    "    }\n",
    "miner nonzero nonce regression",
)

node = ROOT / "apps/pulsedagd/src/main.rs"
replace_once(
    node,
    "const FINAL_QUIESCENCE_CLEANUP_LIMIT: usize = 64;\n",
    "const FINAL_QUIESCENCE_CLEANUP_LIMIT: usize = 64;\n"
    "const SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS: u64 = 64;\n"
    "const SELECTED_LOCATOR_PRIORITY_GRACE_SECS: u64 = 60;\n",
    "selected-segment priority constants",
)
replace_once(
    node,
    "fn final_quiescence_reconcile_pending(total: u64, success: u64, blocked: u64) -> bool {\n"
    "    total > success.saturating_add(blocked)\n"
    "}\n",
    "fn final_quiescence_reconcile_pending(total: u64, success: u64, blocked: u64) -> bool {\n"
    "    total > success.saturating_add(blocked)\n"
    "}\n\n"
    "fn selected_segment_recovery_has_priority(\n"
    "    session_active: bool,\n"
    "    pending_requested_at_unix: Option<u64>,\n"
    "    now_unix: u64,\n"
    ") -> bool {\n"
    "    session_active\n"
    "        || pending_requested_at_unix.is_some_and(|requested_at| {\n"
    "            now_unix.saturating_sub(requested_at) <= SELECTED_LOCATOR_PRIORITY_GRACE_SECS\n"
    "        })\n"
    "}\n",
    "selected-segment priority helper",
)
replace_once(
    node,
    "                .unwrap_or(32)\n"
    "                .clamp(1, 128),\n",
    "                .unwrap_or(128)\n"
    "                .clamp(1, 128),\n",
    "selected-segment closeout capacity",
)

priority_expr = (
    "{\n"
    "                                let guard = selected_segment_locator_state.lock().await;\n"
    "                                selected_segment_recovery_has_priority(\n"
    "                                    selected_segment_session.is_some(),\n"
    "                                    guard\n"
    "                                        .pending_locator\n"
    "                                        .as_ref()\n"
    "                                        .map(|pending| pending.requested_at_unix),\n"
    "                                    now_unix(),\n"
    "                                )\n"
    "                            }"
)


def patch_block_announcement(section: str) -> str:
    marker = "                            let plan = fetch_scheduler.next_requests(&known, &pending, 8);\n"
    if section.count(marker) != 1:
        raise SystemExit(f"block announcement plan matches={section.count(marker)}")
    section = section.replace(
        marker,
        "                            let selected_segment_priority = " + priority_expr + ";\n" + marker,
        1,
    )
    condition = "                                if block_requests.should_issue_getblock_for_peers(\n"
    if section.count(condition) != 1:
        raise SystemExit(f"block announcement condition matches={section.count(condition)}")
    return section.replace(
        condition,
        "                                if !selected_segment_priority\n"
        "                                    && block_requests.should_issue_getblock_for_peers(\n",
        1,
    )


transform_section(
    node,
    "                    InboundEvent::BlockAnnouncement { hash } => {\n",
    "                    InboundEvent::Block(block) => {\n",
    "block announcement selected priority",
    patch_block_announcement,
)


def patch_block_inventory(section: str) -> str:
    marker = "                        let plan = fetch_scheduler.next_requests(&known, &pending, 8);\n"
    if section.count(marker) != 1:
        raise SystemExit(f"block inventory plan matches={section.count(marker)}")
    section = section.replace(
        marker,
        "                        let selected_segment_priority = "
        + priority_expr.replace("                            ", "                        ")
        + ";\n"
        + marker,
        1,
    )
    condition = "                            if block_requests.should_issue_getblock_for_peers(\n"
    if section.count(condition) != 1:
        raise SystemExit(f"block inventory condition matches={section.count(condition)}")
    return section.replace(
        condition,
        "                            if !selected_segment_priority\n"
        "                                && block_requests.should_issue_getblock_for_peers(\n",
        1,
    )


transform_section(
    node,
    "                    InboundEvent::BlockInventory { hashes } => {\n",
    "                    InboundEvent::GetHeaders {\n",
    "block inventory selected priority",
    patch_block_inventory,
)
replace_once(
    node,
    "                            let mut missing_parent_requests_issued = 0usize;\n"
    "                            for parent in &missing_parents {\n"
    "                                if block_requests.should_issue_getblock_for_peers(\n",
    "                            let selected_segment_priority = " + priority_expr + ";\n"
    "                            let mut missing_parent_requests_issued = 0usize;\n"
    "                            for parent in &missing_parents {\n"
    "                                if !selected_segment_priority\n"
    "                                    && block_requests.should_issue_getblock_for_peers(\n",
    "missing-parent selected priority",
)
replace_once(
    node,
    "                        for tip in unknown_tips {\n",
    "                        let selected_segment_priority = "
    + priority_expr.replace("                            ", "                        ")
    + ";\n"
    "                        for tip in unknown_tips {\n",
    "tip selected priority snapshot",
)
replace_once(
    node,
    "                            let readiness = block_requests.classify_getblock_for_peers(\n",
    "                            if selected_segment_priority {\n"
    "                                let mut rt = runtime.write().await;\n"
    "                                rt.final_quiescence_selected_sync_blocked_reason =\n"
    "                                    Some(\"selected_locator_priority\".to_string());\n"
    "                                continue;\n"
    "                            }\n"
    "                            let readiness = block_requests.classify_getblock_for_peers(\n",
    "tip selected priority guard",
)

proactive = """                let proactive_selected_locator = p2p_status.as_ref().and_then(|status| {
                    status
                        .remote_selected_tip_inventory
                        .iter()
                        .filter(|remote| remote.connected && remote.direct_request_capable)
                        .filter(|remote| {
                            remote.selected_height.saturating_sub(best_height)
                                >= SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS
                        })
                        .max_by_key(|remote| remote.selected_height)
                        .map(|remote| (remote.peer_id.clone(), remote.selected_height))
                });
                if let (Some((peer_id, remote_height)), Some(p2p_handle)) =
                    (proactive_selected_locator, p2p.as_ref())
                {
                    let active_session = runtime.read().await.active_session_id.is_some();
                    let priority_already_active = {
                        let guard = selected_segment_locator_state.lock().await;
                        selected_segment_recovery_has_priority(
                            active_session,
                            guard
                                .pending_locator
                                .as_ref()
                                .map(|pending| pending.requested_at_unix),
                            now,
                        )
                    };
                    if !priority_already_active {
                        let selected_locator = {
                            let guard = chain.read().await;
                            guard
                                .dag
                                .selected_chain
                                .iter()
                                .rev()
                                .take(32)
                                .cloned()
                                .collect::<Vec<_>>()
                        };
                        let selected_limits = SelectedSegmentLimits::default();
                        let selected_locator_request_id = {
                            let guard = selected_segment_locator_state.lock().await;
                            guard.next_request_id
                        };
                        let selected_locator_requested = p2p_handle
                            .request_headers(
                                &selected_locator,
                                None,
                                selected_limits.headers_per_chunk,
                            )
                            .is_ok();
                        if selected_locator_requested {
                            let mut guard = selected_segment_locator_state.lock().await;
                            guard.next_request_id = guard.next_request_id.saturating_add(1);
                            guard.pending_locator = Some(PendingSelectedLocator {
                                request_id: selected_locator_request_id,
                                peer_id: peer_id.clone(),
                                locator: selected_locator,
                                requested_at_unix: now,
                            });
                            let mut rt = runtime.write().await;
                            rt.selected_segment_gap_blocks = rt
                                .selected_segment_gap_blocks
                                .max(remote_height.saturating_sub(best_height));
                            rt.dag_sync_selected_chain_locator_total =
                                rt.dag_sync_selected_chain_locator_total.saturating_add(1);
                            rt.selected_segment_header_requests_total =
                                rt.selected_segment_header_requests_total.saturating_add(1);
                            rt.header_requests_sent = rt.header_requests_sent.saturating_add(1);
                            rt.sync_state = "locating_common_ancestor".to_string();
                            info!(
                                peer = %peer_id,
                                local_height = best_height,
                                remote_height,
                                "large remote selected-height gap activated selected-segment priority"
                            );
                        }
                    }
                }

"""
replace_once(
    node,
    "                drop(rt);\n\n                if final_quiescence_due {\n",
    "                drop(rt);\n\n" + proactive + "                if final_quiescence_due {\n",
    "proactive large-gap selected locator",
)
replace_once(
    node,
    "    #[test]\n"
    "    fn selected_segment_validation_accepts_parent_first_chain() {\n",
    "    #[test]\n"
    "    fn selected_segment_priority_is_bounded_and_closeout_capacity_covers_gap() {\n"
    "        assert!(selected_segment_recovery_has_priority(true, None, 100));\n"
    "        assert!(selected_segment_recovery_has_priority(false, Some(80), 100));\n"
    "        assert!(!selected_segment_recovery_has_priority(false, Some(1), 100));\n"
    "        let limits = SelectedSegmentLimits::default();\n"
    "        assert!(limits.max_inflight_blocks_per_peer >= 96);\n"
    "        assert!(limits.max_inflight_blocks_per_peer <= 128);\n"
    "    }\n\n"
    "    #[test]\n"
    "    fn selected_segment_validation_accepts_parent_first_chain() {\n",
    "selected-segment priority regression",
)

print("runtime round-5 patch applied")
