from pathlib import Path

path = Path('apps/pulsedagd/src/main.rs')
text = path.read_text()
old = '''    let selected_segment_locator_state = Arc::new(Mutex::new(SelectedSegmentLocatorState {
        next_request_id: 1,
        pending_locator: None,
    }));
'''
new = '''    let selected_segment_locator_state = Arc::new(Mutex::new(SelectedSegmentLocatorState {
        next_request_id: 1,
        pending_locator: None,
        retained_history_gap_peers: HashSet::new(),
    }));
'''
if text.count(old) != 1:
    raise SystemExit(f'expected one SelectedSegmentLocatorState initializer, found {text.count(old)}')
path.write_text(text.replace(old, new, 1))
print('initialized retained_history_gap_peers')
