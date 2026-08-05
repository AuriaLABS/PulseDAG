#!/usr/bin/env python3
from pathlib import Path

OLD_NETWORK = "private-testnet-v2.3.0"
OLD_CHAIN = "pulsedag-private-v2.3.0"
NEW_NETWORK = "private-testnet-v2.4.0"
NEW_CHAIN = "pulsedag-private-v2.4.0"
STALE_CLAIMS = (
    "preserves the currently approved v2.3.0 private chain identity",
    "v2.3.0 private chain identity until",
)

ACTIVE_WITHOUT_HISTORICAL_EXAMPLES = [
    Path("configs/private-testnet/node.env.example"),
    Path("configs/private-testnet/rehearsal.inventory.example.json"),
    Path("configs/private-testnet/seed.env.example"),
    Path("configs/private-testnet/README.md"),
    Path("configs/single-node/single-node.env.example"),
    Path("scripts/private_testnet/multi_host_rehearsal.py"),
    Path("scripts/private_testnet/netns_inventory.py"),
    Path("scripts/private_testnet/netns_nodes.sh"),
    Path("scripts/v2_4_0_single_node_preflight.sh"),
    Path("docs/ROADMAP_V2_4_0.md"),
]

for path in ACTIVE_WITHOUT_HISTORICAL_EXAMPLES:
    if not path.is_file():
        raise SystemExit(f"missing active v2.4 identity surface: {path}")
    text = path.read_text(encoding="utf-8")
    if OLD_NETWORK in text or OLD_CHAIN in text:
        raise SystemExit(f"stale v2.3 identity remains in active surface: {path}")
    lowered = text.lower()
    if any(claim.lower() in lowered for claim in STALE_CLAIMS):
        raise SystemExit(f"stale v2.3 identity claim remains in active surface: {path}")

required_pairs = [
    Path("apps/pulsedagd/src/config.rs"),
    Path("configs/private-testnet/node.env.example"),
    Path("configs/private-testnet/rehearsal.inventory.example.json"),
    Path("configs/private-testnet/seed.env.example"),
    Path("configs/single-node/single-node.env.example"),
    Path("scripts/private_testnet/multi_host_rehearsal.py"),
    Path("scripts/private_testnet/netns_inventory.py"),
    Path("scripts/private_testnet/netns_nodes.sh"),
    Path("scripts/v2_4_0_single_node_preflight.sh"),
]
for path in required_pairs:
    text = path.read_text(encoding="utf-8")
    if NEW_NETWORK not in text or NEW_CHAIN not in text:
        raise SystemExit(f"v2.4 identity pair missing from {path}")

config = Path("apps/pulsedagd/src/config.rs").read_text(encoding="utf-8")
for required in [
    "PULSEDAG_NETWORK_PROFILE must be private-testnet-v2.4.0",
    "PULSEDAG_CHAIN_ID must be pulsedag-private-v2.4.0",
    "single_node_mode_rejects_stale_v2_3_identity",
    "consensus mode must remain legacy",
]:
    if required not in config:
        raise SystemExit(f"runtime single-node contract missing: {required}")
if config.count(OLD_NETWORK) != 1 or config.count(OLD_CHAIN) != 1:
    raise SystemExit("stale v2.3 identities must appear only once each in the explicit rejection test")

preflight = Path("scripts/v2_4_0_single_node_preflight.sh").read_text(encoding="utf-8")
for required in [NEW_NETWORK, NEW_CHAIN, "consensus mode remains legacy"]:
    if required not in preflight:
        raise SystemExit(f"preflight identity contract missing: {required}")

messages = Path("crates/pulsedag-p2p/src/messages.rs").read_text(encoding="utf-8")
for required in [
    'format!("{}-blocks", chain_id)',
    'format!("{}-txs", chain_id)',
    'format!("{}-sync", chain_id)',
    "pub chain_id: String",
]:
    if required not in messages:
        raise SystemExit(f"P2P chain namespace contract missing: {required}")

operator_doc = Path("docs/V2_4_0_PRIVATE_BURN_IN_OPERATOR_PROFILE.md").read_text(encoding="utf-8")
for required in [
    "new empty RocksDB directory",
    "Do not reuse",
    "24-hour clock starts only after",
    NEW_NETWORK,
    NEW_CHAIN,
]:
    if required not in operator_doc:
        raise SystemExit(f"operator documentation missing: {required}")

print("v2.4.0 private chain identity contract: PASS")
