# Ubuntu v2.2.19 Closeout Runtime Evidence Rerun Guide

## Purpose

This guide is for a **clean Ubuntu 24.04 rerun of only the previously failed runtime evidence collection** after static checks have already passed.

- Scope: rerun runtime evidence only (do not repeat unrelated static validation work).
- Release status: **v2.2.19 remains pre-public-testnet**.

## Prerequisites

Run from repository root:

```bash
cd /path/to/PulseDAG
```

## 1) Clean stale processes

```bash
pgrep -a pulsedagd || true
pgrep -a pulsedag-miner || true
pkill -f pulsedagd || true
pkill -f pulsedag-miner || true
```

## 2) Clean stale runtime artifacts

```bash
rm -rf artifacts/v2_2_19/local_3n_1m_smoke
rm -rf artifacts/v2_2_19/private_5n_4m_rehearsal
mkdir -p artifacts/v2_2_19/local_3n_1m_smoke
mkdir -p artifacts/v2_2_19/private_5n_4m_rehearsal
```

## 3) Syntax-check both runtime scripts

```bash
bash -n scripts/v2_2_19_local_3n_1m_smoke.sh
bash -n scripts/v2_2_19_private_5n_4m_rehearsal.sh
```

## 4) Rebuild locked release binaries

```bash
cargo build --workspace --release --locked
```

## 5) Run local 3N/1M runtime evidence

```bash
OUT_DIR=artifacts/v2_2_19/local_3n_1m_smoke \
DURATION_SECS=900 \
bash scripts/v2_2_19_local_3n_1m_smoke.sh \
2>&1 | tee artifacts/v2_2_19/local_3n_1m_smoke/run.log
```

## 6) Run private 5N/4M runtime evidence

```bash
OUT_DIR=artifacts/v2_2_19/private_5n_4m_rehearsal \
DURATION_SECS=1800 \
bash scripts/v2_2_19_private_5n_4m_rehearsal.sh \
2>&1 | tee artifacts/v2_2_19/private_5n_4m_rehearsal/run.log
```

## 7) Verify `evidence.tar.gz` checksums

```bash
sha256sum artifacts/v2_2_19/local_3n_1m_smoke/evidence.tar.gz \
  | tee artifacts/v2_2_19/local_3n_1m_smoke/evidence.tar.gz.sha256
sha256sum -c artifacts/v2_2_19/local_3n_1m_smoke/evidence.tar.gz.sha256

sha256sum artifacts/v2_2_19/private_5n_4m_rehearsal/evidence.tar.gz \
  | tee artifacts/v2_2_19/private_5n_4m_rehearsal/evidence.tar.gz.sha256
sha256sum -c artifacts/v2_2_19/private_5n_4m_rehearsal/evidence.tar.gz.sha256
```

## 8) Package final closeout evidence

```bash
mkdir -p artifacts/v2_2_19/closeout

tar -czf artifacts/v2_2_19/closeout/v2_2_19_runtime_rerun_closeout_evidence.tar.gz \
  artifacts/v2_2_19/local_3n_1m_smoke \
  artifacts/v2_2_19/private_5n_4m_rehearsal

sha256sum artifacts/v2_2_19/closeout/v2_2_19_runtime_rerun_closeout_evidence.tar.gz \
  > artifacts/v2_2_19/closeout/v2_2_19_runtime_rerun_closeout_evidence.tar.gz.sha256
sha256sum -c artifacts/v2_2_19/closeout/v2_2_19_runtime_rerun_closeout_evidence.tar.gz.sha256
```

## PASS criteria

All must be true:

- No unbound variable errors.
- `evidence-summary` reports overall `PASS`.
- Healthy/ready node counts are nonzero.
- Chain height is nonzero.
- Template count is nonzero.
- Accepted blocks are `> 0`, or an explicit waiver is documented.
- `evidence.tar.gz` exists for each run.
- `sha256sum -c` validates each evidence package.

## NO-GO criteria

Any one of the following is an immediate no-go:

- Any shell crash during execution.
- False PASS signal (reported PASS while required metrics are invalid).
- Missing evidence package(s).
- Zero network progress (e.g., zero height/templates throughout run).
- Any claim or flag that sets `public_testnet_ready=true` in v2.2.19.

## Expected outcome

If all PASS criteria are satisfied and no NO-GO criterion occurs, publish the rerun evidence bundle for review while keeping release posture unchanged:

- **v2.2.19 remains pre-public-testnet**.
