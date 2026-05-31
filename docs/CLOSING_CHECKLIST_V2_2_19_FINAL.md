# v2.2.19 Final Closeout Evidence Checklist

This checklist is the required closeout path for `v2.2.19`.

**Decision posture:** `v2.2.19` remains **pre-public-testnet hardening**. This checklist does **not** authorize public testnet launch, does **not** claim `v2.3.0` readiness, and does **not** claim `v3.0` readiness.

> Rule: do not mark PASS unless the evidence path exists and is reproducible. Default status is PENDING.

## Evidence location conventions (expected)

Use these exact evidence locations when creating closeout artifacts:

- Preflight: `artifacts/v2_2_19/preflight/`
- Cargo check/test/clippy: `artifacts/v2_2_19/validation/`
- Local 3N/1M smoke: `artifacts/v2_2_19/local_3n_1m_smoke/`
- Private 5N/1M baseline rehearsal: `artifacts/v2_2_19/private_5n_1m_rehearsal/`
- Private 5N/2M intermediate rehearsal: `artifacts/v2_2_19/private_5n_2m_rehearsal/`
- Private 5N/4M stress rehearsal: `artifacts/v2_2_19/private_5n_4m_rehearsal/`
- Staged convergence gate aggregate: `artifacts/v2_2_19/staged_convergence_gates/`
- Release binaries workflow: `artifacts/v2_2_19/release_workflow/`
- GPU scaffold/fallback: `artifacts/v2_2_19/gpu_fallback/`
- Readiness/release metadata: `artifacts/v2_2_19/preflight/`
- Snapshot/restore (if available): `artifacts/v2_2_19/snapshot_restore/`
- Final decision record: `artifacts/v2_2_19/closeout_decision/`

## Version sanity

- [x] PASS / [ ] PENDING: `VERSION` is exactly `v2.2.19`. Evidence: uploaded Ubuntu run reports `VERSION=v2.2.19` at commit `5bc26be416e358a7370741d949191b24173a9ca6`. Path target: `artifacts/v2_2_19/preflight/version.txt`
- [x] PASS / [ ] PENDING: workspace crate versions are exactly `2.2.19`. Evidence: uploaded Ubuntu run reports Cargo workspace version `2.2.19`. Path target: `artifacts/v2_2_19/preflight/cargo_workspace_versions.txt`
- [ ] PASS / [x] PENDING: `docs/VERSION_MATRIX.md` remains aligned with `v2.2.19` hardening status (no public-testnet readiness claim). Evidence path: `artifacts/v2_2_19/closeout_decision/version_matrix_alignment.md`

## Cargo.lock sanity

- [ ] PASS / [x] PENDING: `Cargo.lock` is present and consistent with `--locked` commands. Evidence path: `artifacts/v2_2_19/validation/cargo_lock_locked_commands.log`
- [ ] PASS / [x] PENDING: no unexplained `Cargo.lock` drift after validation commands. Evidence path: `artifacts/v2_2_19/validation/cargo_lock_git_diff.txt`

## Formatting/check/clippy/tests

Run and attach logs for all required commands:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

- [x] PASS / [ ] PENDING: required validation commands succeeded on Ubuntu evidence (`cargo check`, `cargo test`, `cargo clippy`; release build also PASS). Path target: `artifacts/v2_2_19/validation/cargo_validation_suite.log`

## Release workflow validation

Run and attach output:

```bash
bash scripts/v2_2_19_preflight_check.sh
OUT_DIR=/tmp/pulsedag-v2-2-19-preflight bash scripts/v2_2_19_preflight_check.sh
```

If `rg` is unavailable in your environment, force the grep fallback by shadowing `rg` in `PATH`:

```bash
TMP_BIN=$(mktemp -d)
printf "#!/usr/bin/env bash\nexit 127\n" > "$TMP_BIN/rg" && chmod +x "$TMP_BIN/rg"
PATH="$TMP_BIN:$PATH" OUT_DIR=/tmp/pulsedag-v2-2-19-preflight-grep bash scripts/v2_2_19_preflight_check.sh
```

- [x] PASS / [ ] PENDING: preflight script result is PASS `12/12` in uploaded Ubuntu evidence. Path target: `artifacts/v2_2_19/preflight/preflight_check.log`
- [ ] PASS / [x] PENDING: release binaries workflow output is captured and reproducible. **Current state: missing evidence** in uploaded Ubuntu package. Evidence path: `artifacts/v2_2_19/release_workflow/release_binaries_workflow.log`

## Local 3N/1M smoke evidence

Run and attach output/artifacts (`OUT_DIR` must be a real writable path):

```bash
OUT_DIR=... bash scripts/v2_2_19_local_3n_1m_smoke.sh
```

