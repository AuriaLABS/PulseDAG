# PulseDAG v2.4.0 release decision

## Current decision

`APPROVE_TAG_AND_PUBLICATION`

The release owner has explicitly authorized publication of PulseDAG v2.4.0. This authorizes creation of the `v2.4.0` tag and GitHub Release **only after the exact versioned candidate passes the required release gates**.

This decision does **not** authorize `GO_PUBLIC_TESTNET`, public ingress, Day 0, the start/backdating of the 30-day public-testnet clock, smart-contract activation, or production/mainnet custody.

## Versioned release scope

The v2.4.0 release must:

1. set `VERSION` to `v2.4.0`;
2. set the Cargo workspace/local PulseDAG package versions to `2.4.0` without unrelated dependency upgrades;
3. keep `Cargo.lock` synchronized without external dependency drift;
4. make active documentation/release workflows identify v2.4.0;
5. include v2.4.0 install, release-note, evidence and closeout surfaces;
6. pass the required exact-SHA release validation before tag/publication.

## Pre-bump implementation baseline

Before the version-surface change, `release/2.4.0` was frozen at:

- SHA `8a1a5f74e03eae695e76bf8a84ddc9d48f94db34`;
- tree `93fbae392d43c2b309475db326f0fee6ebd6acbd`;
- all then-applicable v2.4 repository/pre-burn gates terminal green on that exact SHA.

This evidence remains implementation provenance only. The final published v2.4.0 identity is the immutable commit referenced by the `v2.4.0` tag.

## Required final release validation

Before publication, the exact tag target must pass the complete applicable release matrix, including:

- repository hygiene/version-surface audit;
- locked Cargo metadata/check, workspace tests and Clippy;
- P2P real-swarm and v2.4 private identity;
- RPC/release and public-testnet profile contract;
- wallet transaction-plan and wallet security validation;
- dependency/RustSec audit and configured warning-disposition controls;
- pre-burn-in/release build, packaging and smoke validation applicable to the software release.

No evidence from the pre-bump SHA may be relabeled as exact final-candidate evidence.

## Release authorization

- [x] `APPROVE_TAG_AND_PUBLICATION`
- [ ] `REQUEST_CHANGES`
- [ ] `NO_GO`

The immutable `v2.4.0` tag is the authoritative exact release SHA. GitHub Release assets and provenance must bind to that same tag target.

## Public-testnet boundary

Software release approval is separate from public-testnet launch authorization. The private burn-in, recovery drills, mandatory multi-node/multi-miner rehearsal, public infrastructure freeze, security disposition and separate launch-control decision remain required before Day 0.

Current launch state remains:

- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`;
- production/mainnet custody readiness is not claimed.
