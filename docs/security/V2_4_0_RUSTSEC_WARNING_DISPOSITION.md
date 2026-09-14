# v2.4.0 Task31 RustSec warning disposition — historical record

Status: **historical / superseded; not an active current-lock security gate**

Historical owner: `kalekoi`

Historical review deadline: `2026-08-31 UTC` — expired.

Active dependency-security authority is now:

- issue #1127, with remaining work tracked in #1139;
- `docs/security/V3_DEPENDENCY_SECURITY.md`;
- `scripts/validate_v3_dependency_security.py`;
- `.github/workflows/dependency-audit.yml`.

This file preserves the v2.4 Task31 disposition for provenance only. It must not
be used to describe the current resolved `Cargo.lock`, to authorize a public
network, or to infer v3 security readiness.

## Historical decision boundary

The original disposition permitted only the exact Task31 **technical node +
miner candidate** and private, valueless validation. It never authorized public
exposure, public-testnet GO, Day 0, the 30-day clock, contracts, production
custody, or mainnet claims.

On the historical v2.4 dependency graph, reachable `atty 0.2.14`,
`linkme 0.2.10`, and `lru 0.12.5` were public-testnet blockers. Unsupported
transitive leaf patches were not authorized and no warning advisory was hidden
in `.cargo/audit.toml`.

## Current-line reconciliation

The active v3 dependency gate has superseded this inventory. On the current
line:

- `linkme 0.2.10` / `linkme-impl 0.2.10` are absent after the supported Rusty
  Kaspa 2.0.1 parent migration;
- `lru 0.12.5` is absent after the supported libp2p 0.56 parent migration;
- `intertrait 0.2.2` and the historical Kaspa 0.15 dependency path are absent;
- `atty 0.2.14` remains visible through the supported `hexplay`/Kaspa/workflow
  parent graph and remains an unresolved launch blocker;
- runtime `bincode 1.3.3` remains visible and has an explicit migration plan in
  `docs/security/V3_BINCODE_MIGRATION_PLAN.md`;
- Hickory 0.25.2 advisories remain visible and are permitted only while the
  active exact-candidate gate proves them compiler-unreachable from every
  launch root.

Do not reintroduce historical `linkme`/`lru` entries merely to make this record
match the current lock. The historical table below describes the old evidence,
not current dependencies.

## Historical warning inventory

| Advisory | Package | Historical v2.4 expectation | Historical disposition |
| --- | --- | --- | --- |
| `RUSTSEC-2025-0052` | `async-std 1.13.2` | node and miner | Unmaintained; private-only temporary acceptance. |
| `RUSTSEC-2024-0375` | `atty 0.2.14` | node and miner | Unmaintained; supported parent migration required. |
| `RUSTSEC-2021-0145` | `atty 0.2.14` | node and miner | Windows unsoundness; public-testnet blocker. |
| `RUSTSEC-2025-0141` | `bincode 1.3.3` | node yes; miner no | Unmaintained; storage/schema migration required. |
| `RUSTSEC-2024-0384` | `instant 0.1.13` | node and miner | Unmaintained parent-stack residue. |
| `RUSTSEC-2024-0407` | `linkme 0.2.10` | node and miner | Historical reachable unsoundness; removed on current line. |
| `RUSTSEC-2024-0436` | `paste 1.0.15` | node build artifact; miner absent | Historical build-time warning. |
| `RUSTSEC-2024-0370` | `proc-macro-error 1.0.4` | node and miner build artifacts | Historical build-time warning. |
| `RUSTSEC-2025-0010` | `ring 0.16.20` | neither node nor miner | Historical lock-only package; absent on active v3 line. |
| `RUSTSEC-2026-0002` | `lru 0.12.5` | node yes; miner no | Historical reachable unsoundness; removed on current line. |
| `RUSTSEC-2026-0253` | `lru 0.12.5` | node yes; miner no | Historical reachable soundness issue; removed on current line. |

## Historical patch-level remediation

Task31 required `crossbeam-epoch 0.9.20`, `anyhow 1.0.103`, and
`event-listener 5.4.2`, replacing the earlier vulnerable patch levels. Those
requirements remain part of historical provenance; current dependency policy is
owned by the active v3 gate.

## Historical evidence rule

Historical Task31 warning evidence is valid only for the exact SHA that produced
it. It must not be combined with current v3 evidence or interpreted as a
current-lock audit. The old Linux/Windows classification and the expired
`2026-08-31 UTC` review boundary are retained as provenance only.

The companion script
`scripts/validate_v2_4_0_rustsec_warning_disposition.py` now validates only that
this record remains explicitly historical and that the active v3 security
authority is present. It intentionally does **not** require current `Cargo.lock`
to recreate the obsolete v2.4 graph.

No warning ID is added to `.cargo/audit.toml`.

## Current launch boundary

This historical record grants no GO. Current dependency launch decisions require
fresh exact-candidate evidence from the v3 dependency-security workflow and the
final security decision in #1127/#781. `atty 0.2.14` remains unresolved; this
archive does not re-disposition or waive it.
