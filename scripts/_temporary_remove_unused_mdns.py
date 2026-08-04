from pathlib import Path
import re

manifest = Path("crates/pulsedag-p2p/Cargo.toml")
text = manifest.read_text()
old = '"kad", "mdns", "macros"'
new = '"kad", "macros"'
if text.count(old) != 1:
    raise SystemExit(f"expected one mDNS feature entry, found {text.count(old)}")
manifest.write_text(text.replace(old, new, 1))

p2p = Path("crates/pulsedag-p2p/src/lib.rs")
text = p2p.read_text()
old = "        state.mdns = cfg.enable_mdns;\n"
new = "        state.mdns = false;\n"
if text.count(old) != 1:
    raise SystemExit(f"expected one mDNS status assignment, found {text.count(old)}")
p2p.write_text(text.replace(old, new, 1))

config = Path("apps/pulsedagd/src/config.rs")
text = config.read_text()
enabled_defaults = text.count("                p2p_mdns: true,\n")
if enabled_defaults < 1:
    raise SystemExit("expected at least one enabled mDNS profile default")
text = text.replace(
    "                p2p_mdns: true,\n",
    "                p2p_mdns: false,\n",
)

validation = "    fn validate_security_hardening(&self) -> Result<()> {\n"
replacement = '''    fn validate_security_hardening(&self) -> Result<()> {
        if self.p2p_mdns {
            bail!(
                "invalid config: PULSEDAG_P2P_MDNS=true is unsupported in v2.4.0; use explicit bootnodes and Kademlia discovery"
            );
        }
'''
if text.count(validation) != 1:
    raise SystemExit(f"expected one security validator, found {text.count(validation)}")
text = text.replace(validation, replacement, 1)

assertion_anchor = '''        assert_eq!(cfg.p2p_connection_slot_budget, 24);
        assert!(cfg.auto_prune_enabled);
'''
assertion_replacement = '''        assert_eq!(cfg.p2p_connection_slot_budget, 24);
        assert!(!cfg.p2p_mdns);
        assert!(cfg.auto_prune_enabled);
'''
if text.count(assertion_anchor) != 1:
    raise SystemExit("testnet profile assertion anchor not found exactly once")
text = text.replace(assertion_anchor, assertion_replacement, 1)

test_anchor = '''    #[test]
    fn loads_private_profile_defaults() {
'''
test_replacement = '''    #[test]
    fn rejects_mdns_enablement_until_a_real_behaviour_is_implemented() {
        let _guard = env_guard();
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "testnet");
        std::env::set_var("PULSEDAG_P2P_MDNS", "true");
        let error = Config::from_env().expect_err("mDNS must fail closed");
        assert!(error
            .to_string()
            .contains("PULSEDAG_P2P_MDNS=true is unsupported"));
    }

    #[test]
    fn loads_private_profile_defaults() {
'''
if text.count(test_anchor) != 1:
    raise SystemExit("private profile test anchor not found exactly once")
config.write_text(text.replace(test_anchor, test_replacement, 1))

# Cargo locks every optional dependency exposed by the libp2p umbrella crate,
# including DNS and mDNS chains that PulseDAG does not enable. cargo-audit scans
# those dormant entries, so retain the existing tested graph byte-for-byte and
# remove only the disabled optional resolution edges and their package records.
lock = Path("Cargo.lock")
lock_text = lock.read_text()
parts = lock_text.split("[[package]]")
prefix = parts[0]
blocks: list[str] = []
removed_packages: list[str] = []
removed_optional_edges = 0
patched_quinn = 0

targets = {
    "libp2p-dns",
    "libp2p-mdns",
    "hickory-resolver",
    "hickory-proto",
}

for raw in parts[1:]:
    block = "[[package]]" + raw
    name_match = re.search(r'^name = "([^"]+)"$', block, re.MULTILINE)
    version_match = re.search(r'^version = "([^"]+)"$', block, re.MULTILINE)
    if not name_match or not version_match:
        raise SystemExit("malformed Cargo.lock package block")
    name = name_match.group(1)
    version = version_match.group(1)

    if name in targets:
        removed_packages.append(name)
        continue

    if name == "libp2p" and version == "0.56.0":
        for dependency in ("libp2p-dns", "libp2p-mdns"):
            line = f' "{dependency}",\n'
            count = block.count(line)
            if count != 1:
                raise SystemExit(
                    f"expected one optional {dependency} lock edge, found {count}"
                )
            block = block.replace(line, "", 1)
            removed_optional_edges += 1

    if name == "quinn-proto" and version == "0.11.14":
        old_checksum = (
            'checksum = "434b42fec591c96ef50e21e886936e66d3cc3f737104fdb9b737c40ffb94c098"'
        )
        new_checksum = (
            'checksum = "4fcb935c5bec503c2f0e306bdd3e58bb9029dcb14fa8d9ac76e3a5256ac0763e"'
        )
        if block.count(old_checksum) != 1:
            raise SystemExit("unexpected quinn-proto 0.11.14 checksum")
        block = block.replace('version = "0.11.14"', 'version = "0.11.15"', 1)
        block = block.replace(old_checksum, new_checksum, 1)
        patched_quinn += 1

    blocks.append(block)

if set(removed_packages) != targets or len(removed_packages) != len(targets):
    raise SystemExit(
        f"unexpected optional package pruning set: {sorted(removed_packages)}"
    )
if removed_optional_edges != 2:
    raise SystemExit(f"expected two optional libp2p edges, removed {removed_optional_edges}")
if patched_quinn != 1:
    raise SystemExit(f"expected one quinn-proto patch, applied {patched_quinn}")

focused_lock = prefix + "".join(blocks)
for forbidden in targets:
    if f'name = "{forbidden}"' in focused_lock:
        raise SystemExit(f"{forbidden} remains after focused lock pruning")
lock.write_text(focused_lock)
