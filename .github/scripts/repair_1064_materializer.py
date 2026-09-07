from pathlib import Path

path = Path('.github/workflows/harden-1064-mining-identity-v3.yml')
text = path.read_text()

start_marker = '          # Add a backward-compatible protocol fingerprint field to every durable\n'
end_marker = '          protocol = "crates/pulsedag-rpc/src/handlers/mining_template_protocol.rs"\n'
start = text.index(start_marker)
end = text.index(end_marker, start)

new_block = '''          # Add the backward-compatible field to the type definition, then edit
          # only real struct literals whose first field is protocol_version. This
          # deliberately does not match a function return type `-> StoredMiningTemplate {`.
          legacy = Path("crates/pulsedag-rpc/src/handlers/mining_template.rs")
          source = legacy.read_text()
          definition = "pub struct StoredMiningTemplate {\\n    #[serde(default = \\\"default_mining_protocol_version\\\")]\\n"
          replacement = "pub struct StoredMiningTemplate {\\n    #[serde(default)]\\n    pub protocol_identity_fingerprint: String,\\n    #[serde(default = \\\"default_mining_protocol_version\\\")]\\n"
          if source.count(definition) != 1:
              raise SystemExit("StoredMiningTemplate definition anchor is not unique")
          legacy.write_text(source.replace(definition, replacement, 1))

          import re
          literal_pattern = re.compile(r"(StoredMiningTemplate \\{\\n)([ \\t]+)(protocol_version:)")
          literal_count = 0
          for rust_path in Path("crates/pulsedag-rpc").rglob("*.rs"):
              source = rust_path.read_text()
              def add_field(match):
                  indent = match.group(2)
                  return (
                      match.group(1)
                      + indent
                      + "protocol_identity_fingerprint: String::new(),\\n"
                      + indent
                      + match.group(3)
                  )
              source, count = literal_pattern.subn(add_field, source)
              if count:
                  literal_count += count
                  rust_path.write_text(source)
          if literal_count < 1:
              raise SystemExit(f"unexpected StoredMiningTemplate literal count: {literal_count}")

'''
text = text[:start] + new_block + text[end:]

old_guard = '''          changed = set(subprocess.check_output(['git', 'diff', '--name-only'], text=True).splitlines())
          if changed != expected:
              raise SystemExit(f'unexpected working-tree delta: {sorted(changed)}')'''
new_guard = '''          changed = set(subprocess.check_output(['git', 'diff', '--name-only'], text=True).splitlines())
          optional = {'crates/pulsedag-rpc/src/handlers/mining_submit.rs'}
          required = expected - optional
          if not required.issubset(changed) or changed - expected:
              raise SystemExit(f'unexpected working-tree delta: {sorted(changed)}')'''
if text.count(old_guard) != 1:
    raise SystemExit('v3 exact-delta guard anchor is not unique')
text = text.replace(old_guard, new_guard, 1)
path.write_text(text)
