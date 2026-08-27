# v2.4.0 public-testnet package templates

These files are **pre-GO templates**, not launch configuration and not a frozen public network identity.

They deliberately contain `__TASK31_FREEZE_REQUIRED__` placeholders and keep network activation fail-closed. Until issue #781 records an explicit `GO_PUBLIC_TESTNET`, the templates must retain:

- `PULSEDAG_PUBLIC_TESTNET_READY=false`
- `PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false`
- `PULSEDAG_CONTRACTS_ENABLED=false`
- `PULSEDAG_ADMIN_ENABLED=false`
- `PULSEDAG_P2P_ENABLED=false`

Do not advertise DNS names, bootnodes, public RPC, peer IDs or Day 0 from these files.

## Intended roles

- `seed.env.template`: future public seed/bootnode role.
- `node.env.template`: future ordinary public node.
- `observer.env.template`: future read-only public observer/RPC role.
- `miner.args.template`: future external miner CLI arguments. `pulsedag-miner` uses CLI flags rather than a node-style env configuration.

## Render only after the launch-control gate

After the private burn-in and 5-node/4-miner rehearsal pass on one exact release SHA, Task31/#781 must freeze and record the source SHA, chain ID, network profile, genesis/config digests, node/miner artifact digests, bootnode peer IDs/multiaddrs and public endpoint ownership.

Only then may an operator copy a template to a host-local file and replace every `__TASK31_FREEZE_REQUIRED__` placeholder with the values recorded in the launch-control evidence. P2P/public exposure remains disabled until the explicit GO is recorded.

The repository validator `scripts/validate_v2_4_0_public_hardening.py` fails if a pre-GO template loses these guardrails or accidentally contains apparent credentials/private keys.

See `docs/runbooks/V2_4_0_PUBLIC_TESTNET_PREP.md` and root `SECURITY.md`.
