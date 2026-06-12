# Known Limitations v2.2.20

This document records the current known limitations for `v2.2.20` active hardening. It must be read with `docs/VERSION_MATRIX.md`, `docs/CLOSING_CHECKLIST_V2_2_20.md`, and the committed evidence documents.

## Scope and guardrails

- `v2.2.20` is an active hardening milestone, not a release-readiness declaration.
- `public_testnet_ready=false` remains the only allowed public-testnet signal.
- This document does not claim public-testnet live status, public-testnet readiness, or `v2.3.0` readiness.
- No limitation below authorizes a consensus-rule change, PoW semantic change, smart-contract enablement, pool-logic enablement, or `VERSION` bump.

## Remaining real limitations that block closeout unless waived

| Limitation | Current state | Required exit evidence |
|---|---|---|
| `5N/2M` accepted-block recovery | The latest recorded `5N/2M` evidence improved peer visibility, convergence, and backlog drain, but failed because accepted blocks remained `0`. | Replacement `5N/2M` evidence after the latest hardening PRs with accepted blocks, archive, checksum, final node table, miner accept/reject summary, and no regression in peer/orphan/final-tip behavior; or a complete non-readiness waiver. |
| `5N/4M` stress recovery | The latest recorded `v2.2.20` stress evidence is still `OBSERVE_FAIL`, with all-zero peer visibility, orphan and pending-missing-parent saturation, and divergent tips. | Replacement `5N/4M` evidence that is PASS, or an accepted bounded limitation with measured divergence, peer visibility, RPC liveness, orphan/missing-parent backlog, owner, reviewer, UTC approval, expiry, and exit criteria. |
| Snapshot/restore closeout artifact | The deterministic drill is documented, but closeout requires an attached drill artifact/checksum or formal waiver. | Snapshot creation/restore bundle, checksums, restored node health/readiness, timing metrics, and evidence manifest; or waiver with owner, UTC approval, scope, expiry, and exit criteria. |
| Final CI/workspace validation artifact | The closeout requires validation logs for the evaluated merge commit. | Accepted logs or CI artifacts for `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings`. |

## Out-of-scope non-readiness areas

The following are not active `v2.2.20` code blockers by themselves. They remain future-public-testnet or future-product scope and cannot be used as readiness claims without separate evidence and approval.

| Area | Current scope | Required future evidence before any claim |
|---|---|---|
| Public RPC exposure | RPC endpoints remain private/localhost by default; public exposure is out of scope for this closeout. | Public-testnet-specific security/RPC exposure review, authn/authz posture, firewall/listener config, secret-redaction check, and operator sign-off. |
| Long-run public operation | Private rehearsals do not provide long-run public-testnet burn-in. | Separate public-testnet go/no-go authorization and at least 30 consecutive UTC days of burn-in evidence after launch authorization. |
| Kaspa/GHOSTDAG compatibility assertions | Deterministic DAG behavior does not prove full Kaspa/GHOSTDAG compatibility. | Canonical compatibility implementation and explicit tests/evidence before any compatibility claim. |
| GPU mining | GPU paths remain optional/scaffold-only unless a canonical tested kHeavyHash GPU kernel and evidence are included. | Canonical GPU kernel implementation, deterministic validation, and miner evidence before any production-ready GPU claim. |

## Limitations resolved or narrowed by PRs #600-#605

| PR | Hardening area | Limitation status |
|---|---|---|
| `#600` | Kaspa-style orphan-root recovery | Narrowed the orphan-root recovery limitation by adding explicit orphan-root recovery behavior. It still requires replacement `5N/2M` and `5N/4M` evidence before closeout. |
| `#601` | Isolate `submit_block` from status paths | Narrowed RPC/status coupling risk by separating mining submission pressure from status paths. Stress evidence is still required to prove final-capture liveness. |
| `#602` | Bound concurrent mining submit requests | Narrowed unbounded submit concurrency risk. Closeout still needs accepted-block evidence and queue/backpressure observations. |
| `#603` | Rate-limit-aware orphan recovery plan | Resolved the missing documented follow-up for rate-limit-aware orphan recovery by recording the plan. Runtime evidence remains required. |
| `#604` | Rate-limit-aware block request recovery in the submit-isolation branch | Narrowed parent-fetch/orphan recovery retry risk by making block request recovery rate-limit aware. Replacement staged evidence remains required. |
| `#605` | Bound RPC liveness endpoint handlers | Narrowed RPC liveness starvation risk by bounding liveness endpoint handlers. Replacement stress evidence must prove the endpoints remain captureable under load. |

## Limitations resolved or narrowed by later PRs

| PR | Hardening area | Limitation status |
|---|---|---|
| `#606` | Lock-free liveness snapshot | Further narrowed RPC liveness starvation risk by allowing stale/degraded snapshots instead of blocking evidence endpoints. |
| `#607` | Bounded mining submit actor | Further narrowed mining-submit backpressure risk by routing submissions through a bounded actor. |
| `#609` | Deterministic orphan recovery supervisor | Further narrowed orphan-recovery ambiguity by adding deterministic supervision. Replacement evidence must show backlog behavior. |
| `#610` | Peer retention under stress | Further narrowed peer drop-to-zero risk. Replacement `5N/4M` evidence must prove whether peer visibility remains non-zero. |
| `#611` | Deterministic snapshot restore drill | Added the required restore evidence track, but closeout still needs an artifact or waiver. |
| `#612` | Mining submit tests aligned with actor semantics | Narrowed regression-test mismatch for bounded submit behavior. |
| `#613` | Windows and Docker rehearsal hardening | Narrowed environment ambiguity by hardening preflight and Docker execution. |
| `#614` | Evidence manifest completion | Narrowed evidence-review ambiguity by requiring self-classifying evidence manifests for rehearsal bundles. |

## Closeout decision reference

The final closeout evidence index is `docs/V2_2_20_FINAL_EVIDENCE_INDEX.md`. It records `NO_GO` as of 2026-06-12 because the remaining closeout blockers above have no complete replacement evidence or waiver in this repository.

## Closeout interpretation

Resolved or narrowed limitations are not public-testnet readiness evidence by themselves. They must be paired with the closeout checklist, replacement private rehearsal bundles, checksums, waivers where applicable, and a final `GO/NO-GO` decision.
