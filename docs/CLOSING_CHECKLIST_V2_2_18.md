# v2.2.18 Private RC Closing Checklist

> Rule: do **not** mark PASS without an evidence path (file path, command output path, or report section). If evidence is unavailable, keep PENDING or mark WAIVED with explicit approver/date/reason.

## 1) Upstream prerequisite gate
- [ ] PASS / [ ] WAIVED / [x] PENDING: v2.2.17 evidence is fully closed **or** explicitly waived.  
  Evidence path: `____________________`

## 2) Version and metadata alignment gate
- [ ] PASS / [x] PENDING: `VERSION`, Cargo workspace version, `README.md`, and `docs/VERSION_MATRIX.md` are aligned for v2.2.18 RC closing state.  
  Evidence path: `____________________`

## 3) Build and test command gate
- [ ] PASS / [x] PENDING: `cargo fmt --check` PASS.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: `cargo test --workspace` PASS.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: `cargo build --workspace --release` PASS.  
  Evidence path: `____________________`

## 4) Rehearsal execution gate
- [ ] PASS / [x] PENDING: local 3-node + 1-miner rehearsal PASS.  
  Evidence path: `____________________`
- [ ] PASS / [ ] PENDING: RC 5-node + 4-miner rehearsal attempted **or** explicitly marked pending with owner and target date.  
  Evidence path: `____________________`

## 5) Required operational evidence gate
- [ ] PASS / [x] PENDING: sync convergence evidence present.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: miner telemetry evidence present (accepted/rejected share behavior and node acceptance visibility).  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: perturbation drills evidence present.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: snapshot/restore drill evidence present.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: RPC security smoke evidence present.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: release artifact dry run evidence present.  
  Evidence path: `____________________`

## 6) Reporting and risk gate
- [ ] PASS / [x] PENDING: go/no-go report generated and archived.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: known limitations documented.  
  Evidence path: `____________________`
- [ ] PASS / [x] PENDING: risk register updated for residual v2.2.18 risks.  
  Evidence path: `____________________`

## 7) Guardrail assertions (must remain true)
- [ ] PASS / [x] PENDING: no consensus changes.
- [ ] PASS / [x] PENDING: no PoW semantic changes.
- [ ] PASS / [x] PENDING: no smart contracts.
- [ ] PASS / [x] PENDING: no pool logic.
- [ ] PASS / [x] PENDING: miner remains external.
- [ ] PASS / [x] PENDING: GPU remains optional only.
- [ ] PASS / [x] PENDING: no v2.3.0 readiness claim.
- [ ] PASS / [x] PENDING: no v3.0 readiness claim.

## Final decision
- [ ] PASS / [x] PENDING: v2.2.18 private RC closeout approved and handed off to v2.3.0 decision review inputs.
- Decision owner: `____________________`
- Decision date (UTC): `____________________`
- Decision evidence/report path: `____________________`

## Handoff artifact (required before closeout PASS)
- [ ] PASS / [x] PENDING: `docs/V2_3_0_READINESS_DECISION_INPUTS.md` exists and reflects current evidence state (PASS/PENDING/WAIVED with explicit rationale).
  Evidence path: `docs/V2_3_0_READINESS_DECISION_INPUTS.md`
