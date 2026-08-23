#!/usr/bin/env python3
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)


def patch(path: str, replacements: list[tuple[str, str, str]]) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    for old, new, label in replacements:
        text = replace_once(text, old, new, f"{path}: {label}")
    p.write_text(text, encoding="utf-8")


# Runtime config: bind explicit single-node release profile to the v2.4 chain
# identity and require the independent protocol selector to be ghostdag_v1.
config = Path("apps/pulsedagd/src/config.rs")
text = config.read_text(encoding="utf-8")
text = replace_once(
    text,
    '''        if self.network_profile != "private-testnet-v2.3.0" {
            bail!(
                "invalid single-node config: PULSEDAG_NETWORK_PROFILE must be private-testnet-v2.3.0"
            );
        }
        if self.chain_id != "pulsedag-private-v2.3.0" {
            bail!("invalid single-node config: PULSEDAG_CHAIN_ID must be pulsedag-private-v2.3.0");
        }
''',
    '''        if self.network_profile != "private-testnet-v2.4.0" {
            bail!(
                "invalid single-node config: PULSEDAG_NETWORK_PROFILE must be private-testnet-v2.4.0"
            );
        }
        if self.chain_id != "pulsedag-private-v2.4.0" {
            bail!("invalid single-node config: PULSEDAG_CHAIN_ID must be pulsedag-private-v2.4.0");
        }
        if std::env::var("PULSEDAG_PROTOCOL_CONSENSUS_MODE")
            .ok()
            .as_deref()
            != Some("ghostdag_v1")
        {
            bail!(
                "invalid single-node config: PULSEDAG_PROTOCOL_CONSENSUS_MODE must be ghostdag_v1"
            );
        }
        if self.auto_prune_enabled {
            bail!(
                "invalid single-node config: PULSEDAG_AUTO_PRUNE_ENABLED must be false for activated v2.4 until protocol-v2 prune/replay is validated"
            );
        }
''',
    "single-node release identity",
)
text = replace_once(
    text,
    '''            "PULSEDAG_CONSENSUS_MODE",
''',
    '''            "PULSEDAG_CONSENSUS_MODE",
            "PULSEDAG_PROTOCOL_CONSENSUS_MODE",
''',
    "test env cleanup protocol selector",
)
text = replace_once(
    text,
    '''        std::env::set_var("PULSEDAG_NETWORK_PROFILE", "private-testnet-v2.3.0");
        std::env::set_var("PULSEDAG_CHAIN_ID", "pulsedag-private-v2.3.0");
        std::env::set_var("PULSEDAG_CONSENSUS_MODE", "legacy");
''',
    '''        std::env::set_var("PULSEDAG_NETWORK_PROFILE", "private-testnet-v2.4.0");
        std::env::set_var("PULSEDAG_CHAIN_ID", "pulsedag-private-v2.4.0");
        std::env::set_var("PULSEDAG_CONSENSUS_MODE", "legacy");
        std::env::set_var("PULSEDAG_PROTOCOL_CONSENSUS_MODE", "ghostdag_v1");
        std::env::set_var("PULSEDAG_AUTO_PRUNE_ENABLED", "false");
''',
    "valid single-node test identity",
)
config.write_text(text, encoding="utf-8")

# Release/private profiles.
for path, role in [
    ("configs/private-testnet/seed.env.example", "seed"),
    ("configs/private-testnet/node.env.example", "node"),
]:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    text = text.replace("PulseDAG v2.3.0", "PulseDAG v2.4.0", 1)
    text = replace_once(text, "PULSEDAG_NETWORK_PROFILE=private-testnet-v2.3.0", "PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0", f"{role} network profile")
    text = replace_once(text, "PULSEDAG_CHAIN_ID=pulsedag-private-v2.3.0", "PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0", f"{role} chain id")
    text = replace_once(text, "PULSEDAG_CONSENSUS_MODE=legacy", "PULSEDAG_CONSENSUS_MODE=legacy\nPULSEDAG_PROTOCOL_CONSENSUS_MODE=ghostdag_v1", f"{role} protocol selector")
    text = replace_once(text, "PULSEDAG_AUTO_PRUNE_ENABLED=true", "PULSEDAG_AUTO_PRUNE_ENABLED=false", f"{role} v2 auto-prune guard")
    p.write_text(text, encoding="utf-8")

