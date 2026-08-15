from pathlib import Path


def replace_count(path: str, old: str, new: str, expected: int) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} matches, found {count}")
    p.write_text(text.replace(old, new))

replace_count(
    "crates/pulsedag-p2p/src/messages.rs",
    "            selected_height: Some(741),\n            selected_blue_score: Some(741),",
    "            selected_height: Some(741),\n            prune_boundary_height: None,\n            selected_blue_score: Some(741),",
    1,
)
replace_count(
    "crates/pulsedag-p2p/src/lib.rs",
    "            selected_height: Some(600 + generation),\n            selected_blue_score: Some(700 + generation),",
    "            selected_height: Some(600 + generation),\n            prune_boundary_height: None,\n            selected_blue_score: Some(700 + generation),",
    1,
)
replace_count(
    "apps/pulsedagd/src/main.rs",
    "            selected_height: Some(128),\n            selected_blue_score: Some(128),",
    "            selected_height: Some(128),\n            prune_boundary_height: None,\n            selected_blue_score: Some(128),",
    2,
)
Path(".github/workflows/fix-824-literals.yml").unlink(missing_ok=True)
Path("scripts/fix_824_literals.py").unlink(missing_ok=True)