- [ ] PASS / [x] PENDING: local `3N/1M` smoke completes with evidence bundle. **Current state: FAIL** due to runtime shell error `node: unbound variable`. Evidence path: `artifacts/v2_2_19/local_3n_1m_smoke/local_3n_1m_smoke.log`

## Private staged convergence evidence

Run and attach output/artifacts (`OUT_DIR` must be a real writable path). The baseline is mandatory; the intermediate gate is mandatory unless a warning-only waiver is recorded; the 5N/4M stress gate is diagnostic until orphan recovery lands.

```bash
OUT_DIR=... bash scripts/v2_2_19_private_5n_1m_rehearsal.sh
OUT_DIR=... bash scripts/v2_2_19_private_5n_2m_rehearsal.sh
OUT_DIR=... bash scripts/v2_2_19_private_5n_4m_rehearsal.sh
# or run the orchestrated sequence
OUT_DIR=... bash scripts/v2_2_19_staged_convergence_gates.sh
```

- [ ] PASS / [x] PENDING: private `5N/1M baseline` rehearsal completes with evidence bundle and quiescence metrics. Evidence path: `artifacts/v2_2_19/private_5n_1m_rehearsal/evidence-summary.md`
- [ ] PASS / [x] PENDING: private `5N/2M intermediate` rehearsal completes or has an explicit warning-only waiver with failure classification. Evidence path: `artifacts/v2_2_19/private_5n_2m_rehearsal/evidence-summary.md`
- [ ] PASS / [x] PENDING: private `5N/4M stress` rehearsal completes without harness hang and classifies any divergence/orphan pressure. Evidence path: `artifacts/v2_2_19/private_5n_4m_rehearsal/evidence-summary.md`
- [ ] PASS / [x] PENDING: each staged rehearsal includes `evidence.tar.gz` and `evidence.tar.gz.sha256` on both PASS and FAIL. Evidence path: `artifacts/v2_2_19/staged_convergence_gates/`

## Snapshot/restore evidence

- [ ] PASS / [x] PENDING: snapshot creation evidence attached (command logs + artifact paths). **Current state: missing evidence**. Evidence path: `artifacts/v2_2_19/snapshot_restore/snapshot_create.log`
- [ ] PASS / [x] PENDING: restore/rebuild drill evidence attached (expected post-restore checks included). **Current state: missing evidence**. Evidence path: `artifacts/v2_2_19/snapshot_restore/restore_rebuild_drill.log`

## P2P convergence evidence

- [ ] PASS / [x] PENDING: multi-node convergence evidence attached with pre/post-quiescence convergence, worst lag, distinct final tips, peer visibility, orphan count, and missing-parent count. Evidence paths: `artifacts/v2_2_19/private_5n_1m_rehearsal/p2p_convergence.json`, `artifacts/v2_2_19/private_5n_2m_rehearsal/p2p_convergence.json`, `artifacts/v2_2_19/private_5n_4m_rehearsal/p2p_convergence.json`
- [ ] PASS / [x] PENDING: restart/rejoin behavior evidence attached for rehearsal topology. Evidence path: `artifacts/v2_2_19/private_5n_4m_rehearsal/restart_rejoin.log`

## Miner external protocol evidence

- [ ] PASS / [x] PENDING: miner/node contract remains external-mode only for `v2.2.19`. Evidence path: `artifacts/v2_2_19/private_5n_4m_rehearsal/miner_external_mode_contract.md`
- [ ] PASS / [x] PENDING: mining protocol rehearsal evidence attached for declared topology. Evidence path: `artifacts/v2_2_19/private_5n_4m_rehearsal/mining_protocol_rehearsal.log`

## GPU scaffold/fallback evidence

- [ ] PASS / [x] PENDING: GPU path status is explicitly documented as optional/scaffold unless canonical kernel evidence is provided. Evidence path: `artifacts/v2_2_19/gpu_fallback/gpu_scaffold_status.md`
- [ ] PASS / [x] PENDING: CPU fallback/compatibility behavior evidence attached for environments without GPU path enablement. Evidence path: `artifacts/v2_2_19/gpu_fallback/cpu_fallback_compatibility.log`

## RPC readiness/release metadata evidence

- [ ] PASS / [x] PENDING: `/release` metadata reflects `v2.2.19` runtime truths (no stale algorithm/engine fields). Evidence path: `artifacts/v2_2_19/preflight/release_endpoint.json`
- [ ] PASS / [x] PENDING: `/status` and `/readiness` evidence attached for operator-facing fields in scope. Evidence path: `artifacts/v2_2_19/preflight/status_readiness.json`
- [ ] PASS / [x] PENDING: RPC exposure posture remains private/localhost unless explicitly hardened and approved. Evidence path: `artifacts/v2_2_19/preflight/rpc_exposure_posture.md`