single = Path("configs/single-node/single-node.env.example")
text = single.read_text(encoding="utf-8")
text = replace_once(
    text,
    '''# PulseDAG v2.4.0 Task 14 reference profile.
# This preserves the currently approved v2.3.0 private chain identity until a
# separate release decision authorizes a version or chain-id change.
''',
    '''# PulseDAG v2.4.0 Task31 release-candidate single-node profile.
# The release protocol identity is explicit and chain-bound; this remains an
# isolated technical candidate and does not authorize public-testnet launch.
''',
    "single-node header",
)
text = replace_once(text, "PULSEDAG_NETWORK_PROFILE=private-testnet-v2.3.0", "PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0", "single-node network")
text = replace_once(text, "PULSEDAG_CHAIN_ID=pulsedag-private-v2.3.0", "PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0", "single-node chain")
text = replace_once(text, "PULSEDAG_CONSENSUS_MODE=legacy", "PULSEDAG_CONSENSUS_MODE=legacy\nPULSEDAG_PROTOCOL_CONSENSUS_MODE=ghostdag_v1", "single-node protocol selector")
text = replace_once(text, "PULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true", "PULSEDAG_AUTO_PRUNE_ENABLED=false\nPULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true", "single-node auto-prune guard")
single.write_text(text, encoding="utf-8")

# Single-node preflight and manifest.
preflight = Path("scripts/v2_4_0_single_node_preflight.sh")
text = preflight.read_text(encoding="utf-8")
text = replace_once(text, 'check "network profile is private-testnet-v2.3.0" test "${PULSEDAG_NETWORK_PROFILE:-}" = "private-testnet-v2.3.0"', 'check "network profile is private-testnet-v2.4.0" test "${PULSEDAG_NETWORK_PROFILE:-}" = "private-testnet-v2.4.0"', "single preflight network")
text = replace_once(text, 'check "chain id is pulsedag-private-v2.3.0" test "${PULSEDAG_CHAIN_ID:-}" = "pulsedag-private-v2.3.0"', 'check "chain id is pulsedag-private-v2.4.0" test "${PULSEDAG_CHAIN_ID:-}" = "pulsedag-private-v2.4.0"', "single preflight chain")
text = replace_once(text, 'check "consensus mode remains legacy" test "${PULSEDAG_CONSENSUS_MODE:-}" = "legacy"', 'check "internal consensus runtime remains legacy" test "${PULSEDAG_CONSENSUS_MODE:-}" = "legacy"\ncheck "release protocol consensus mode is ghostdag_v1" test "${PULSEDAG_PROTOCOL_CONSENSUS_MODE:-}" = "ghostdag_v1"\ncheck "activated-v2 auto-prune remains disabled" is_false "${PULSEDAG_AUTO_PRUNE_ENABLED:-false}"', "single preflight protocol")
text = replace_once(text, '  "chain_id": "${PULSEDAG_CHAIN_ID:-}",', '  "chain_id": "${PULSEDAG_CHAIN_ID:-}",\n  "protocol_consensus_mode": "${PULSEDAG_PROTOCOL_CONSENSUS_MODE:-}",\n  "auto_prune_enabled": false,', "single manifest protocol")
preflight.write_text(text, encoding="utf-8")

