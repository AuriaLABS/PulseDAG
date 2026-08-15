from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()
old = ".map_or(true, |boundary| boundary <= local_height.saturating_add(1))"
new = ".is_none_or(|boundary| boundary <= local_height.saturating_add(1))"
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected exactly one #824 Clippy anchor, found {count}")
path.write_text(text.replace(old, new, 1))
Path(".github/workflows/fix-824-clippy.yml").unlink(missing_ok=True)
Path(__file__).unlink(missing_ok=True)
