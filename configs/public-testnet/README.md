# v2.4.x public-testnet package templates — legacy validation surface

Status: **SUPERSEDED FOR PROJECT LAUNCH PLANNING**

These files are retained for v2.4.x historical/private validation and regression coverage. They are **not** the configuration authority for the definitive PulseDAG public launch.

The active launch roadmap is `docs/ROADMAP_V3_0_0.md`: PulseDAG targets **v3.0.0** in **Q4 2026**, with **mainnet and a parallel public testnet launched in one coordinated release window**. There is no standalone public-testnet launch or 30-day public-testnet acceptance clock required before mainnet.

## Why these templates remain fail-closed

The existing v2.4.x validator expects these legacy pre-GO templates to retain:

- `PULSEDAG_PUBLIC_TESTNET_READY=false`
- `PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false`
- `PULSEDAG_CONTRACTS_ENABLED=false`
- `PULSEDAG_ADMIN_ENABLED=false`
- `PULSEDAG_P2P_ENABLED=false`
- `__TASK31_FREEZE_REQUIRED__` placeholders

These values preserve historical v2.4.x safety semantics only. They must not be interpreted as the current v3 launch model.

Do not advertise DNS names, bootnodes, public RPC, peer IDs or a launch date from these files.

## Intended historical roles

- `seed.env.template`: v2.4.x future-public-seed template.
- `node.env.template`: v2.4.x ordinary-node template.
- `observer.env.template`: v2.4.x read-only public observer/RPC template.
- `miner.args.template`: v2.4.x external miner arguments.

## Do not promote these files to v3 production

The old workflow referenced `GO_PUBLIC_TESTNET`, Day 0 and a 30-day public-testnet clock. That sequencing is superseded.

For v3.0.0:

- use `configs/v3-launch/README.md` as the placeholder configuration authority until exact identities are frozen;
- freeze separate mainnet and parallel-testnet chain IDs, network profiles, genesis hashes, bootnodes and endpoints;
- tie both networks to one exact v3.0.0 release candidate and provenance set;
- require the final `GO_V3_DUAL_LAUNCH` decision in #781;
- launch both networks in the coordinated Q4 release window.

The repository validator `scripts/validate_v2_4_0_public_hardening.py` remains a legacy compatibility check for this v2.4.x package. It is not the v3 launch gate.

See `docs/ROADMAP_V3_0_0.md`, `docs/runbooks/V3_0_0_DUAL_NETWORK_LAUNCH.md`, `configs/v3-launch/README.md` and root `SECURITY.md`.
