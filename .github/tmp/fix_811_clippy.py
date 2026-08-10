from pathlib import Path

p = Path('apps/pulsedagd/src/main.rs')
s = p.read_text()
old = 'fn selected_segment_request_order(headers: &[HeaderInventory], limit: usize) -> Vec<String> {'
new = '#[cfg(test)]\nfn selected_segment_request_order(headers: &[HeaderInventory], limit: usize) -> Vec<String> {'
if s.count(old) != 1:
    raise SystemExit(f'expected one helper, found {s.count(old)}')
p.write_text(s.replace(old, new, 1))
print('marked test-only selected_segment_request_order')
