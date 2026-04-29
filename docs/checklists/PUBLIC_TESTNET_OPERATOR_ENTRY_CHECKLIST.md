# Public testnet operator entry checklist (pre-launch prep)

Purpose: ensure each operator can enter a **future** public-testnet window with consistent, auditable readiness.

This checklist is preparation-only and does **not** authorize launch by itself.

## 1) Operator identity and ownership
- [ ] Primary operator handle, backup handle, and UTC on-call window documented.
- [ ] Incident escalation path validated against `docs/runbooks/INDEX.md`.
- [ ] Operator confirms ability to publish UTC-stamped incident updates.

## 2) Runtime access and observability
- [ ] Access to `/status`, `/runtime/status`, and `/sync/status` verified.
- [ ] Access to node logs and event stream captures verified.
- [ ] Alert channel subscription verified for runtime/sync/mining health alarms.
- [ ] Ability to export evidence artifacts into `artifacts/release-evidence/<run_id>/` verified.

## 3) Node and miner operating boundaries
- [ ] Operator acknowledges consensus parameters are frozen for readiness drills.
- [ ] Operator confirms miner remains external and standalone per `docs/MINER_FINAL.md`.
- [ ] Operator confirms no pool logic, share accounting, or payout workflow is in scope.
- [ ] Operator confirms only documented env/config surfaces will be used.

## 4) Readiness drill execution ability
- [ ] Operator can execute restart/churn/rejoin drills from `docs/runbooks/FINAL_POW_PUBLIC_TESTNET_DRY_RUN.md`.
- [ ] Operator can execute snapshot restore/rebuild drills using `docs/runbooks/RECOVERY_ORCHESTRATION.md`.
- [ ] Operator can record drill start/end, owner, and outcomes with UTC timestamps.
- [ ] Operator can produce explicit pass/fail notes and unresolved-risk statements.

## 5) Evidence and go/no-go hygiene
- [ ] Operator can populate `dry-run/go-no-go.md` with objective criterion outcomes.
- [ ] Operator can maintain `dry-run/incident-log.md` with mitigations and residual risk.
- [ ] Operator can map collected evidence to `docs/RELEASE_EVIDENCE.md` required paths.
- [ ] Operator understands this checklist is an entry prerequisite, not a launch signal.
