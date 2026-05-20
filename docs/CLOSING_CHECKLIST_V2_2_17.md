# v2.2.17 Closing Checklist (API/operator/security)

> Rule: mark PASS only when evidence file/path/output exists. Otherwise keep PENDING and include exact command.

## Version consistency
- [ ] PASS / [x] PENDING: VERSION=`v2.2.17`, Cargo=`2.2.17`, README status, version matrix baseline, release notes, and this checklist are mutually consistent.  
  Evidence: repo files review.

## Required command evidence
- [ ] PASS / [x] PENDING: `cargo fmt --check`  
  Run: `cargo fmt --check` and store output in `artifacts/v2_2_17_api_security/<run_id>/checks/cargo_fmt_check.txt`.
- [ ] PASS / [x] PENDING: `cargo test --workspace`  
  Run: `cargo test --workspace` and store output in `.../checks/cargo_test_workspace.txt`.
- [ ] PASS / [x] PENDING: `cargo build --workspace --release`  
  Run: `cargo build --workspace --release` and store output in `.../checks/cargo_build_release.txt`.

## API/operator/security closeout evidence
- [ ] PASS / [x] PENDING: API endpoint inventory complete (`docs/RPC_ENDPOINT_INVENTORY_V2_2_17.md`).
- [ ] PASS / [x] PENDING: API exposure profiles documented (public_safe/operator/admin/dev).
- [ ] PASS / [x] PENDING: admin endpoints disabled by default (capture `/admin/*` probe results).
- [ ] PASS / [x] PENDING: unsafe admin exposure blocked.
- [ ] PASS / [x] PENDING: optional operator auth tested.
- [ ] PASS / [x] PENDING: request body limits tested.
- [ ] PASS / [x] PENDING: rate limits tested or explicitly documented pending.
- [ ] PASS / [x] PENDING: CORS/bind-address policy tested.
- [ ] PASS / [x] PENDING: diagnostics redaction tested.
- [ ] PASS / [x] PENDING: `/release` endpoint checked.
- [ ] PASS / [x] PENDING: `/readiness` endpoint checked.
- [ ] PASS / [x] PENDING: RPC security smoke script executed (`scripts/v2_2_17_rpc_security_smoke.sh`).
- [ ] PASS / [x] PENDING: evidence bundle generated (`scripts/v2_2_17_collect_api_security_evidence.sh`).

## Guardrail assertions
- [ ] PASS / [x] PENDING: no smart contracts added.
- [ ] PASS / [x] PENDING: no pool logic added.
- [ ] PASS / [x] PENDING: no consensus rule changes.
- [ ] PASS / [x] PENDING: no PoW semantic changes.
- [ ] PASS / [x] PENDING: no GPU kernel changes in v2.2.17.
- [ ] PASS / [x] PENDING: no v2.3.0 readiness claim.
- [ ] PASS / [x] PENDING: no v3.0 readiness claim.
