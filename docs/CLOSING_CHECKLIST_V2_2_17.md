# PulseDAG v2.2.17 closing checklist

v2.2.17 closes only when API/operator/security boundaries are documented, defaults are hardened, and release evidence is captured. This checklist is a release gate for v2.2.17 and is not a v2.3.0 readiness claim.

## Version and scope gate

- [ ] `VERSION` is `v2.2.17`.
- [ ] Cargo workspace version is `2.2.17`.
- [ ] Cargo workspace license metadata remains `ISC`.
- [ ] `README.md` and `docs/VERSION_MATRIX.md` describe v2.2.17 as the current API/operator/security hardening milestone.
- [ ] v2.2.16 is described as miner/node contract hardening evidence passed, not the current milestone.
- [ ] v2.2.18 remains private-testnet RC.
- [ ] v2.3.0 remains a readiness decision only, not an automatic launch.
- [ ] No smart contracts are added.
- [ ] No pool logic is added.
- [ ] No consensus-rule change is included.
- [ ] No PoW semantic change is included.
- [ ] No miner protocol change is included unless strictly required for security documentation clarity.
- [ ] No full Kaspa/GHOSTDAG compatibility claim is made.
- [ ] No v3.0 readiness claim is made.

## Required command gate

Run these commands from the repository root and attach output or CI links:

```bash
cargo fmt --check
cargo test --workspace
```

- [ ] `cargo fmt --check` passes.
- [ ] `cargo test --workspace` passes.

## RPC/API inventory and classification gate

- [ ] An RPC/API surface audit has been completed.
- [ ] Endpoint inventory includes method/path and owner.
- [ ] Every endpoint is classified as `public`, `operator`, or `admin`.
- [ ] Classification and rationale are documented in release notes and/or API docs.
- [ ] Deprecated/legacy endpoints are documented with risk posture.

## Admin lockdown and local-only default gate

- [ ] Admin endpoints are disabled by default or bound to local-only exposure by default.
- [ ] Any non-local admin exposure requires explicit configuration.
- [ ] Admin endpoint enablement is documented with risk warning.
- [ ] Startup/config validation prevents contradictory admin exposure settings.
- [ ] Runbook includes verification steps for admin lock-down posture.

## Optional operator auth gate

- [ ] Optional operator authentication strategy is documented.
- [ ] Authentication scope (which operator/admin routes require auth) is documented.
- [ ] Auth disabled/enabled behavior is deterministic and documented.
- [ ] Misconfigured auth settings fail closed or emit blocking validation errors.
- [ ] Secret/token handling guidance avoids plaintext leakage in logs and diagnostics.

## Rate limiting, request bounds, and CORS gate

- [ ] Rate-limiting policy is documented for public and operator routes.
- [ ] Request-size limits are documented and bounded.
- [ ] Oversized request behavior is documented and tested/evidenced.
- [ ] CORS default policy is documented and conservative by default.
- [ ] CORS override configuration includes explicit allowlist guidance.
- [ ] Insecure wildcard policies, if allowed, are clearly warned and operator-gated.

## Safe config validation gate

- [ ] Security-relevant API config keys are documented.
- [ ] Invalid/unsafe combinations produce actionable validation errors.
- [ ] Defaults prioritize least privilege and local safety.
- [ ] Validation behavior is covered in tests or documented command evidence.

## Diagnostics redaction and endpoint hardening gate

- [ ] Diagnostics/logging policy documents redaction of secrets/tokens/sensitive identifiers.
- [ ] `/health`, `/status`, readiness, and release metadata endpoints are reviewed for over-disclosure.
- [ ] Public diagnostics avoid leaking sensitive internals by default.
- [ ] Operator-only diagnostics are clearly scoped and documented.
- [ ] Redaction behavior is tested or evidenced where applicable.

## Operator runbook and incident response gate

- [ ] Operator runbook includes endpoint exposure policy (`public`/`operator`/`admin`).
- [ ] Runbook includes rollout procedure for auth, rate limits, and request-size settings.
- [ ] Runbook includes API abuse and incident response steps (throttle/lockdown/recover).
- [ ] Runbook includes rollback steps for misconfigured API security settings.

## Evidence collection gate

- [ ] Evidence location is defined (for example `evidence/v2.2.17/`).
- [ ] Command output for required checks is attached.
- [ ] API classification table snapshot is attached.
- [ ] Admin lockdown verification notes are attached.
- [ ] Rate-limit/request-size/CORS validation notes are attached.
- [ ] Redaction/readiness endpoint review notes are attached.
- [ ] Operator runbook update references are attached.
- [ ] Closeout summary explicitly states v2.2.17 hardens API/operator/security boundaries and does not claim v2.3.0 or v3.0 readiness.

## Defect gate

- [ ] No unresolved Sev-1 consensus defect remains open.
- [ ] No unresolved Sev-1 sync defect remains open.
- [ ] No unresolved Sev-1 API security defect remains open.
- [ ] Any unresolved Sev-2 API/operator/security defect is documented with impact, owner, and follow-up milestone.
- [ ] Any release evidence failure is either fixed and rerun or recorded as a blocking release issue.

## Closeout decision

- [ ] Release notes are updated in `docs/RELEASE_NOTES_V2_2_17.md`.
- [ ] Closing checklist is updated in `docs/CLOSING_CHECKLIST_V2_2_17.md`.
- [ ] `README.md` status text is updated to v2.2.17 framing.
- [ ] `docs/VERSION_MATRIX.md` is updated to v2.2.17 framing.
- [ ] Markdown links in updated release/security/operator docs are verified.
- [ ] Evidence links are collected in the release issue, PR, or release artifact index.
