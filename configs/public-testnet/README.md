# v3.0.0 public-testnet package templates

These files are **pre-GO templates**, not launch configuration and not a frozen public network identity.

They deliberately contain `__TASK31_FREEZE_REQUIRED__` placeholders and keep network activation fail-closed. **Current node/seed/observer templates still select legacy/ghostdag_v1, not native-v3 monetary admission. They are NOT a launch-ready native-v3 testnet profile.** A separate reviewed native-v3 identity-bound configuration and runtime must exist before public GO; do not merely flip a readiness/P2P flag. Until issue #781 records an explicit `GO_PUBLIC_TESTNET`, the templates must retain:

- `PULSEDAG_PUBLIC_TESTNET_READY=false`
- `PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false`
- `PULSEDAG_CONTRACTS_ENABLED=false`
- `PULSEDAG_ADMIN_ENABLED=false`
- `PULSEDAG_P2P_ENABLED=false`

Do not advertise DNS names, bootnodes, public RPC, peer IDs or Day 0 from these files.

## Active pre-GO template guard (offline only)

`scripts/validate_v3_0_0_public_hardening.py` now checks the **active**
`PULSEDAG_KEY=value` assignments in each unrendered seed/node/observer template,
not merely the presence of `false` text. A required disabled flag hidden in
a comment, an overridden or duplicate key, an unreviewed `PULSEDAG_` variable,
a shell `export`/substitution,
ambiguous whitespace/inline commands, a public RPC bind, a premature network
identity or a non-legacy startup mode is rejected. The restricted literal
syntax is intentional for these unrendered, non-executable safety templates.

The negative fixtures in
`scripts/tests/test_v3_public_testnet_template_semantic_guard.py`
exercise those cases on ordinary and optimized Python; the exact-head GitHub
Action also invokes the full pre-GO hardening validator. This **does not**
prove that an externally rendered configuration is authentic, safe or
authorized; future testnet-specific monetary activation needs its own exact
artifact/identity-bound validation and a recorded `GO_PUBLIC_TESTNET`.
Contracts remain **INACTIVE** regardless.

## Intended roles

- `seed.env.template`: future public seed/bootnode role.
- `node.env.template`: future ordinary public node.
- `observer.env.template`: future read-only public observer/RPC role.
- `miner.args.template`: future external miner CLI arguments. `pulsedag-miner` uses CLI flags rather than a node-style env configuration.

## Render only after the launch-control gate

Before `GO_PUBLIC_TESTNET`, Task31/#781 must freeze and record the exact source SHA, a **native-v3-only** chain ID/genesis/consensus activation/config digest, separate mainnet identity, node/miner artifacts, bootnode peer IDs/multiaddrs, public endpoint ownership, and offline/CI safety evidence. **no mandatory private live network or 5-node/4-miner rehearsal is required before public testnet.** Public testnet is the **first live native-v3 monetary mining network**; larger multi-node/fork/prune/recovery exercises take place there after its own GO.

Only after a new **native-v3** template and an implementation of testnet-only monetary activation pass exact-head CI, and #781 records `GO_PUBLIC_TESTNET`, may an operator render the verified network-specific configuration and expose approved nodes. The legacy `ghostdag_v1` examples in this directory must not be promoted into a monetary-v3 network. P2P/public exposure remains disabled before GO.

The repository validator `scripts/validate_v3_0_0_public_hardening.py` fails if a pre-GO template loses these guardrails or accidentally contains apparent credentials/private keys.

See `docs/runbooks/V3_0_0_PUBLIC_TESTNET_PREP.md` and root `SECURITY.md`.
