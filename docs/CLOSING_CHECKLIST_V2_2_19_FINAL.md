# v2.2.19 Final Closeout Evidence Checklist

This checklist is the required closeout path for `v2.2.19`.

**Decision posture:** `v2.2.19` remains **pre-public-testnet hardening**. This checklist does **not** authorize public testnet launch, does **not** claim `v2.3.0` readiness, and does **not** claim `v3.0` readiness.

> Rule: do not mark PASS unless the evidence path exists and is reproducible. Default status is PENDING.

## Version sanity

- [ ] PASS / [x] PENDING: `VERSION` is exactly `v2.2.19`. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: workspace crate versions are exactly `2.2.19`. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: `docs/VERSION_MATRIX.md` remains aligned with `v2.2.19` hardening status (no public-testnet readiness claim). Evidence path: `____________________`

## Cargo.lock sanity

- [ ] PASS / [x] PENDING: `Cargo.lock` is present and consistent with `--locked` commands. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: no unexplained `Cargo.lock` drift after validation commands. Evidence path: `____________________`

## Formatting/check/clippy/tests

Run and attach logs for all required commands:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] PASS / [x] PENDING: format/check/test/clippy command logs attached. Evidence path: `____________________`

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

- [ ] PASS / [x] PENDING: preflight script completes and artifacts are recorded. Evidence path: `____________________`

## Local 3N/1M smoke evidence

Run and attach output/artifacts (`OUT_DIR` must be a real writable path):

```bash
OUT_DIR=... bash scripts/v2_2_19_local_3n_1m_smoke.sh
```

- [ ] PASS / [x] PENDING: local `3N/1M` smoke completes with evidence bundle. Evidence path: `____________________`

## Private 5N/4M rehearsal evidence

Run and attach output/artifacts (`OUT_DIR` must be a real writable path):

```bash
OUT_DIR=... bash scripts/v2_2_19_private_5n_4m_rehearsal.sh
```

- [ ] PASS / [x] PENDING: private `5N/4M` rehearsal completes with evidence bundle. Evidence path: `____________________`

## Snapshot/restore evidence

- [ ] PASS / [x] PENDING: snapshot creation evidence attached (command logs + artifact paths). Evidence path: `____________________`
- [ ] PASS / [x] PENDING: restore/rebuild drill evidence attached (expected post-restore checks included). Evidence path: `____________________`

## P2P convergence evidence

- [ ] PASS / [x] PENDING: multi-node convergence evidence attached (peer visibility + selected tip convergence). Evidence path: `____________________`
- [ ] PASS / [x] PENDING: restart/rejoin behavior evidence attached for rehearsal topology. Evidence path: `____________________`

## Miner external protocol evidence

- [ ] PASS / [x] PENDING: miner/node contract remains external-mode only for `v2.2.19`. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: mining protocol rehearsal evidence attached for declared topology. Evidence path: `____________________`

## GPU scaffold/fallback evidence

- [ ] PASS / [x] PENDING: GPU path status is explicitly documented as optional/scaffold unless canonical kernel evidence is provided. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: CPU fallback/compatibility behavior evidence attached for environments without GPU path enablement. Evidence path: `____________________`

## RPC readiness/release metadata evidence

- [ ] PASS / [x] PENDING: `/release` metadata reflects `v2.2.19` runtime truths (no stale algorithm/engine fields). Evidence path: `____________________`
- [ ] PASS / [x] PENDING: `/status` and `/readiness` evidence attached for operator-facing fields in scope. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: RPC exposure posture remains private/localhost unless explicitly hardened and approved. Evidence path: `____________________`

## Known limitations accepted for v2.2.19

- [ ] PASS / [x] PENDING: limitations acceptance is explicitly aligned with `docs/KNOWN_LIMITATIONS_V2_2_19.md`. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: closeout record states `v2.2.19` is private-testnet hardening and not a public launch declaration. Evidence path: `____________________`

## Blockers before 2.3.0 public testnet

Record open blockers that must be closed before any `v2.3.0` public-testnet go/no-go:

- [ ] PASS / [x] PENDING: public-testnet launch checklist gates are satisfied and evidenced. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: unresolved private rehearsal issues are tracked with owners and due dates. Evidence path: `____________________`
- [ ] PASS / [x] PENDING: no closeout text makes readiness claims beyond `v2.2.19` scope. Evidence path: `____________________`

## Final sign-off

- [ ] PASS / [x] PENDING: all required sections above are PASS with evidence links.
- [ ] PASS / [x] PENDING: disposition recorded as one of `CLOSED_WITH_EVIDENCE` or `WAIVED_WITH_REASON`.
