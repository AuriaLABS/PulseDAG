# v2.3.0 private-testnet release closeout checklist

Use this checklist to record the final private-testnet release decision for the exact v2.3.0 candidate. It does not authorize a public-testnet launch by itself.

## Candidate identity

- [ ] Exact candidate commit recorded.
- [ ] `VERSION=v2.3.0` and Cargo workspace version `2.3.0` verified.
- [ ] Approval record and proposal SHA verified.
- [ ] Candidate evidence workflow and artifact digests recorded.
- [ ] No dependency drift beyond the approved workspace version update.

## Required candidate evidence

- [ ] Repository hygiene and active-version surface audit pass.
- [ ] Workspace format, locked check, all package tests, and Clippy pass.
- [ ] P2P, lifecycle, bootstrap, observability, RPC, release, and runbook gates pass.
- [ ] Linux, Windows, and macOS node/miner archives are built on native runners.
- [ ] Native smoke tests pass for each target.
- [ ] Every archive has a matching SHA-256 file and provenance manifest.
- [ ] Consolidated checksums, install verification, and provenance summary verify independently.
- [ ] Protected private-testnet rehearsal evidence remains valid for the approved candidate lineage.
- [ ] No unresolved SEV-1 consensus, storage, sync, mining, packaging, or operator-safety incident exists.

## Operator and rollback readiness

- [ ] Installation and checksum instructions are current for v2.3.0.
- [ ] Node lifecycle install/start/stop/upgrade/rollback procedures pass.
- [ ] Bootnode addresses include the complete `/p2p/<peer-id>` component.
- [ ] Snapshot, restore, rebuild, and reconciliation procedures are documented and tested.
- [ ] External miner operation is documented; pool logic remains outside the node.
- [ ] Release rollback owner and decision path are recorded.

## Final private-testnet release decision

Choose exactly one:

- [ ] `APPROVE_TAG_AND_PUBLICATION`
- [ ] `REQUEST_CHANGES`
- [ ] `NO_GO`

The decision record must include:

- maintainer and UTC date;
- exact candidate SHA;
- workflow and artifact identities;
- rationale;
- unresolved risks;
- rollback conditions;
- explicit tag and publication authorization values.

## Public-testnet boundary

A private-testnet release approval does not automatically authorize public-testnet launch.

Before any public-testnet launch:

- [ ] a separate launch decision is recorded;
- [ ] `public_testnet_ready=true` is explicitly authorized;
- [ ] operators and public infrastructure are confirmed ready;
- [ ] the 30-day clock anchor is defined as the first public-testnet launch.

The 30-day clock must not start or be backdated before the first public-testnet launch. Smart contracts remain blocked until at least 30 contiguous days of accepted public-testnet evidence have been reviewed.

## Sign-off

- Maintainer: ____________________
- Release owner: _________________
- Operations owner: ______________
- Decision date (UTC): ___________
- Final decision: ________________
- Blocking issue IDs, if any: ___________________________________________
