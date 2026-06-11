# v2.2.20 deterministic snapshot restore drill

Date: 2026-06-11

## Scope

This drill gives operators a bounded, repeatable way to prove that a snapshot bundle can be exported from one private node and imported into a fresh data directory without changing the resulting height or selected tip.

This is **pre-public-testnet hardening evidence only**. It does **not** claim public-testnet readiness and does **not** claim v2.3.0 readiness.

## Automation

Run the drill from the repository root:

```bash
scripts/v2_2_20_snapshot_restore_drill.sh
```

For CI or a quick local gate, use bounded runtime mode:

```bash
CI_MODE=1 BUILD_NODE=1 scripts/v2_2_20_snapshot_restore_drill.sh --ci
```

The script:

1. starts a private, single-node `pulsedagd` instance with P2P disabled;
2. mines deterministic local-dev blocks through `/mine` until `HEIGHT_THRESHOLD` is reached;
3. creates an admin snapshot through `/admin/snapshot/create`;
4. stops the original node;
5. exports `snapshot_bundle.bin` from the original RocksDB directory;
6. writes `snapshot_bundle.bin.sha256` for the exported snapshot artifact;
7. imports the bundle into a fresh RocksDB directory;
8. starts a restored node from that fresh directory;
9. verifies `chain_id`, height, selected tip, block count, snapshot height, and snapshot checksum evidence;
10. emits `evidence_manifest.json` and `evidence.tar.gz` with a tarball checksum.

## Configuration knobs

| Variable | Default | Purpose |
|---|---:|---|
| `RUN_ID` | current UTC timestamp | Artifact/run directory suffix. |
| `ARTIFACT_ROOT` | `artifacts/v2_2_20_snapshot_restore` | Evidence root. |
| `DATA_ROOT` | `run/v2_2_20_snapshot_restore/<RUN_ID>` | Temporary RocksDB root. |
| `NODE_BIN` | `target/debug/pulsedagd` | Node binary to run. |
| `BUILD_NODE` | `0` | Set `1` to build `pulsedagd` if the binary is absent. |
| `RPC_PORT` | `29220` | Local drill RPC port. |
| `CHAIN_ID` | `pulsedag-restore-drill-v2-2-20` | Private drill chain id. |
| `HEIGHT_THRESHOLD` | `3` | Minimum height to mine before snapshot. |
| `START_TIMEOUT_SECONDS` | `60` | Node readiness timeout. |
| `MINE_MAX_TRIES` | `1000000` | Bound per local-dev mining RPC. |
| `CI_MODE` | `0` | Set by `--ci`; intended for bounded CI evidence. |

## Evidence outputs

Each run writes:

- `summary.md`
- `snapshot_bundle.bin`
- `snapshot_bundle.bin.sha256`
- `snapshot_export_report.json`
- `snapshot_import_report.json`
- `original_status.json`
- `restored_status.json`
- `restore_report.json`
- `evidence_manifest.json`
- `original-node.log`
- `restored-node.log`
- `evidence.tar.gz`
- `evidence.tar.gz.sha256`

The restore gate passes only when the original and restored summaries agree on:

- `chain_id`
- `best_height`
- `selected_tip`
- `block_count`
- `snapshot_height`
- non-empty snapshot artifact checksum

## Metadata validation tests

The script has parser-only test hooks so CI can validate snapshot metadata shape without launching a node:

```bash
bash scripts/v2_2_20_snapshot_restore_drill.sh --validate-snapshot-metadata metadata.json
bash scripts/v2_2_20_snapshot_restore_drill.sh --compare-summaries original_status.json restored_status.json
```

`pytest scripts/tests/test_v2_2_20_snapshot_restore_drill.py` covers required metadata fields, empty selected-tip rejection, checksum comparison, and tip mismatch detection.

## v2.2.20 evidence attachment

Latest local validation on 2026-06-11 used the CI-friendly command below:

```bash
CI_MODE=1 BUILD_NODE=1 HEIGHT_THRESHOLD=2 scripts/v2_2_20_snapshot_restore_drill.sh --ci
```

Expected evidence path pattern:

```text
artifacts/v2_2_20_snapshot_restore/<RUN_ID>/evidence_manifest.json
artifacts/v2_2_20_snapshot_restore/<RUN_ID>/evidence.tar.gz
artifacts/v2_2_20_snapshot_restore/<RUN_ID>/evidence.tar.gz.sha256
```

Attach those files to the v2.2.20 evidence package for operator review. Keep this evidence separate from public-testnet readiness materials until the wider v2.2.20 hardening gates are closed.
