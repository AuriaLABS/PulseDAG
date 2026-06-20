# v2.2.20 Closing Checklist

This checklist closes `v2.2.20` active hardening only. It must not be used to claim public-testnet live status, public-testnet readiness, or `v2.3.0` readiness.

## Required closeout evidence

| Gate | Required state | Evidence path / notes |
|---|---|---|
| Version guard | PASS | Attach `cat VERSION` showing `v2.2.20`, root Cargo `[workspace.package] version = "2.2.20"`, and a diff proving no `v2.3.0` bump is included. |
| Public-testnet signal guard | PASS | `public_testnet_ready=false` remains the required value for all closeout materials; any true/ready/live assertion is automatic **NO-GO**. |
| Workspace validation | PASS | Attach logs for `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets -- -D warnings`. |
| `5N/1M baseline` | PASS | Attach private evidence archive, sha256, manifest, final node table, convergence metrics, orphan/missing-parent counts, peer counts, and miner accept/reject summary. |
| `5N/2M intermediate` | PASS or approved non-readiness waiver | Attach replacement evidence after all remaining `v2.2.20` hardening PRs. If non-PASS, waiver must explicitly say public-testnet readiness and `v2.3.0` readiness are not claimed. |
| `5N/4M stress` | PASS or accepted non-blocking limitation | Attach replacement stress evidence after PRs `#600`-`#614` and any later `v2.2.20` hardening PRs, including RPC liveness/final-capture behavior, peer visibility, final tips, orphan/missing-parent backlog, miner metrics, archive, sha256, and manifest. |
| Snapshot/restore | PASS or approved waiver | Attach deterministic restore drill evidence from `docs/SNAPSHOT_RESTORE_DRILL_V2_2_20.md`, or waiver. |
| Known limitations | PASS | Attach decision-scoped mapping from `docs/KNOWN_LIMITATIONS_V2_2_20.md` showing remaining limitations, resolved/narrowed limitations, owners, expiry, and exit criteria. |
| Incident ledger | PASS | Attach incident list proving no unresolved Sev-1 consensus/sync/security blocker. |
| Evidence integrity | PASS | Every referenced bundle has an archive, checksum, evaluated commit, UTC timestamp, and reproducible command or workflow name. |

## Waiver requirements

Waivers are allowed only for `v2.2.20` hardening closeout scope. They cannot convert a non-PASS gate into public-testnet readiness or `v2.3.0` readiness.

A waiver is valid only when it records all of the following:

- gate name and exact non-PASS condition;
- owner and reviewer;
- UTC approval date;
- evaluated commit and branch;
- scope and explicit exclusions;
- expiry date or event;
- exit criteria;
- risk classification;
- statement that the waiver does not authorize public-testnet readiness, public-testnet live status, or `v2.3.0` readiness.

Missing waiver metadata is automatic **NO-GO** for the affected gate.

## GO/NO-GO rules

### `GO_TO_START_V2_3_0_REVIEW`

Allowed only when:

- all mandatory evidence gates are PASS, or any non-PASS gate has a complete waiver that is explicitly non-readiness and non-public-testnet;
- `5N/1M` is PASS;
- `5N/2M` has replacement evidence after all remaining `v2.2.20` hardening PRs, or a complete non-readiness waiver;
- `5N/4M` has replacement evidence after PRs `#600`-`#614` and any later `v2.2.20` hardening PRs, and any non-PASS outcome is accepted as a bounded limitation;
- no unresolved Sev-1 consensus/sync/security blocker exists;
- `VERSION` remains `v2.2.20` and Cargo workspace version remains `2.2.20`;
- `public_testnet_ready=false` remains the only public-testnet signal.

### `NO_GO`

Required when any of the following is true:

- missing `5N/1M` evidence;
- missing replacement `5N/2M` evidence or missing waiver for a non-PASS result;
- missing replacement `5N/4M` evidence or missing accepted limitation for a non-PASS result;
- evidence archive, checksum, evaluated commit, or manifest is missing for a required bundle;
- unresolved Sev-1 consensus/sync/security blocker exists;
- `VERSION` or Cargo workspace version is bumped without explicit maintainer approval;
- any document claims public-testnet ready, public-testnet live, or `v2.3.0` ready status;
- a waiver omits owner, reviewer, UTC approval, scope, expiry, exit criteria, or the required non-readiness statement.

### `WAIVED_WITH_REASON`

Allowed only for explicitly scoped non-PASS gates that are not public-testnet readiness gates. The final decision must list each waiver and explain why it is safe for `v2.2.20` hardening closeout only.


## Final v2.2.20 evidence index

The final closeout index is `docs/V2_2_20_FINAL_EVIDENCE_INDEX.md`. As of 2026-06-12, the recorded decision is `NO_GO`, not `GO_TO_START_V2_3_0_REVIEW`, because the repository does not yet contain acceptable replacement or waiver evidence for all required closeout gates.

Current blockers recorded by the index:

- `5N/2M intermediate` remains non-PASS because the latest accepted evidence failed the accepted-block gate.
- `5N/4M stress` remains observe-only/non-PASS without an accepted bounded limitation.
- Snapshot restore automation is documented, but a final closeout restore bundle/checksum or waiver is not attached.
- Final CI/workspace validation artifacts must be attached for the evaluated closeout commit.

This `NO_GO` preserves `public_testnet_ready=false`, makes no public-testnet live claim, and makes no `v2.3.0` readiness claim.

## Final decision record

Record the final decision at `artifacts/v2_2_20/closeout_decision/final_decision.md` with:

- `GO_TO_START_V2_3_0_REVIEW`, `NO_GO`, or `WAIVED_WITH_REASON`;
- evaluated commit and branch;
- reviewer and UTC date;
- evidence bundle paths and checksums;
- incident and waiver ledger links;
- explicit statement that `public_testnet_ready=false` remains unchanged;
- explicit statement that the decision does not claim public-testnet readiness or public-testnet live status;
- explicit statement that any future `v2.3.0` work starts from a separate start decision and no `VERSION` bump is included here.