## Known limitations accepted for v2.2.19

- [ ] PASS / [x] PENDING: limitations acceptance is explicitly aligned with `docs/KNOWN_LIMITATIONS_V2_2_19.md`. Evidence path: `artifacts/v2_2_19/closeout_decision/limitations_acceptance.md`
- [ ] PASS / [x] PENDING: closeout record states `v2.2.19` is private-testnet hardening and not a public launch declaration. Evidence path: `artifacts/v2_2_19/closeout_decision/closeout_scope_statement.md`

## Final v2.3.0 start checklist

Record these gates before opening formal `v2.3.0` readiness work. Passing this checklist does **not** bump `VERSION` and does **not** change `public_testnet_ready=false`. The detailed checklist lives in `docs/V2_3_0_START_CHECKLIST.md`.

- [ ] PASS / [x] PENDING: dependency ordering is confirmed: `#545`, `#546`, and `#547` are merged, and `#548` is merged or explicitly deferred with rationale. Evidence path: `artifacts/v2_2_19/closeout_decision/dependency_ordering.md`
- [ ] PASS / [x] PENDING: version guard confirms `VERSION=v2.2.19` and Cargo workspace `2.2.19`; no version bump is included. Evidence path: `artifacts/v2_2_19/closeout_decision/version_guard.md`
- [ ] PASS / [x] PENDING: build/test gate is PASS for fmt, check, test, and clippy. Evidence path: `artifacts/v2_2_19/validation/cargo_validation_suite.log`
- [ ] PASS / [x] PENDING: `3N/1M`, `5N/1M`, and `5N/2M` gates are PASS with evidence bundles, archives, and checksums. Evidence path: `artifacts/v2_2_19/staged_convergence_gates/`
- [ ] PASS / [x] PENDING: `5N/4M` stress is PASS or accepted as a non-blocking limitation with metrics, owner, expiry, and exit criteria. Evidence path: `artifacts/v2_2_19/private_5n_4m_rehearsal/evidence-summary.md`
- [ ] PASS / [x] PENDING: known limitations are mapped into accepted limitations and blockers for the v2.3.0 start decision. Evidence path: `artifacts/v2_2_19/closeout_decision/v2_3_0_known_limitations_mapping.md`
- [ ] PASS / [x] PENDING: incident and waiver ledger is complete, with no unresolved Sev-1 consensus/sync/security blocker. Evidence path: `artifacts/v2_2_19/closeout_decision/incident_waiver_ledger.md`
- [ ] PASS / [x] PENDING: final start decision is recorded as `GO_TO_START_V2_3_0_REVIEW`, `NO_GO`, or `WAIVED_WITH_REASON`. Evidence path: `artifacts/v2_2_19/closeout_decision/v2_3_0_start_decision.md`

## Public-testnet readiness evidence requirements

Public-testnet readiness remains blocked until all launch evidence is attached and reviewed. `public_testnet_ready` must remain `false` while any item is PENDING.

- [ ] PASS / [x] PENDING: RPC exposure, authentication/authorization posture, firewall/listener configuration, secret redaction, and operator approval are attached. Evidence path: `artifacts/public_testnet/readiness/security_rpc_posture.md`
- [ ] PASS / [x] PENDING: bootnodes, chain ID, peer admission policy, restart/rejoin behavior, NAT/firewall assumptions, and chain-isolation evidence are attached. Evidence path: `artifacts/public_testnet/readiness/network_posture.md`
- [ ] PASS / [x] PENDING: deployment, rollback, incident response, snapshot/restore, monitoring, and evidence-collection runbooks are attached. Evidence path: `artifacts/public_testnet/readiness/operator_runbooks.md`
- [ ] PASS / [x] PENDING: release artifacts, checksums, install verification, `/release` metadata, and rollback target artifacts are attached. Evidence path: `artifacts/public_testnet/readiness/release_artifacts.md`
- [ ] PASS / [x] PENDING: `/readiness` captures for every required node show no public-testnet blockers, and the aggregate report proves the gate. Evidence path: `artifacts/public_testnet/readiness/readiness_aggregate.md`
- [ ] PASS / [x] PENDING: accepted limitations are explicitly non-blocking for public testnet and include owners, expiry, and exit criteria. Evidence path: `artifacts/public_testnet/readiness/accepted_limitations.md`

## 30-day burn-in evidence requirements

The 30-day burn-in clock starts only after public-testnet launch evidence collection is explicitly authorized. Burn-in completion is **NO-GO** if any unresolved Sev-1 consensus/sync/security incident remains.

