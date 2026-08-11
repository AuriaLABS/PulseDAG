from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match, found {count}: {old[:120]!r}")
    text = text.replace(old, new, 1)


replace_once(
    "    collections::{BTreeMap, HashSet},\n",
    "    collections::{BTreeMap, HashMap, HashSet},\n",
)

helper = r'''
fn selected_chain_metadata_needs_repair(chain: &pulsedag_core::ChainState) -> bool {
    let Some(selected_tip) = pulsedag_core::preferred_tip_hash(chain) else {
        return false;
    };
    if chain.dag.selected_chain.last() != Some(&selected_tip) {
        return true;
    }
    if chain
        .dag
        .selected_chain
        .iter()
        .any(|hash| !chain.dag.blocks.contains_key(hash))
    {
        return true;
    }
    if chain.dag.selected_chain.windows(2).any(|window| {
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
        .dag
        .blocks
        .values()
        .map(|block| block.header.height)
        .min()
        .unwrap_or_default();
    let selected_tip_height = chain
        .dag
        .blocks
        .get(&selected_tip)
        .map(|block| block.header.height)
        .unwrap_or_default();
    selected_tip_height > retained_floor_height && chain.dag.selected_chain.len() < 2
}

fn repair_selected_chain_metadata_if_needed(chain: &mut pulsedag_core::ChainState) -> bool {
    if !selected_chain_metadata_needs_repair(chain) {
        return false;
    }

    let mut ordered_blocks = chain.dag.blocks.values().cloned().collect::<Vec<_>>();
    ordered_blocks.sort_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.hash.cmp(&b.hash))
    });

    let mut selected_parents = HashMap::with_capacity(ordered_blocks.len());
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

    chain.dag.selected_parents = selected_parents;
    pulsedag_core::refresh_selected_chain(chain);
    true
}

'''
replace_once(
    "const SELECTED_CHAIN_LOCATOR_MAX_ENTRIES: usize = 32;\n",
    helper + "const SELECTED_CHAIN_LOCATOR_MAX_ENTRIES: usize = 32;\n",
)

startup_old = r'''    chain_state.dag.selected_parent_policy = if cfg.consensus_mode.ghostdag_metadata_active() {
        pulsedag_core::SelectedParentPolicy::GhostdagInspired
    } else {
        pulsedag_core::SelectedParentPolicy::LegacyTip
    };
    let startup_persisted_max_height = persisted_blocks
'''
startup_new = r'''    chain_state.dag.selected_parent_policy = if cfg.consensus_mode.ghostdag_metadata_active() {
        pulsedag_core::SelectedParentPolicy::GhostdagInspired
    } else {
        pulsedag_core::SelectedParentPolicy::LegacyTip
    };
    let startup_selected_chain_len_before = chain_state.dag.selected_chain.len();
    if repair_selected_chain_metadata_if_needed(&mut chain_state) {
        if selected_chain_metadata_needs_repair(&chain_state) {
            anyhow::bail!(
                "selected-chain metadata remains incoherent after deterministic startup repair"
            );
        }
        storage.persist_chain_state(&chain_state)?;
        let selected_tip = pulsedag_core::preferred_tip_hash(&chain_state)
            .unwrap_or_else(|| chain_state.dag.genesis_hash.clone());
        let selected_chain_len_after = chain_state.dag.selected_chain.len();
        warn!(
            event = "startup_selected_chain_metadata_repaired",
            selected_chain_len_before = startup_selected_chain_len_before,
            selected_chain_len_after,
            best_height = chain_state.dag.best_height,
            selected_tip = %selected_tip,
            "repaired persisted selected-chain metadata before p2p sync"
        );
        let _ = storage.append_runtime_event(
            "warn",
            "startup_selected_chain_metadata_repaired",
            &format!(
                "before_len={} after_len={} best_height={} selected_tip={}",
                startup_selected_chain_len_before,
                selected_chain_len_after,
                chain_state.dag.best_height,
                selected_tip
            ),
        );
    }
    let startup_persisted_max_height = persisted_blocks
'''
replace_once(startup_old, startup_new)

test = r'''
    #[test]
    fn startup_selected_chain_metadata_repair_restores_locator_from_empty_snapshot_metadata() {
        let mut chain = build_test_chain("testnet", 10);
        let selected_tip = pulsedag_core::preferred_tip_hash(&chain).expect("selected tip");
        assert!(chain.dag.selected_chain.len() > 1);

        chain.dag.selected_chain.clear();
        chain.dag.selected_parents.clear();
        assert!(selected_chain_metadata_needs_repair(&chain));
        assert!(repair_selected_chain_metadata_if_needed(&mut chain));
        assert!(!selected_chain_metadata_needs_repair(&chain));
        assert_eq!(chain.dag.selected_chain.last(), Some(&selected_tip));
        assert!(chain.dag.selected_chain.len() > 1);

        let locator =
            build_selected_chain_locator(&chain.dag.selected_chain, SELECTED_CHAIN_LOCATOR_MAX_ENTRIES);
        assert_eq!(locator.first(), Some(&selected_tip));
        assert!(locator.contains(&chain.dag.genesis_hash));
        assert!(locator
            .iter()
            .all(|hash| chain.dag.blocks.contains_key(hash)));
        assert!(!repair_selected_chain_metadata_if_needed(&mut chain));
    }

'''
replace_once(
    "    #[test]\n    fn selected_chain_locator_spans_long_fresh_node_divergence() {\n",
    test + "    #[test]\n    fn selected_chain_locator_spans_long_fresh_node_divergence() {\n",
)

path.write_text(text, encoding="utf-8")
