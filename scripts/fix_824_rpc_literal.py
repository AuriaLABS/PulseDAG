from pathlib import Path

path = Path("crates/pulsedag-rpc/src/handlers/canonical_sync.rs")
text = path.read_text()
old = '''                selected_tip: Some("remote-741".into()),
                selected_height: 741,
                selected_blue_score: Some(741),
'''
new = '''                selected_tip: Some("remote-741".into()),
                selected_height: 741,
                prune_boundary_height: None,
                selected_blue_score: Some(741),
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one canonical sync literal anchor, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
Path(".github/workflows/fix-824-rpc-literal.yml").unlink(missing_ok=True)
Path(__file__).unlink(missing_ok=True)
