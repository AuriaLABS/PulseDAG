from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one anchor, found {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1))


main = "apps/pulsedagd/src/main.rs"
replace_once(
    main,
    "fn retained_selected_history_boundary(chain: &pulsedag_core::ChainState) -> Option<u64> {",
    "// Derive the advertised history floor from selected-parent continuity that is actually\n"
    "// present in memory. After a pruned restart, the first missing selected parent keeps\n"
    "// the boundary at the retained floor; isolated older anchors cannot make the node\n"
    "// advertise archival history accidentally.\n"
    "fn retained_selected_history_boundary(chain: &pulsedag_core::ChainState) -> Option<u64> {",
)

acceptance_tests = r'''

    #[test]
    fn retained_history_boundary_advertises_genesis_and_full_history() {
        let genesis = pulsedag_core::genesis::init_chain_state("boundary-genesis".to_string());
        assert_eq!(retained_selected_history_boundary(&genesis), Some(0));
        assert_eq!(
            local_tip_inventory_status(&genesis).prune_boundary_height,
            Some(0)
        );

        let full = build_test_chain("boundary-full-history", 5);
        assert_eq!(retained_selected_history_boundary(&full), Some(0));
        assert_eq!(
            local_tip_inventory_status(&full).prune_boundary_height,
            Some(0)
        );
    }

    #[test]
    fn retained_history_boundary_reports_contiguous_pruned_floor_with_old_anchor() {
        let mut chain = build_test_chain("boundary-pruned-history", 6);
        assert!(chain.dag.selected_chain.len() >= 5);

        let boundary_index = 3usize;
        let boundary_hash = chain.dag.selected_chain[boundary_index].clone();
        let boundary_height = chain
            .dag
            .blocks
            .get(&boundary_hash)
            .expect("retained boundary block")
            .header
            .height;
        let genesis_hash = chain.dag.genesis_hash.clone();
        let removed = chain.dag.selected_chain[1..boundary_index].to_vec();
        for hash in removed {
            assert!(chain.dag.blocks.remove(&hash).is_some());
        }

        assert!(
            chain.dag.blocks.contains_key(&genesis_hash),
            "old genesis anchor intentionally remains present"
        );
        assert!(chain.dag.blocks.contains_key(&boundary_hash));
        assert_eq!(
            retained_selected_history_boundary(&chain),
            Some(boundary_height)
        );
        assert_eq!(
            local_tip_inventory_status(&chain).prune_boundary_height,
            Some(boundary_height)
        );
    }

    #[test]
    fn retained_history_boundary_is_unknown_for_empty_selected_state() {
        let mut empty = pulsedag_core::genesis::init_chain_state("boundary-empty".to_string());
        empty.dag.blocks.clear();
        empty.dag.tips.clear();
        empty.dag.selected_chain.clear();
        empty.dag.selected_parents.clear();

        assert_eq!(retained_selected_history_boundary(&empty), None);
        assert_eq!(
            local_tip_inventory_status(&empty).prune_boundary_height,
            None
        );
    }
'''
replace_once(
    main,
    "\n}\n\n#[cfg(test)]\nmod prune_boundary_peer_selection_tests {",
    acceptance_tests + "\n}\n\n#[cfg(test)]\nmod prune_boundary_peer_selection_tests {",
)

all_incompatible_test = r'''
    #[test]
    fn prune_boundary_selection_returns_none_when_all_known_peers_are_incompatible() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("too-new-a", 500, Some(2)),
                remote("too-new-b", 700, Some(100)),
            ],
            ..Default::default()
        };
        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 0, 1, &HashSet::new()),
            None
        );

        let local = TipInventoryStatus {
            selected_height: Some(0),
            ..Default::default()
        };
        assert_eq!(
            selected_locator_peer_for_reconcile(&status, &local, &HashSet::new()),
            None
        );
    }

'''
replace_once(
    main,
    "    #[test]\n    fn prune_boundary_priority_selection_keeps_unknown_as_fallback() {",
    all_incompatible_test
    + "    #[test]\n    fn prune_boundary_priority_selection_keeps_unknown_as_fallback() {",
)

p2p = "crates/pulsedag-p2p/src/lib.rs"
old = '''        let status = &state.remote_selected_tip_inventory["peer-a"].status;
        assert_eq!(status.inventory_generation, 2);
        assert_eq!(status.selected_height, 602);
'''
new = old + '''
        let mut with_boundary = tip_inventory_for_test(3, 13);
        with_boundary.prune_boundary_height = Some(321);
        assert!(note_remote_tip_inventory(
            &mut state,
            "peer-a",
            with_boundary,
            14,
            "Tips"
        ));
        let status = &state.remote_selected_tip_inventory["peer-a"].status;
        assert_eq!(status.inventory_generation, 3);
        assert_eq!(status.prune_boundary_height, Some(321));
'''
replace_once(p2p, old, new)

Path(".github/workflows/complete-824-acceptance.yml").unlink(missing_ok=True)
Path(__file__).unlink(missing_ok=True)
