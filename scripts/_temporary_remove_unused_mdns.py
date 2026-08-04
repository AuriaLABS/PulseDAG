from pathlib import Path

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