# Single-node regression: add explicit protocol/autoprune negative cases and evidence assertions.
test = Path("scripts/tests/test_v2_4_0_single_node_preflight.sh")
text = test.read_text(encoding="utf-8")
anchor = '''cp "$REFERENCE" "$TMP_DIR/p2p-enabled.env"
sed -i 's/^PULSEDAG_P2P_ENABLED=false/PULSEDAG_P2P_ENABLED=true/' "$TMP_DIR/p2p-enabled.env"
expect_fail "$TMP_DIR/p2p-enabled.env"

'''
insert = anchor + '''cp "$REFERENCE" "$TMP_DIR/legacy-protocol.env"
sed -i 's/^PULSEDAG_PROTOCOL_CONSENSUS_MODE=ghostdag_v1/PULSEDAG_PROTOCOL_CONSENSUS_MODE=legacy/' "$TMP_DIR/legacy-protocol.env"
expect_fail "$TMP_DIR/legacy-protocol.env"

cp "$REFERENCE" "$TMP_DIR/auto-prune.env"
sed -i 's/^PULSEDAG_AUTO_PRUNE_ENABLED=false/PULSEDAG_AUTO_PRUNE_ENABLED=true/' "$TMP_DIR/auto-prune.env"
expect_fail "$TMP_DIR/auto-prune.env"

'''
text = replace_once(text, anchor, insert, "single preflight negative cases")
text = replace_once(text, 'grep -q \'"operator_mode": "single-node"\' "$TMP_DIR/evidence/single-node-preflight.json"', 'grep -q \'"operator_mode": "single-node"\' "$TMP_DIR/evidence/single-node-preflight.json"\ngrep -q \'"protocol_consensus_mode": "ghostdag_v1"\' "$TMP_DIR/evidence/single-node-preflight.json"\ngrep -q \'"auto_prune_enabled": false\' "$TMP_DIR/evidence/single-node-preflight.json"', "single evidence assertions")
test.write_text(text, encoding="utf-8")

# Preserve historical v2.3 test semantics even though live reference profiles move to v2.4.
historical = Path("scripts/tests/test_v2_3_0_private_testnet_preflight.sh")
text = historical.read_text(encoding="utf-8")
anchor = '''cp "$ROOT_DIR/configs/private-testnet/seed.env.example" "$TMP_DIR/seed.env"
cp "$ROOT_DIR/configs/private-testnet/node.env.example" "$TMP_DIR/node.env"
expect_pass "$TMP_DIR/seed.env"
expect_pass "$TMP_DIR/node.env"
'''
replacement = '''cp "$ROOT_DIR/configs/private-testnet/seed.env.example" "$TMP_DIR/seed.env"
cp "$ROOT_DIR/configs/private-testnet/node.env.example" "$TMP_DIR/node.env"
for file in "$TMP_DIR/seed.env" "$TMP_DIR/node.env"; do
  sed -i 's/private-testnet-v2.4.0/private-testnet-v2.3.0/g' "$file"
  sed -i 's/pulsedag-private-v2.4.0/pulsedag-private-v2.3.0/g' "$file"
  sed -i '/^PULSEDAG_PROTOCOL_CONSENSUS_MODE=/d' "$file"
  sed -i 's/^PULSEDAG_AUTO_PRUNE_ENABLED=false/PULSEDAG_AUTO_PRUNE_ENABLED=true/' "$file"
done
expect_pass "$TMP_DIR/seed.env"
expect_pass "$TMP_DIR/node.env"
'''
text = replace_once(text, anchor, replacement, "historical fixture derivation")
historical.write_text(text, encoding="utf-8")

# New v2.4 multi-host preflight derived from the frozen historical parser/safety baseline.
old = Path("scripts/v2_3_0_private_testnet_preflight.sh").read_text(encoding="utf-8")
new = old.replace("v2_3_0_private_testnet_preflight.sh", "v2_4_0_private_testnet_preflight.sh")
new = new.replace("private-testnet-v2.3.0", "private-testnet-v2.4.0")
new = new.replace("pulsedag-private-v2.3.0", "pulsedag-private-v2.4.0")
new = new.replace("v2.3.0-private-testnet-bootstrap-preflight", "v2.4.0-private-testnet-bootstrap-preflight")
new = replace_once(new, 'check "consensus mode remains legacy" test "${PULSEDAG_CONSENSUS_MODE:-}" = "legacy"', 'check "internal consensus runtime remains legacy" test "${PULSEDAG_CONSENSUS_MODE:-}" = "legacy"\ncheck "release protocol consensus mode is ghostdag_v1" test "${PULSEDAG_PROTOCOL_CONSENSUS_MODE:-}" = "ghostdag_v1"\ncheck "activated-v2 auto-prune remains disabled" is_false "${PULSEDAG_AUTO_PRUNE_ENABLED:-true}"', "v2.4 multi-host protocol gates")
new = replace_once(new, '  "chain_id": "${PULSEDAG_CHAIN_ID:-}",', '  "chain_id": "${PULSEDAG_CHAIN_ID:-}",\n  "protocol_consensus_mode": "${PULSEDAG_PROTOCOL_CONSENSUS_MODE:-}",\n  "auto_prune_enabled": false,', "v2.4 multi-host manifest")
Path("scripts/v2_4_0_private_testnet_preflight.sh").write_text(new, encoding="utf-8")

