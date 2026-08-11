from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly 1 match, found {count}")
    text = text.replace(old, new, 1)


old = '''fn selected_chain_metadata_needs_repair(chain: &pulsedag_core::ChainState) -> bool {
    let Some(selected_tip) = pulsedag_core::preferred_tip_hash(chain) else {
        return false;
    };
'''
new = '''fn deterministic_selected_parent_for_block(
    block: &pulsedag_core::Block,
    chain: &pulsedag_core::ChainState,
) -> Option<String> {
    if block.hash == chain.dag.genesis_hash {
        None
    } else if chain.dag.consensus_mode.ghostdag_metadata_active() {
        pulsedag_core::calculate_selected_parent(block, chain)
    } else {
        block
            .header
            .parents
            .iter()
            .filter(|parent| chain.dag.blocks.contains_key(*parent))
            .max()
            .cloned()
    }
}

fn selected_chain_metadata_needs_repair(chain: &pulsedag_core::ChainState) -> bool {
    let Some(selected_tip) = pulsedag_core::preferred_tip_hash(chain) else {
        return false;
    };
'''
replace_once(old, new, "add deterministic selected-parent helper")

old = '''    if chain.dag.selected_chain.windows(2).any(|window| {
        chain
            .dag
            .selected_parents
            .get(&window[1])
            .and_then(|parent| parent.as_ref())
            != Some(&window[0])
    }) {
        return true;
    }

    let retained_floor_height = chain
'''
new = '''    if chain.dag.selected_chain.windows(2).any(|window| {
        chain
            .dag
            .selected_parents
            .get(&window[1])
            .and_then(|parent| parent.as_ref())
            != Some(&window[0])
    }) {
        return true;
    }
    if chain.dag.blocks.values().any(|block| {
        chain.dag.selected_parents.get(&block.hash).cloned().flatten()
            != deterministic_selected_parent_for_block(block, chain)
    }) {
        return true;
    }

    let retained_floor_height = chain
'''
replace_once(old, new, "audit all retained selected-parent metadata")

old = '''    let mut selected_parents = HashMap::with_capacity(ordered_blocks.len());
    for block in ordered_blocks {
        let selected_parent = if block.hash == chain.dag.genesis_hash {
            None
        } else if chain.dag.consensus_mode.ghostdag_metadata_active() {
            pulsedag_core::calculate_selected_parent(&block, chain)
        } else {
            block
                .header
                .parents
                .iter()
                .filter(|parent| chain.dag.blocks.contains_key(*parent))
                .max()
                .cloned()
        };
        selected_parents.insert(block.hash, selected_parent);
    }
'''
new = '''    let mut selected_parents = HashMap::with_capacity(ordered_blocks.len());
    for block in ordered_blocks {
        let selected_parent = deterministic_selected_parent_for_block(&block, chain);
        selected_parents.insert(block.hash, selected_parent);
    }
'''
replace_once(old, new, "reuse deterministic selected-parent helper")

marker = '''    #[test]
    fn selected_chain_locator_spans_long_fresh_node_divergence() {
'''
test = '''    #[test]
    fn startup_selected_chain_metadata_repair_detects_stale_side_selected_parent() {
        let mut chain = build_test_chain("startup-side-selected-parent", 8);
        assert!(chain.dag.selected_chain.len() >= 5);
        assert!(!selected_chain_metadata_needs_repair(&chain));

        let template_hash = chain.dag.selected_chain[3].clone();
        let mut side = chain
            .dag
            .blocks
            .get(&template_hash)
            .cloned()
            .expect("selected template block");
        side.hash = "stale-side-selected-parent".to_string();
        side.header.height = side.header.height.saturating_sub(1);
        let expected_parent = deterministic_selected_parent_for_block(&side, &chain);
        assert!(expected_parent.is_some());
        let side_hash = side.hash.clone();
        chain.dag.blocks.insert(side_hash.clone(), side);
        chain.dag.selected_parents.insert(side_hash.clone(), None);

        assert_eq!(
            chain.dag.selected_chain.last(),
            pulsedag_core::preferred_tip_hash(&chain).as_ref()
        );
        assert!(selected_chain_metadata_needs_repair(&chain));
        assert!(repair_selected_chain_metadata_if_needed(&mut chain));
        assert!(!selected_chain_metadata_needs_repair(&chain));
        assert_eq!(
            chain.dag.selected_parents.get(&side_hash).cloned().flatten(),
            expected_parent
        );
    }

'''
replace_once(marker, test + marker, "add stale side selected-parent regression")

path.write_text(text, encoding="utf-8")
print("applied deterministic selected-parent startup audit")
