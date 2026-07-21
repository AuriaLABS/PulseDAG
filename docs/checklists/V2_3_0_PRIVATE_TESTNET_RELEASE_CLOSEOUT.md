# v2.3.0 private-testnet release closeout checklist

Use this checklist to record the final private-testnet release decision for the exact v2.3.0 candidate. It does not authorize a public-testnet launch by itself.

## Status legend

- `[x]` verified from repository, workflow, or accepted evidence.
- `[ ]` not yet closed for the exact final candidate.
- Evidence from an earlier candidate may be retained below, but it does not replace an exact-SHA rerun after later repository changes.

## Verification snapshot

- Reviewed: `2026-07-21 UTC`.
- Current `main`: `f9f88c3536dccbeed92b1ae7c1eae82871458588` from PR `#775`.
- Previously packaged candidate: `629b35fe2dcf27bebfa4ac9ad51458ce255221d0`.
- PR `#775` was merged after the packaged candidate and changed active documentation, workflows, Docker surfaces, release checks, and operator entrypoints. It declared no protocol or dependency changes, but the final exact candidate and its publication evidence must still be rebound to one final SHA.
- PR `#776` now corrects the active v2.3.0 operations/recovery identity and changes `docs/release/V2_3_0_RELEASE_DECISION.md`, which requests a fresh exact-candidate workflow for the final evaluated PR head.
- Current release state remains `PENDING_FINAL_CANDIDATE_EVIDENCE`.
- `public_testnet_ready=false` and `thirty_day_public_testnet_clock_started=false` remain mandatory.

## Candidate identity

- [ ] Exact final candidate commit recorded. Current `main` is `f9f88c3536dccbeed92b1ae7c1eae82871458588`; the earlier packaged candidate `629b35fe2dcf27bebfa4ac9ad51458ce255221d0` is retained as prior evidence but is not the current post-cleanup candidate.
- [x] `VERSION=v2.3.0` and Cargo workspace version `2.3.0` verified.
- [x] Approval record and proposal SHA verified: decision `APPROVE_RELEASE_CANDIDATE`, proposal SHA `4a3d4e3df587f9bd6f438ddd7359a5148f0cff8e`, proposal merge commit `fec0b304a2544245826e5f799d9932d157818d43`.
- [ ] Candidate evidence workflow and artifact digests recorded for the exact final candidate. Previous run `29800778099` is recorded below; the post-cleanup refresh requested by PR `#776` must complete and be reviewed.
- [x] No dependency drift beyond the approved workspace version update. The exact-candidate contract passed for `629b35fe2dcf27bebfa4ac9ad51458ce255221d0`, and PR `#775` declared no protocol or dependency changes.

## Required candidate evidence

- [x] Repository hygiene and active-version surface audit pass. PR `#775` head `bb66b6d9efbf0bc65fbbceea18dd3280fd3d6af7` passed workflow run `29834891685` before being merged to `main` as `f9f88c3536dccbeed92b1ae7c1eae82871458588`.
- [x] Workspace format, locked check, all package tests, and Clippy pass. The earlier exact candidate passed run `29800778225`; the post-cleanup tree also passed pre-burn-in run `29834893035` and Lint run `29834892551`.
- [ ] P2P, lifecycle, bootstrap, observability, RPC, release, and runbook gates pass as one final-candidate bundle. Active operations, snapshot/restore, guarded drill, and snapshot+delta rebuild documentation are now candidate-scoped for v2.3.0 in PR `#776`; the refreshed gates remain to be reviewed on one exact head.
- [ ] Linux, Windows, and macOS node/miner archives are built on native runners for the exact final candidate. Previous candidate run `29800778099` passed all three native packaging jobs.
- [ ] Native smoke tests pass for each target on the exact final candidate. Previous candidate run `29800778099` passed native asset verification on Linux, Windows, and macOS.
- [ ] Every archive has a matching SHA-256 file and provenance manifest for the exact final candidate. Previous candidate evidence exists and is recorded below.
- [ ] Consolidated checksums, install verification, and provenance summary verify independently for the exact final candidate. Previous candidate consolidation job passed in run `29800778099`.
- [x] Protected private-testnet rehearsal evidence remains valid for the approved runtime candidate lineage. Accepted Task 12 candidate: `22fa09b19da2893fa73b91b198b26675bd1e6e32`; workflow run `29773225491`; artifact SHA-256 `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`; all nine mandatory phases passed; 56/56 controller checksums and the independent 55-snapshot endpoint audit passed.
- [ ] No unresolved SEV-1 consensus, storage, sync, mining, packaging, or operator-safety incident exists. The proposal identified no such unresolved defect, but a final candidate-scoped incident/waiver ledger and sign-off are still required.