old_test = Path("scripts/tests/test_v2_3_0_private_testnet_preflight.sh").read_text(encoding="utf-8")
new_test = old_test.replace("v2_3_0_private_testnet_preflight.sh", "v2_4_0_private_testnet_preflight.sh")
# Remove the synthetic historical conversion block from the new v2.4 test.
legacy_block = '''for file in "$TMP_DIR/seed.env" "$TMP_DIR/node.env"; do
  sed -i 's/private-testnet-v2.4.0/private-testnet-v2.3.0/g' "$file"
  sed -i 's/pulsedag-private-v2.4.0/pulsedag-private-v2.3.0/g' "$file"
  sed -i '/^PULSEDAG_PROTOCOL_CONSENSUS_MODE=/d' "$file"
  sed -i 's/^PULSEDAG_AUTO_PRUNE_ENABLED=false/PULSEDAG_AUTO_PRUNE_ENABLED=true/' "$file"
done
'''
new_test = replace_once(new_test, legacy_block, "", "remove v2.3 fixture conversion from v2.4 test")
new_test = new_test.replace("PASS: v2.3.0 private-testnet preflight contract", "PASS: v2.4.0 private-testnet preflight contract")
anchor = '''cp "$TMP_DIR/node.env" "$TMP_DIR/chain.env"
sed -i 's/^PULSEDAG_CHAIN_ID=.*/PULSEDAG_CHAIN_ID=wrong-chain/' "$TMP_DIR/chain.env"
expect_fail "$TMP_DIR/chain.env"

'''
insert = anchor + '''cp "$TMP_DIR/node.env" "$TMP_DIR/legacy-protocol.env"
sed -i 's/^PULSEDAG_PROTOCOL_CONSENSUS_MODE=ghostdag_v1/PULSEDAG_PROTOCOL_CONSENSUS_MODE=legacy/' "$TMP_DIR/legacy-protocol.env"
expect_fail "$TMP_DIR/legacy-protocol.env"

cp "$TMP_DIR/node.env" "$TMP_DIR/auto-prune.env"
sed -i 's/^PULSEDAG_AUTO_PRUNE_ENABLED=false/PULSEDAG_AUTO_PRUNE_ENABLED=true/' "$TMP_DIR/auto-prune.env"
expect_fail "$TMP_DIR/auto-prune.env"

'''
new_test = replace_once(new_test, anchor, insert, "v2.4 multi-host negative protocol cases")
new_test = replace_once(new_test, 'grep -q \'"public_testnet_ready": false\' "$TMP_DIR/evidence/private-testnet-preflight.json"', 'grep -q \'"public_testnet_ready": false\' "$TMP_DIR/evidence/private-testnet-preflight.json"\ngrep -q \'"protocol_consensus_mode": "ghostdag_v1"\' "$TMP_DIR/evidence/private-testnet-preflight.json"\ngrep -q \'"auto_prune_enabled": false\' "$TMP_DIR/evidence/private-testnet-preflight.json"', "v2.4 multi-host evidence assertions")
Path("scripts/tests/test_v2_4_0_private_testnet_preflight.sh").write_text(new_test, encoding="utf-8")

