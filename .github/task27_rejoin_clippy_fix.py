from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()

old = '''        let mut status = P2pStatus::default();
        status.remote_selected_tip_inventory = vec![
            remote("legacy-high", 500),
            remote("v2-b", 220),
            remote("v2-a", 220),
            remote("v2-low", 210),
        ];'''
new = '''        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("legacy-high", 500),
                remote("v2-b", 220),
                remote("v2-a", 220),
                remote("v2-low", 210),
            ],
            ..P2pStatus::default()
        };'''

count = text.count(old)
if count != 1:
    raise SystemExit(f"expected exactly one Clippy test pattern, found {count}")

path.write_text(text.replace(old, new, 1))
