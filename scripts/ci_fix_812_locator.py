from pathlib import Path
import re

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()

marker = "fn selected_headers_own_broadcast_locator(\n"
helper = r'''const SELECTED_CHAIN_LOCATOR_MAX_ENTRIES: usize = 32;
const SELECTED_CHAIN_LOCATOR_RECENT_ENTRIES: usize = 10;

fn build_selected_chain_locator(selected_chain: &[String], max_entries: usize) -> Vec<String> {
    if selected_chain.is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let last = selected_chain.len() - 1;
    let mut locator = Vec::with_capacity(max_entries.min(selected_chain.len()));
    let mut offset = 0usize;

    while offset <= last && locator.len() < max_entries {
        let index = last.saturating_sub(offset);
        locator.push(selected_chain[index].clone());
        if index == 0 {
            break;
        }
        offset = if offset < SELECTED_CHAIN_LOCATOR_RECENT_ENTRIES {
            offset.saturating_add(1)
        } else {
            offset.saturating_mul(2)
        };
        if offset == 0 {
            break;
        }
    }

    let oldest = &selected_chain[0];
    if max_entries > 1 && locator.last() != Some(oldest) {
        if locator.len() == max_entries {
            locator.pop();
        }
        locator.push(oldest.clone());
    }

    locator
}

'''
if helper.strip() not in text:
    if marker not in text:
        raise SystemExit("selected headers marker not found")
    text = text.replace(marker, helper + marker, 1)

pattern = re.compile(
    r'''guard\s*\.dag\s*\.selected_chain\s*\.iter\(\)\s*\.rev\(\)\s*\.take\(32\)\s*\.cloned\(\)\s*\.collect::<Vec<_>>\(\)''',
    re.MULTILINE,
)
text, count = pattern.subn(
    "build_selected_chain_locator(&guard.dag.selected_chain, SELECTED_CHAIN_LOCATOR_MAX_ENTRIES)",
    text,
)
if count != 5:
    raise SystemExit(f"expected 5 selected locator constructions, replaced {count}")

old = '''    let locator_heights = locator
        .iter()
        .filter_map(|hash| chain.dag.blocks.get(hash).map(|block| block.header.height))
        .collect::<Vec<_>>();
    let start_height = locator_heights.into_iter().max().unwrap_or(0);
    let selected: HashSet<_> = chain.dag.selected_chain.iter().cloned().collect();
'''
new = '''    let selected: HashSet<_> = chain.dag.selected_chain.iter().cloned().collect();
    let start_height = locator
        .iter()
        .filter(|hash| selected.contains(*hash))
        .filter_map(|hash| chain.dag.blocks.get(hash).map(|block| block.header.height))
        .max()
        .unwrap_or(0);
'''
if old not in text:
    raise SystemExit("headers_for_request locator height block not found")
text = text.replace(old, new, 1)

tests = r'''

    #[test]
    fn selected_chain_locator_spans_long_fresh_node_divergence() {
        let selected_chain = (0..=611)
            .map(|height| format!("selected-{height}"))
            .collect::<Vec<_>>();

        let locator = build_selected_chain_locator(
            &selected_chain,
            SELECTED_CHAIN_LOCATOR_MAX_ENTRIES,
        );

        assert_eq!(locator.first().map(String::as_str), Some("selected-611"));
        assert_eq!(locator.last().map(String::as_str), Some("selected-0"));
        assert!(locator.len() <= SELECTED_CHAIN_LOCATOR_MAX_ENTRIES);
        assert!(
            locator.iter().any(|hash| {
                hash.strip_prefix("selected-")
                    .and_then(|height| height.parse::<usize>().ok())
                    .is_some_and(|height| height <= 400)
            }),
            "locator must reach well beyond the previous contiguous 32-block window"
        );
    }

    #[test]
    fn headers_for_request_ignores_non_selected_locator_height() {
        let mut chain = build_test_chain("headers-selected-locator", 5);
        assert!(chain.dag.selected_chain.len() >= 4);

        let selected_ancestor = chain.dag.selected_chain[1].clone();
        let expected_first = chain.dag.selected_chain[2].clone();
        let mut side = chain
            .dag
            .blocks
            .get(&expected_first)
            .cloned()
            .expect("selected block exists");
        side.hash = "side-dag-high-locator".to_string();
        side.header.height = 10_000;
        let side_hash = side.hash.clone();
        chain.dag.blocks.insert(side_hash.clone(), side);

        let headers = headers_for_request(
            &chain,
            &[side_hash, selected_ancestor],
            None,
            16,
        );

        assert!(!headers.is_empty(), "selected ancestor must win over side-DAG locator height");
        assert_eq!(headers[0].hash, expected_first);
    }
'''

if "fn selected_chain_locator_spans_long_fresh_node_divergence()" not in text:
    end = text.rfind("\n}")
    if end < 0:
        raise SystemExit("test module closing brace not found")
    text = text[:end] + tests + text[end:]

path.write_text(text)
