# PulseDAG v2.2.17 closing checklist

v2.2.17 closes only when API/operator/security boundaries are documented, defaults are hardened, and release evidence is captured. This checklist is a **release gate** for v2.2.17 and is **not** a v2.3.0 or v3.0 readiness claim.

## Version and scope gate

- [ ] VERSION/Cargo workspace aligned to v2.2.17 **if release bump is included** (`VERSION=v2.2.17`, workspace `version=2.2.17`).
- [ ] `README.md` and `docs/VERSION_MATRIX.md` describe v2.2.17 as the API/operator/security hardening milestone.
- [ ] RPC endpoint inventory complete.
- [ ] API profiles documented and tested (`public` / `operator` / `admin`).
- [ ] admin disabled by default.
- [ ] unsafe admin exposure blocked.
- [ ] optional operator auth tested.
- [ ] rate/request size limits tested.
- [ ] CORS/bind-address policy tested.
- [ ] diagnostics redaction tested.
- [ ] `/release` hardened.
- [ ] `/readiness` hardened.
- [ ] secure operator runbook complete.
- [ ] RPC security smoke script passes.
- [ ] API security evidence bundle generated.
- [ ] no smart contracts added.
- [ ] no pool logic added.
- [ ] miner remains external.
- [ ] no consensus rule changes.
- [ ] no PoW semantic changes.
- [ ] no GPU kernel changes in v2.2.17.
- [ ] no v2.3.0 readiness claim.
- [ ] no v3.0 readiness claim.

## Required command gate

Run from repo root and attach output or CI links:

```bash
cargo fmt --check
cargo test --workspace
cargo build --workspace --release
```

- [ ] cargo fmt --check passes.
- [ ] cargo test --workspace passes.
- [ ] cargo build --workspace --release passes.

## Evidence and closeout gate

- [ ] Evidence location is defined (for example `evidence/v2.2.17/`).
- [ ] API inventory table (method/path/profile/owner/default exposure/auth) attached.
- [ ] Admin lockdown verification notes attached.
- [ ] Operator auth test notes attached (enabled/disabled/misconfigured).
- [ ] Rate-limit/request-size/CORS/bind-address validation notes attached.
- [ ] `/release` and `/readiness` disclosure-hardening validation attached.
- [ ] Diagnostics redaction validation attached.
- [ ] Secure operator runbook link/update reference attached.
- [ ] RPC security smoke script output attached.
- [ ] API security evidence bundle index attached.
- [ ] Markdown links in updated release/security/operator docs are verified.

## Boundary assertions for v2.2.17

- [ ] Documentation-only unless version/status files are touched.
- [ ] No smart contracts, pool logic, or embedded miner scope expansion.
- [ ] No consensus/PoW semantic changes under this closeout.
- [ ] No claim that v2.2.17 itself grants v2.3.0 or v3.0 readiness.

## Closeout decision

- [ ] `docs/RELEASE_NOTES_V2_2_17.md` updated to finalized closeout framing.
- [ ] `docs/CLOSING_CHECKLIST_V2_2_17.md` updated and checked.
- [ ] `docs/VERSION_MATRIX.md` updated with v2.2.17 closeout position.
- [ ] `README.md` updated with v2.2.17 closeout status summary.
- [ ] Release issue/PR links all evidence artifacts and command outputs.