# Update Task14 workflow to run on the actual Task31 PR base/current version and validate both v2.4 profiles.
workflow = Path(".github/workflows/v2_4_0_single_node_profile.yml")
text = workflow.read_text(encoding="utf-8")
text = replace_once(text, "      - release/2.4.0", "      - main", "single-node workflow base")
text = replace_once(text, '''      - scripts/v2_3_0_private_testnet_preflight.sh
      - scripts/tests/test_v2_3_0_private_testnet_preflight.sh
''', '''      - scripts/v2_3_0_private_testnet_preflight.sh
      - scripts/tests/test_v2_3_0_private_testnet_preflight.sh
      - scripts/v2_4_0_private_testnet_preflight.sh
      - scripts/tests/test_v2_4_0_private_testnet_preflight.sh
''', "workflow v2.4 multi-host paths")
text = replace_once(text, 'test "$(tr -d \'\\r\\n\' < VERSION)" = "v2.3.0"\n          test "$(awk \'/^version = / {gsub(/"/, "", $3); print $3; exit}\' Cargo.toml)" = "2.3.0"', 'test "$(tr -d \'\\r\\n\' < VERSION)" = "v2.4.0"\n          test "$(awk \'/^version = / {gsub(/"/, "", $3); print $3; exit}\' Cargo.toml)" = "2.4.0"', "workflow version freeze")
text = replace_once(text, '''          bash -n scripts/v2_4_0_single_node_preflight.sh
          bash -n scripts/tests/test_v2_4_0_single_node_preflight.sh
''', '''          bash -n scripts/v2_4_0_single_node_preflight.sh
          bash -n scripts/tests/test_v2_4_0_single_node_preflight.sh
          bash -n scripts/v2_4_0_private_testnet_preflight.sh
          bash -n scripts/tests/test_v2_4_0_private_testnet_preflight.sh
''', "workflow shell syntax")
text = replace_once(text, '''      - name: Preserve v2.3.0 multi-host preflight behavior
        shell: bash
        run: bash scripts/tests/test_v2_3_0_private_testnet_preflight.sh

''', '''      - name: Preserve v2.3.0 multi-host preflight behavior
        shell: bash
        run: bash scripts/tests/test_v2_3_0_private_testnet_preflight.sh

      - name: Validate v2.4.0 multi-host release profile
        shell: bash
        run: bash scripts/tests/test_v2_4_0_private_testnet_preflight.sh

''', "workflow v2.4 multi-host test")
text = replace_once(text, '''            .operator_mode == "single-node" and
            .result == "PASS" and
''', '''            .operator_mode == "single-node" and
            .protocol_consensus_mode == "ghostdag_v1" and
            .auto_prune_enabled == false and
            .result == "PASS" and
''', "workflow manifest protocol enforcement")
workflow.write_text(text, encoding="utf-8")

# Runbook: move operator instructions to explicit v2.4 identity and preflight.
runbook = Path("docs/runbooks/V2_4_0_SINGLE_NODE_OPERATIONS.md")
text = runbook.read_text(encoding="utf-8")
text = replace_once(text, "The currently approved software version and private-chain identity remain v2.3.0 until a separate release decision authorizes a version change.", "The Task31 technical candidate uses the explicit v2.4.0 private-chain identity `pulsedag-private-v2.4.0` with protocol consensus mode `ghostdag_v1`. This is a candidate freeze only and does not authorize public-testnet launch.", "runbook identity")
text = replace_once(text, '''- persistent RocksDB storage outside `/tmp` and `/run`;
- public-testnet readiness false;
''', '''- persistent RocksDB storage outside `/tmp` and `/run`;
- `PULSEDAG_PROTOCOL_CONSENSUS_MODE=ghostdag_v1`;
- activated-v2 auto-prune disabled until protocol-v2 prune/replay is validated;
- public-testnet readiness false;
''', "runbook safety protocol")
text = replace_once(text, '''- `isolated_mining_authorized=true`;
- `public_testnet_ready=false`;
''', '''- `isolated_mining_authorized=true`;
- `protocol_consensus_mode=ghostdag_v1`;
- `auto_prune_enabled=false`;
- `public_testnet_ready=false`;
''', "runbook evidence protocol")
text = replace_once(text, "bash scripts/v2_3_0_private_testnet_preflight.sh <private-env-file>", "bash scripts/v2_4_0_private_testnet_preflight.sh <private-env-file>", "runbook multi-host preflight")
runbook.write_text(text, encoding="utf-8")