- [ ] PASS / [x] PENDING: at least 30 consecutive UTC daily evidence bundles are present. Evidence path: `artifacts/public_testnet/burn_in_30d/daily/`
- [ ] PASS / [x] PENDING: daily health/readiness, uptime, restart, incident, peer, convergence, orphan/missing-parent, miner, RPC, storage, snapshot/restore, and security metrics are aggregated. Evidence path: `artifacts/public_testnet/burn_in_30d/aggregate_metrics.md`
- [ ] PASS / [x] PENDING: waiver and incident ledgers are updated with closeout status for every burn-in issue. Evidence path: `artifacts/public_testnet/burn_in_30d/incident_waiver_closeout.md`
- [ ] PASS / [x] PENDING: final burn-in report links daily bundles and aggregate metrics and records PASS, NO_GO, or WAIVED_WITH_REASON. Evidence path: `artifacts/public_testnet/burn_in_30d/final_report.md`

## Blockers before 2.3.0 public testnet

Record open blockers that must be closed before any `v2.3.0` public-testnet go/no-go:

- [ ] PASS / [x] PENDING: public-testnet launch checklist gates are satisfied and evidenced. Evidence path: `artifacts/v2_2_19/closeout_decision/public_testnet_blockers.md`
- [ ] PASS / [x] PENDING: unresolved private rehearsal issues are tracked with owners and due dates. Evidence path: `artifacts/v2_2_19/closeout_decision/private_rehearsal_issue_register.md`
- [ ] PASS / [x] PENDING: no closeout text makes readiness claims beyond `v2.2.19` scope. Evidence path: `artifacts/v2_2_19/closeout_decision/scope_compliance_review.md`
- [ ] PASS / [x] PENDING: `public_testnet_ready=true` is not asserted before the public-testnet go/no-go passes. Evidence path: `artifacts/v2_2_19/closeout_decision/public_testnet_signal_guard.md`

## Final closeout decision

- Decision: `GO_TO_CLOSE_V2_2_19 | NO_GO | WAIVED_WITH_REASON`
- Evidence path: `artifacts/v2_2_19/closeout_decision/final_decision.md`
- Reviewer: `____________________`
- Date (UTC): `YYYY-MM-DD`

## Final sign-off

- [ ] PASS / [x] PENDING: all required sections above are PASS with evidence links. **Not satisfied; blockers remain.**
- [x] PASS / [ ] PENDING: disposition recorded as `NO_GO` with rationale and evidence path.


## Automatic NO-GO rules (must remain enforced)

- [ ] PASS / [x] PENDING: `cargo check`, `cargo test`, and `cargo clippy` were all run and attached; otherwise automatic **NO_GO**.
- [ ] PASS / [x] PENDING: preflight evidence exists at `artifacts/v2_2_19/preflight/`; missing evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: local smoke evidence exists at `artifacts/v2_2_19/local_3n_1m_smoke/`; missing evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: private `5N/1M baseline` evidence exists at `artifacts/v2_2_19/private_5n_1m_rehearsal/`; missing evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: private `5N/2M intermediate` evidence exists at `artifacts/v2_2_19/private_5n_2m_rehearsal/`, or a warning-only waiver is recorded with reason.
- [ ] PASS / [x] PENDING: private `5N/4M stress` evidence exists at `artifacts/v2_2_19/private_5n_4m_rehearsal/` with failure classification if it diverges; missing evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: release workflow evidence exists at `artifacts/v2_2_19/release_workflow/`; missing evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: snapshot/restore evidence exists at `artifacts/v2_2_19/snapshot_restore/`, or explicit waiver is recorded with reason.
- [ ] PASS / [x] PENDING: `public_testnet_ready=true` is **not** asserted for v2.2.19 scope; any true assertion is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: GPU is **not** claimed production-ready without canonical kernel evidence; unsupported claim is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: any shell error during runtime script execution is automatic **NO_GO** unless explicitly waived.
- [ ] PASS / [x] PENDING: any required node with `healthy=0` is automatic **NO_GO** unless explicitly waived.
- [ ] PASS / [x] PENDING: any required node with readiness `0` is automatic **NO_GO** unless explicitly waived.
- [ ] PASS / [x] PENDING: all peers `=0` in private `5N/4M` evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: all heights `=0` in rehearsal evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: all miner templates `=0` in rehearsal evidence is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: accepted blocks `=0` without explicit waiver is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: `chain_id` unknown without explicit waiver is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: evidence archive `evidence.tar.gz` missing is automatic **NO_GO**.
- [ ] PASS / [x] PENDING: evidence checksum file missing is automatic **NO_GO**.

