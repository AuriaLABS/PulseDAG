from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly 1 match, found {count}")
    text = text.replace(old, new, 1)


old = '''fn headers_for_request(
    chain: &pulsedag_core::state::ChainState,
    locator: &[String],
    stop_hash: Option<&String>,
    limit: usize,
) -> Vec<HeaderInventory> {
    let limit = limit.clamp(1, 512);
    let selected: HashSet<_> = chain.dag.selected_chain.iter().cloned().collect();
    let start_height = locator
        .iter()
        .filter(|hash| selected.contains(*hash))
        .filter_map(|hash| chain.dag.blocks.get(hash).map(|block| block.header.height))
        .max()
        .unwrap_or(0);
'''

new = '''fn selected_ancestor_for_locator_hash(
    chain: &pulsedag_core::state::ChainState,
    selected: &HashSet<String>,
    locator_hash: &str,
) -> Option<String> {
    let mut current = locator_hash.to_string();
    let mut visited = HashSet::new();
    while visited.insert(current.clone()) {
        if selected.contains(&current) {
            return Some(current);
        }
        current = chain
            .dag
            .selected_parents
            .get(&current)
            .and_then(|parent| parent.as_ref())?
            .clone();
    }
    None
}

fn selected_locator_start_height(
    chain: &pulsedag_core::state::ChainState,
    locator: &[String],
    selected: &HashSet<String>,
) -> u64 {
    locator
        .iter()
        .filter_map(|hash| selected_ancestor_for_locator_hash(chain, selected, hash))
        .filter_map(|hash| chain.dag.blocks.get(&hash).map(|block| block.header.height))
        .max()
        .unwrap_or(0)
}

fn headers_for_request(
    chain: &pulsedag_core::state::ChainState,
    locator: &[String],
    stop_hash: Option<&String>,
    limit: usize,
) -> Vec<HeaderInventory> {
    let limit = limit.clamp(1, 512);
    let selected: HashSet<String> = chain.dag.selected_chain.iter().cloned().collect();
    let start_height = selected_locator_start_height(chain, locator, &selected);
'''
replace_once(old, new, "resolve locator through selected parents")

marker = '''    #[test]
    fn headers_for_request_ignores_non_selected_locator_height() {
'''

tests = '''    #[test]
    fn headers_for_request_resolves_side_locator_to_selected_ancestor() {
        let mut chain = build_test_chain("headers-side-locator-ancestor", 5);
        assert!(chain.dag.selected_chain.len() >= 4);

        let selected_ancestor = chain.dag.selected_chain[1].clone();
        let expected_first = chain.dag.selected_chain[2].clone();
        let mut side = chain
            .dag
            .blocks
            .get(&expected_first)
            .cloned()
            .expect("selected block");
        side.hash = "side-dag-only-locator".to_string();
        side.header.height = 10_000;
        let side_hash = side.hash.clone();
        chain.dag.blocks.insert(side_hash.clone(), side);
        chain
            .dag
            .selected_parents
            .insert(side_hash.clone(), Some(selected_ancestor.clone()));

        assert!(!chain.dag.selected_chain.contains(&side_hash));
        assert_eq!(
            selected_ancestor_for_locator_hash(
                &chain,
                &chain.dag.selected_chain.iter().cloned().collect(),
                &side_hash,
            ),
            Some(selected_ancestor.clone())
        );

        let headers = headers_for_request(&chain, &[side_hash], None, 16);
        assert!(!headers.is_empty());
        assert_eq!(headers[0].hash, expected_first);
    }

    #[test]
    fn headers_for_request_walks_multiple_side_selected_parents() {
        let mut chain = build_test_chain("headers-side-locator-chain", 6);
        assert!(chain.dag.selected_chain.len() >= 5);

        let selected_ancestor = chain.dag.selected_chain[1].clone();
        let expected_first = chain.dag.selected_chain[2].clone();
        let template = chain
            .dag
            .blocks
            .get(&expected_first)
            .cloned()
            .expect("selected block");

        let mut side_one = template.clone();
        side_one.hash = "side-one".to_string();
        side_one.header.height = 9_000;
        let side_one_hash = side_one.hash.clone();
        chain.dag.blocks.insert(side_one_hash.clone(), side_one);
        chain
            .dag
            .selected_parents
            .insert(side_one_hash.clone(), Some(selected_ancestor.clone()));

        let mut side_two = template;
        side_two.hash = "side-two".to_string();
        side_two.header.height = 10_000;
        let side_two_hash = side_two.hash.clone();
        chain.dag.blocks.insert(side_two_hash.clone(), side_two);
        chain
            .dag
            .selected_parents
            .insert(side_two_hash.clone(), Some(side_one_hash));

        let headers = headers_for_request(&chain, &[side_two_hash], None, 16);
        assert!(!headers.is_empty());
        assert_eq!(headers[0].hash, expected_first);
    }

'''
replace_once(marker, tests + marker, "add side-locator resolver regressions")

path.write_text(text, encoding="utf-8")
print("applied #812 side-locator selected-ancestor resolver")
