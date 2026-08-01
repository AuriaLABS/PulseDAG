#!/usr/bin/env python3
"""Final one-shot correction for the fake-merkle validation fixture."""

from pathlib import Path
import re

path = Path("crates/pulsedag-core/src/validation.rs")
text = path.read_text()
pattern = re.compile(
    r"(?ms)^    #\[test\]\n    fn validate_block_rejects_fake_merkle_root\(\) \{.*?^    \}\n"
)
replacement = '''    #[test]
    fn validate_block_rejects_fake_merkle_root() {
        let state = init_chain_state("test".to_string());
        let mut block = structurally_valid_block(&state);
        block.header.merkle_root = "not-the-canonical-merkle-root".to_string();
        let (header, mined, _, _) = crate::dev_mine_header(block.header.clone(), 200_000);
        assert!(mined, "expected fake-merkle fixture to satisfy consensus PoW");
        block.header = header;
        block.hash = compute_block_hash(&block.header);

        assert_invalid_block_contains(validate_block(&block, &state), "merkle root mismatch");
    }
'''
updated, count = pattern.subn(replacement, text, count=1)
if count != 1:
    raise SystemExit(f"expected one fake-merkle test, found {count}")
path.write_text(updated)
