from pathlib import Path

path = Path("crates/pulsedag-p2p/src/lib.rs")
text = path.read_text(encoding="utf-8")

replacements = [
    (
        "    fn oversized_inventory_is_capped() {\n",
        "    fn inventory_request_fanout_is_capped() {\n",
        "test name",
    ),
    (
        "        let hashes = (0..MAX_INV_BLOCK_HASHES + 10)\n",
        "        let hashes = (0..MAX_INV_BLOCK_REQUEST_FANOUT + 10)\n",
        "legal request-fanout fixture size",
    ),
]

for old, new, label in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one {label} anchor, found {count}")
    text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
