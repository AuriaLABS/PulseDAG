# Chaos restart/recovery validation suite (v2.2.4 closeout)

This runbook defines a **repeatable operator validation suite** for crash, restart, recovery, and churn.

Design constraints for this suite:
- does **not** change consensus behavior,
- keeps miner external and standalone,
- introduces no pool logic,
- focuses on scripts, validation workflow, and evidence outputs.

## 1) Why this suite exists

Before closing v2.2.4, operators need a practical way to validate the highest-value failure modes repeatedly and attach evidence to release closeout.

This suite provides:
1. A fixed scenario manifest (priority + SLO targets).
2. Guided execution that captures pre/post status snapshots.
3. Deterministic evidence files that can be attached to release package artifacts.

## 2) Scenario coverage (highest-value first)

P0 scenarios (must pass):
1. `crash-restart-node-b` — abrupt validator crash + restart + reconvergence.
2. `graceful-restart-seed-a` — seed restart without prolonged cluster drift.
3. `peer-churn-isolate-rejoin` — node isolation + rejoin with clean sync convergence.
4. `recovery-snapshot-restore` — restore/rebuild recovery drill returns to healthy participation.

P1 scenario (should pass):
5. `external-miner-churn` — external miner detach/reattach and mining flow recovery.

## 3) Prerequisites

1. Stable multi-node environment (minimum: 3 nodes, 1+ external miner).
2. Run ID chosen for evidence package (example: `v2.2.4-chaos-2026-04-26`).
3. Operator access to process/network controls needed for crash/churn actions.
4. Runbook familiarity:
   - `docs/runbooks/RECOVERY_ORCHESTRATION.md`
   - `docs/runbooks/P2P_RECOVERY.md`
   - `docs/runbooks/FAST_BOOT_AND_FALLBACK.md`

## 4) Execute the suite

```bash
scripts/chaos/run-validation-suite.sh \
  --run-id v2.2.4-chaos-YYYYMMDD \
  --node-urls http://127.0.0.1:8080,http://127.0.0.1:8081,http://127.0.0.1:8082 \
  --scenario-manifest scripts/chaos/scenarios.csv
```

What this script does:
- Creates `artifacts/release-evidence/<run_id>/chaos-suite/`.
- Copies a fixed scenario catalog (`scripts/chaos/scenarios.csv`) into `manifest.csv`.
- Captures pre/post snapshots of `/health`, `/sync/status`, and `/runtime/status` for each node.
- Prompts operator for each real-world action (crash, restart, isolate, recover).
- Produces both `summary.md` and `scenario-outcomes.csv` with pass/fail, duration, and SLO met/not-met flags.
- Writes `run-info.json` so auditors can reconstruct run parameters exactly.

### Non-interactive mode (lab rehearsal)

```bash
scripts/chaos/run-validation-suite.sh --run-id dryrun --yes
```

Use `--yes` only for dry rehearsal of evidence wiring; do not treat as production validation evidence.

## 5) Validate evidence completeness

```bash
scripts/chaos/validate-evidence.sh --run-id v2.2.4-chaos-YYYYMMDD
```

The evidence check fails if:
- required files are missing,
- scenario manifest is empty,
- summary outcomes do not cover all listed scenarios.
- endpoint capture count is lower than expected for scenarios × nodes × endpoints × pre/post.

Then create an immutable archive payload for release evidence transfer:

```bash
scripts/chaos/archive-evidence.sh --run-id v2.2.4-chaos-YYYYMMDD
```

This emits:
- `chaos-suite-<run_id>.tar.gz`
- `chaos-suite-<run_id>.tar.gz.sha256`

## 6) Required artifacts for release closeout

Attach at least these outputs under the run ID:

```text
artifacts/release-evidence/<run_id>/chaos-suite/
  manifest.csv
  events.csv
  summary.md
  scenario-outcomes.csv
  run-info.json
  raw/*.json
```

Recommended attachments in the same run package:
- operator incident notes and ticket links,
- alert timeline exports,
- runtime events extracts around each perturbation window.

## 7) Pass/fail policy

The suite is considered release-eligible when:
1. All P0 scenarios pass in one contiguous run window.
2. Any P1 failures are explained, mitigated, and re-tested.
3. No unresolved Sev-1 consensus/sync incident remains.
4. Evidence files are complete and auditable by a reviewer not involved in execution.

If a P0 scenario fails, fix, re-baseline, and re-run the full suite with a new run ID.
