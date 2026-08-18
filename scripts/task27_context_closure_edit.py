from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one exact match, found {count}\n--- needle ---\n{old}")
    p.write_text(text.replace(old, new, 1))


path = "crates/pulsedag-p2p/src/messages/frontier_response_v1.rs"
replace_once(
    path,
    "    DagSyncContractError, SelectedChainLocatorV1, SelectedLocatorError,\n    MAX_SELECTED_CHAIN_SUFFIX_HASHES, P2P_DAG_SYNC_CONTRACT_VERSION,",
    "    DagSyncContractError, SelectedChainLocatorV1, SelectedLocatorError,\n    MAX_DAG_FRONTIER_REQUIRED_CONTEXT, MAX_SELECTED_CHAIN_SUFFIX_HASHES,\n    P2P_DAG_SYNC_CONTRACT_VERSION,",
)

old = '''fn collect_required_reference(
    state: &ChainState,
    selected_suffix: &BTreeSet<Hash>,
    frontier: &BTreeSet<Hash>,
    required: &mut BTreeSet<Hash>,
    referenced_hash: &Hash,
) -> Result<(), DagFrontierBuildErrorV1> {
    if selected_suffix.contains(referenced_hash) || frontier.contains(referenced_hash) {
        return Ok(());
    }
    if !state.dag.blocks.contains_key(referenced_hash) {
        return Err(DagFrontierBuildErrorV1::MissingReferencedContext {
            hash: referenced_hash.clone(),
        });
    }
    required.insert(referenced_hash.clone());
    Ok(())
}
'''
new = '''fn collect_required_reference(
    state: &ChainState,
    selected_suffix: &BTreeSet<Hash>,
    frontier: &BTreeSet<Hash>,
    required: &mut BTreeSet<Hash>,
    referenced_hash: &Hash,
) -> Result<(), DagFrontierBuildErrorV1> {
    let mut pending = vec![referenced_hash.clone()];
    while let Some(hash) = pending.pop() {
        if selected_suffix.contains(&hash) || frontier.contains(&hash) || required.contains(&hash) {
            continue;
        }
        let block = state.dag.blocks.get(&hash).ok_or_else(|| {
            DagFrontierBuildErrorV1::MissingReferencedContext { hash: hash.clone() }
        })?;
        validate_canonical_hashes(&hash, "parents", &block.header.parents)?;
        required.insert(hash.clone());
        if required.len() > MAX_DAG_FRONTIER_REQUIRED_CONTEXT {
            return Err(DagFrontierBuildErrorV1::Contract(
                DagSyncContractError::RequiredContextTooLarge {
                    observed: required.len(),
                    maximum: MAX_DAG_FRONTIER_REQUIRED_CONTEXT,
                },
            ));
        }
        for parent in block.header.parents.iter().rev() {
            if !selected_suffix.contains(parent)
                && !frontier.contains(parent)
                && !required.contains(parent)
            {
                pending.push(parent.clone());
            }
        }
    }
    Ok(())
}
'''
replace_once(path, old, new)

old = '''        assert_eq!(response.common_ancestor, "b");
        assert_eq!(response.selected_tip, "c");
        assert_eq!(response.selected_chain_suffix, vec!["b", "c"]);
        assert_eq!(response.required_context, vec!["ctx-a", "ctx-z"]);
        assert_eq!(
'''
new = '''        assert_eq!(response.common_ancestor, "b");
        assert_eq!(response.selected_tip, "c");
        assert_eq!(response.selected_chain_suffix, vec!["b", "c"]);
        let expected_context = [
            state.dag.genesis_hash.clone(),
            "ctx-a".to_string(),
            "ctx-z".to_string(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
        assert_eq!(response.required_context, expected_context);
        assert_eq!(
'''
replace_once(path, old, new)

needle = '''    #[test]
    fn response_is_deterministic_across_tip_insertion_order() {
'''
insert = '''    #[test]
    fn required_context_is_transitively_closed_over_parent_ancestry() {
        let (state, identity, locator) = fixture();
        let response = build_dag_frontier_response_v1(&identity, &locator, &state)
            .unwrap()
            .expect("retained common ancestor");
        let selected = response
            .selected_chain_suffix
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let frontier = response
            .frontier
            .iter()
            .map(|entry| entry.hash.clone())
            .collect::<BTreeSet<_>>();
        let required = response
            .required_context
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

        assert!(required.contains(&state.dag.genesis_hash));
        for hash in &required {
            let block = state
                .dag
                .blocks
                .get(hash)
                .expect("required context must reference a local block");
            for parent in &block.header.parents {
                assert!(
                    selected.contains(parent)
                        || frontier.contains(parent)
                        || required.contains(parent),
                    "required context block {hash} has unresolved parent {parent}"
                );
            }
        }
    }

    #[test]
    fn response_is_deterministic_across_tip_insertion_order() {
'''
replace_once(path, needle, insert)
