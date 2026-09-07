from pathlib import Path
import textwrap

workflow_path = Path('.github/workflows/harden-1064-mining-identity-v3.yml')
workflow = workflow_path.read_text()

heredoc_marker = "          python3 - <<'PY'\n"
start = workflow.index(heredoc_marker) + len(heredoc_marker)
end = workflow.index("\n          PY\n", start)
source = textwrap.dedent(workflow[start:end])

start_marker = '# Add a backward-compatible protocol fingerprint field to every durable\n'
end_marker = 'protocol = "crates/pulsedag-rpc/src/handlers/mining_template_protocol.rs"\n'
block_start = source.index(start_marker)
block_end = source.index(end_marker, block_start)

safe_block = '''# Add the backward-compatible field to the type definition, then edit
# only real struct literals whose first field is protocol_version. This
# deliberately does not match a function return type `-> StoredMiningTemplate {`.
legacy = Path("crates/pulsedag-rpc/src/handlers/mining_template.rs")
legacy_source = legacy.read_text()
definition = "pub struct StoredMiningTemplate {\\n    #[serde(default = \\\"default_mining_protocol_version\\\")]\\n"
replacement = "pub struct StoredMiningTemplate {\\n    #[serde(default)]\\n    pub protocol_identity_fingerprint: String,\\n    #[serde(default = \\\"default_mining_protocol_version\\\")]\\n"
if legacy_source.count(definition) != 1:
    raise SystemExit("StoredMiningTemplate definition anchor is not unique")
legacy.write_text(legacy_source.replace(definition, replacement, 1))

import re
literal_pattern = re.compile(r"(StoredMiningTemplate \\{\\n)([ \\t]+)(protocol_version:)")
literal_count = 0
for rust_path in Path("crates/pulsedag-rpc").rglob("*.rs"):
    rust_source = rust_path.read_text()

    def add_field(match):
        indent = match.group(2)
        return (
            match.group(1)
            + indent
            + "protocol_identity_fingerprint: String::new(),\\n"
            + indent
            + match.group(3)
        )

    rust_source, count = literal_pattern.subn(add_field, rust_source)
    if count:
        literal_count += count
        rust_path.write_text(rust_source)
if literal_count < 1:
    raise SystemExit(f"unexpected StoredMiningTemplate literal count: {literal_count}")

'''

source = source[:block_start] + safe_block + source[block_end:]
exec(compile(source, '<harden-1064-production>', 'exec'), {'__name__': '__main__'})