## Operator and rollback readiness

- [x] Installation and checksum instructions are current for v2.3.0 in `docs/INSTALL_BINARIES_V2_3_0.md`.
- [x] Node lifecycle install/start/stop/upgrade/rollback procedures pass. Post-cleanup lifecycle workflow run `29834892286` passed.
- [x] Bootnode addresses include the complete `/p2p/<peer-id>` component. The Task 12 topology contract and post-cleanup bootstrap workflow run `29834892270` passed.
- [ ] Snapshot, restore, rebuild, and reconciliation procedures are documented and tested for the final v2.3.0 candidate. The active procedures are now documented with v2.3.0 identity, loopback RPC examples, candidate binding, evidence requirements, RTO, reconciliation, and failure criteria; a fresh candidate-scoped execution bundle is still required.
- [x] External miner operation is documented; pool logic remains outside the node.
- [ ] Release rollback owner and decision path are recorded. The lifecycle rollback path exists, but the named owner and final release decision path are not yet signed off in this checklist.

## Final private-testnet release decision

Choose exactly one only after every required exact-candidate item above is closed:

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

## Recorded prior candidate evidence

### Accepted Task 12 private-testnet rehearsal

- Candidate: `22fa09b19da2893fa73b91b198b26675bd1e6e32`.
- Workflow run: `29773225491`.
- Artifact SHA-256: `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`.
- Result: independently reviewed `GO`.

### Versioned candidate evidence before PR #775

- Candidate SHA: `629b35fe2dcf27bebfa4ac9ad51458ce255221d0`.
- Exact-candidate workflow run: `29800778099`.
- Consolidated artifact: `v2_3_0_candidate_consolidated_29800778099`.
- Consolidated artifact digest: `sha256:770c7fb5415ae6c6ec5c983162cc146f43cd63fd44afe22af2aa99cb0841c8f6`.
- Linux artifact digest: `sha256:f7acb3c8ee0817f91d4975fd3a8132cf09aba8abe5d32ee8c67a8556fad3dd6e`.
- Windows artifact digest: `sha256:05a8b4de39128d4b03ec2ba8d2207d22ef751e079501aab32fb259473d08fc27`.
- macOS artifact digest: `sha256:55af99371636f057549041905d4232726380c1516923f2226425be6131d176b8`.
- Candidate-contract artifact digest: `sha256:a0fc1389167b19f99eca714f4dac8b5264de9e50bfacfcc029e0e68e694bda51`.
- Workspace validation run: `29800778225`.
- Lint run: `29800778106`.
- Repository hygiene run: `29800778120`.
- RPC and release validation run: `29800778164`.

### Post-cleanup validation before merge of PR #775

- PR head: `bb66b6d9efbf0bc65fbbceea18dd3280fd3d6af7`.
- Merged `main`: `f9f88c3536dccbeed92b1ae7c1eae82871458588`.
- Repository hygiene: `29834891685` — success.
- Lint: `29834892551` — success.
- Pre-burn-in verification: `29834893035` — success.
- Multi-host private-testnet rehearsal contract: `29834891908` — success.
- Private-testnet bootstrap: `29834892270` — success.
- Operator and incident runbooks: `29834891702` — success.
- Node lifecycle: `29834892286` — success.
- RPC and release validation: `29834891757` — success.

## Remaining blockers before a final decision

1. Nominate and record one exact final candidate SHA after the PR `#776` changes stabilize.
2. Complete and independently review the refreshed exact-candidate native packaging, smoke, checksum, manifest, provenance, and consolidation workflow.
3. Run and retain candidate-scoped snapshot/restore/rebuild/reconciliation evidence, or record an explicit non-blocking waiver with owner, expiry, and exit criteria.
4. Record a final incident/waiver ledger proving no unresolved SEV-1 blocker.
5. Assign the release rollback owner and complete sign-off.
6. Record exactly one final private-testnet release decision.

## Sign-off

- Maintainer: ____________________
- Release owner: _________________
- Operations owner: ______________
- Decision date (UTC): ___________
- Exact final candidate SHA: ___________________________________________
- Final decision: ________________
- Blocking issue IDs, if any: ___________________________________________
