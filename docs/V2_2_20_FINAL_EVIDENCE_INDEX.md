# v2.2.20 Final Hardening Evidence Index

Date: 2026-06-18

This is the final `v2.2.20` hardening closeout index. It records the evidence that exists, the checksums that were provided with that evidence, the known waivers or missing artifacts, and the decision for whether a future `v2.3.0` review may start.

This index is evidence-only. It does **not** bump `VERSION`, does **not** change consensus rules or PoW semantics, does **not** add smart contracts, does **not** add pool logic, does **not** change miner architecture, does **not** claim public-testnet live status, and does **not** claim public-testnet readiness.

## Release-control guardrails

| Guardrail | Required value | Closeout value | Status |
|---|---|---|---|
| `VERSION` | `v2.2.20` | `v2.2.20` | PASS |
| Cargo workspace version | `2.2.20` | `2.2.20` | PASS |
| Public-testnet signal | `public_testnet_ready=false` | `public_testnet_ready=false` | PASS |
| Public-testnet live claim | Forbidden | No live claim is made by this index. | PASS |
| `v2.3.0` readiness claim | Forbidden | No readiness claim is made by this index. | PASS |

## Final evidence matrix

| Gate | Result for closeout | Artifact / evidence reference | Checksum / integrity reference | Closeout interpretation |
|---|---|---|---|---|
| `5N/1M baseline` | PASS | `docs/V2_2_20_5N_1M_BASELINE_EVIDENCE.md`; uploaded artifact `v2_2_20_5n_1m_baseline_evidence (3).zip`; inner archive `evidence.tar.gz` | inner archive sha256 `4de50edfba42e11bd75abac0ae242baf1d9239fbfe6aa6104d5c325fa2f18c6e` | Accepted as the `v2.2.20` baseline regression guard. |
| `5N/2M intermediate` | FAIL, improved; not closeout-pass | `docs/V2_2_20_5N_2M_INTERMEDIATE_EVIDENCE.md`; local artifact `artifacts/v2_2_20/private_5n_2m_rehearsal/20260607T074320Z/evidence.tar.gz` | archive sha256 `98ac709013b051f85ba400c050b971f606e6feda96c249480754e9928a541d5d` | Peer visibility, final-tip convergence, and backlog drain improved, but accepted blocks remained `0`; replacement PASS evidence or a complete waiver is still missing. |
| `5N/4M stress` | OBSERVE_FAIL; not closeout-pass | `docs/V2_2_20_FIRST_STRESS_EVIDENCE.md`; uploaded artifact `v2_2_20_5n_4m_stress_observe_evidence.zip`; inner archive `evidence.tar.gz` | inner archive sha256 `321e260bf57daf9c25106d9eac8bf7ec01172488ee7a6a4be29b6d23771d7a2e`; artifact zip sha256 `8574e1ae856be675773e3da4d42f022cc140cadd27869fa6aa5b2906670c6939` | Stress evidence remains diagnostic only: peer visibility collapsed to zero, orphan/pending-missing-parent backlogs saturated, and tips remained divergent. Replacement post-hardening evidence or an accepted bounded limitation is still missing. |
| Snapshot restore drill | AUTOMATED; run artifact required per execution | `docs/SNAPSHOT_RESTORE_DRILL_V2_2_20.md`; script `scripts/v2_2_20_snapshot_restore_drill.sh`; expected artifacts `artifacts/v2_2_20_snapshot_restore/<RUN_ID>/evidence_manifest.json`, `snapshot_bundle.bin.sha256`, `evidence.tar.gz`, and `evidence.tar.gz.sha256` | Per-run `evidence_manifest.json` records `snapshot_sha256`; `snapshot_bundle.bin.sha256` and `evidence.tar.gz.sha256` provide artifact checksums. | The deterministic drill is documented and automated for v2.2.20 evidence attachment; each closeout run must attach the generated manifest and checksums. |
| CI/workspace validation | PASS locally for this PR; final CI artifact still required for evaluated merge commit | Commands run on 2026-06-12 UTC from repository root: `cargo fmt --all -- --check`; `cargo check --workspace --locked`; `cargo test --workspace --locked`; `cargo clippy --workspace --all-targets -- -D warnings` | Terminal transcript from this PR run; external CI artifact/checksum to be attached by release manager for the evaluated merge commit. | Local workspace validation passed for this docs-only closeout PR, but mergeability still depends on acceptable CI or release-manager validation evidence for the final evaluated commit. |

## Waiver and limitation ledger

No complete closeout waiver is recorded in this repository for any non-PASS gate. Therefore the non-PASS `5N/2M`, non-PASS `5N/4M`, missing snapshot artifact, and missing final CI artifact are treated as blockers for a GO decision.

Remaining real limitations are maintained in `docs/KNOWN_LIMITATIONS_V2_2_20.md`. Fixed or narrowed historical blockers are not restated as active blockers unless replacement evidence is still required.

## GO/NO-GO decision

### Decision: `NO_GO`

`GO_TO_START_V2_3_0_REVIEW` is **not** recorded because the closeout criteria are not met.

Specific blockers:

1. `5N/2M intermediate` is not closeout-pass: the latest evidence failed the accepted-block gate with `MINER_NO_ACCEPTED_BLOCKS`, and no complete non-readiness waiver is recorded.
2. `5N/4M stress` is not closeout-pass: the latest stress evidence is `OBSERVE_FAIL`, and no accepted bounded limitation with owner, reviewer, UTC approval date, expiry, and exit criteria is recorded.
3. Snapshot restore now has deterministic automation and checksum manifest requirements; a final evaluated closeout run still must attach the generated restore artifact/checksum or record a formal waiver.
4. Final CI/workspace validation artifacts for the evaluated closeout merge commit must be attached and accepted before this evidence can support any future review start decision; local validation for this PR passed on 2026-06-12 UTC but is not a substitute for final evaluated-merge evidence.

## Allowed next state

The only allowed public-testnet signal remains:

```text
public_testnet_ready=false
```

A future PR may replace this `NO_GO` only after it attaches acceptable missing evidence or complete waivers and still keeps `VERSION=v2.2.20` unless separately approved later.
