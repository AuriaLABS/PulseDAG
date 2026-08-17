from pathlib import Path

paths = [
    Path('.github/workflows/v2_3_0_netns_live_rehearsal.yml'),
    Path('.github/workflows/v2_3_0_direct_peer_accounting.yml'),
]

for path in paths:
    text = path.read_text()
    if text.count('v2.2.20') != 1 or text.count('2.2.20') != 2:
        raise SystemExit(
            f'{path}: expected one v2.2.20 and two total 2.2.20 occurrences'
        )
    text = text.replace('v2.2.20', 'v2.3.0')
    text = text.replace('2.2.20', '2.3.0')
    path.write_text(text)
