from pathlib import Path


main_path = Path("apps/pulsedagd/src/main.rs")
main = main_path.read_text()
if "selected_segment_frontier_reconcile_requested" not in main:
    old = (
        "                            if selected_segment_completed {\n"
        "                                selected_segment_session = None;\n"
        "                                selected_segment_locator_state.lock().await.pending_locator = None;\n"
        "                            }\n"
    )
    new = (
        "                            if selected_segment_completed {\n"
        "                                selected_segment_session = None;\n"
        "                                selected_segment_locator_state.lock().await.pending_locator = None;\n"
        "                                if let Some(ref p2p_handle) = p2p {\n"
        "                                    if let Err(e) = p2p_handle.request_tips() {\n"
        "                                        warn!(\n"
        "                                            error = %e,\n"
        "                                            \"failed requesting DAG frontier tips after selected-segment completion\"\n"
        "                                        );\n"
        "                                    } else {\n"
        "                                        let mut rt = runtime.write().await;\n"
        "                                        rt.sync_state =\n"
        "                                            DagSyncStage::DagFrontierTips.as_str().to_string();\n"
        "                                        info!(\n"
        "                                            event = \"selected_segment_frontier_reconcile_requested\",\n"
        "                                            \"selected segment complete; requested fresh tips for lateral DAG frontier reconciliation\"\n"
        "                                        );\n"
        "                                    }\n"
        "                                }\n"
        "                            }\n"
    )
    if old not in main:
        raise SystemExit("selected-segment completion anchor not found")
    main_path.write_text(main.replace(old, new, 1))

canonical_path = Path("crates/pulsedag-rpc/src/handlers/canonical_sync.rs")
canonical = canonical_path.read_text()
if "fn active_selected_segment_gap" not in canonical:
    anchor = "fn local_selected_height(chain: &ChainState) -> u64 {"
    helper = (
        "fn active_selected_segment_gap(runtime: &NodeRuntimeStats) -> u64 {\n"
        "    let selected_recovery_active = runtime.active_session_id.is_some()\n"
        "        || matches!(\n"
        "            runtime.sync_state.as_str(),\n"
        "            \"locating_common_ancestor\"\n"
        "                | \"selected_chain_locator_sync\"\n"
        "                | \"requesting_selected_headers\"\n"
        "                | \"requesting_selected_blocks\"\n"
        "                | \"applying_selected_segment\"\n"
        "        );\n"
        "    if selected_recovery_active {\n"
        "        runtime.selected_segment_gap_blocks\n"
        "    } else {\n"
        "        0\n"
        "    }\n"
        "}\n\n"
    )
    if anchor not in canonical:
        raise SystemExit("canonical helper anchor not found")
    canonical = canonical.replace(anchor, helper + anchor, 1)

    old_gap = (
        "    let network_selected_height_gap = best_remote_selected_height\n"
        "        .unwrap_or(local_selected_height)\n"
        "        .saturating_sub(local_selected_height);\n"
    )
    new_gap = (
        "    let instantaneous_network_selected_height_gap = best_remote_selected_height\n"
        "        .unwrap_or(local_selected_height)\n"
        "        .saturating_sub(local_selected_height);\n"
        "    let network_selected_height_gap = instantaneous_network_selected_height_gap\n"
        "        .max(active_selected_segment_gap(runtime));\n"
    )
    if old_gap not in canonical:
        raise SystemExit("canonical gap anchor not found")
    canonical = canonical.replace(old_gap, new_gap, 1)

    test_anchor = (
        "    #[test]\n"
        "    fn p2p_status_remote_inventory_produces_n5_style_gap() {\n"
    )
    test = (
        "    #[test]\n"
        "    fn active_selected_segment_preserves_initial_gap_until_frontier() {\n"
        "        let chain = chain_at_selected_height(72);\n"
        "        let evidence = vec![fresh_remote(\"peer-a\", 120)];\n"
        "        let mut runtime = NodeRuntimeStats {\n"
        "            sync_state: \"requesting_selected_blocks\".into(),\n"
        "            selected_segment_gap_blocks: 112,\n"
        "            active_session_id: Some(7),\n"
        "            ..NodeRuntimeStats::default()\n"
        "        };\n"
        "        let active = build_canonical_sync_state_with_remote_evidence(\n"
        "            &chain,\n"
        "            &runtime,\n"
        "            chain.dag.blocks.len(),\n"
        "            1_000,\n"
        "            None,\n"
        "            &evidence,\n"
        "        );\n"
        "        assert_eq!(active.network_selected_height_gap, 112);\n\n"
        "        runtime.active_session_id = None;\n"
        "        runtime.sync_state = \"dag_frontier_tips_sync\".into();\n"
        "        let frontier = build_canonical_sync_state_with_remote_evidence(\n"
        "            &chain,\n"
        "            &runtime,\n"
        "            chain.dag.blocks.len(),\n"
        "            1_000,\n"
        "            None,\n"
        "            &evidence,\n"
        "        );\n"
        "        assert_eq!(frontier.network_selected_height_gap, 48);\n"
        "    }\n\n"
    )
    if test_anchor not in canonical:
        raise SystemExit("canonical test anchor not found")
    canonical = canonical.replace(test_anchor, test + test_anchor, 1)
    canonical_path.write_text(canonical)
