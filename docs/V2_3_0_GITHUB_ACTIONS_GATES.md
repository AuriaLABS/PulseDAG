# v2.3.0 GitHub Actions gates

These workflows turn the remaining `v2.2.20 -> v2.3.0 review` work into manually runnable, commit-pinned GitHub Actions gates.

They do not change `VERSION`, do not claim public-testnet readiness, and do not start the 30-day public-testnet clock.

## Workflows

| Workflow | Purpose | Closeout behavior |
|---|---|---|
| `v2_3_0_workspace_validation.yml` | `fmt`, workspace check, bounded package-by-package tests and clippy | Strict: every command and package suite must pass; timeouts produce diagnostics. |
| `v2_3_0_staged_network_gate.yml` | Runs `5N/1M -> 5N/2M -> 5N/4M` sequentially | Strict: every stage must PASS, use the same commit and produce an archive checksum. |
| `v2_3_0_mempool_tx_relay_gate.yml` | Mempool bounds, relay coverage and five-node transaction drill | `unit-contract` is diagnostic. `runtime-closeout` requires real five-node evidence. |
| `v2_3_0_lag_injection_gate.yml` | Selected-tip inventory and selected-segment lag recovery | `schema-only` is synthetic and never closeout-eligible. `runtime-closeout` requires an actual isolated node. |
| `v2_3_0_prune_restart_rejoin_gate.yml` | Non-zero pruning, snapshot restart and offline rejoin | `test-gate` is diagnostic. `runtime-closeout` requires an operational drill. |
| `v2_3_0_candidate_closeout_gate.yml` | Calls all gates for one candidate SHA | Emits `GO_TO_START_V2_3_0_REVIEW` only when all runtime-closeout jobs pass. |

## Running a gate

After these workflows exist on the default branch:

1. Open **Actions**.
2. Select the desired `v2.3.0` workflow.
3. Choose **Run workflow**.
4. Enter one exact `candidate_sha`, or leave it blank to use the triggering/default-branch SHA.
5. For closure, select `runtime-closeout`.

Do not combine artifacts produced from different candidate commits.

## Candidate closeout workflow

The recommended entry point is:

```text
v2.3.0 candidate closeout gate
```

Inputs:

- `candidate_sha`: exact candidate commit;
- `mode=runtime-closeout`: required for a GO decision;
- `mode=diagnostic`: allows contract/schema checks but always records `NO_GO`;
- staged mining duration and quiescence wait;
- minimum selected-height gap, default `96`;
- package test timeout, default `10` minutes for each workspace package;
- isolated real-swarm P2P initialization timeout, default `120` seconds;
- Rust test threads per package, default `1`;
- independent timeouts for `cargo check` and `cargo clippy`.

The final decision artifact contains:

```json
{
  "result": "PASS|FAIL",
  "closeout_eligible": true,
  "decision": "GO_TO_START_V2_3_0_REVIEW|NO_GO",
  "public_testnet_ready": false,
  "thirty_day_public_testnet_clock_started": false
}
```

`closeout_eligible=true` is impossible in diagnostic mode.

## Bounded workspace tests

The workspace gate does not run one unbounded `cargo test --workspace` process. It discovers workspace packages from `cargo metadata` and executes each package independently:

```text
cargo test -p <package> --locked -- --nocapture --test-threads=<n>
```

Each package has its own timeout. The gate continues after a failure or timeout so that Clippy, the manifest and the evidence upload still run.

The cancelled candidate run `29241831089` identified one specific stalled test:

```text
pulsedag_p2p::tests::real_runtime_mode_initializes_without_loopback_labeling
```

The P2P package is therefore executed in two explicit cases:

1. the remaining `pulsedag-p2p` suite with that test skipped;
2. the real-swarm initialization test alone, with `--test-threads=1` and its own short hard timeout.

This is isolation, not a waiver: the workspace gate remains failed unless the isolated test exits successfully. If it times out, any surviving `pulsedag_p2p` test process is terminated before the workflow continues.

For every test case it records:

- start and end timestamps;
- duration;
- exit code;
- `PASS`, `FAIL`, or `TIMEOUT`;
- complete test log.

A failed or timed-out case also produces a diagnostic file containing:

- process table;
- process tree;
- listening sockets;
- the final 400 lines of test output.

The final `test-results.json` identifies the exact package and case that failed or stalled. Any timeout makes the workspace gate fail; it is never treated as a waiver or an acceptable retry.

## Evidence artifacts

Each workflow uploads:

- exact candidate SHA;
- command/test logs;
- gate manifest;
- runtime manifest where applicable;
- SHA-256 checksum inventory;
- staged `evidence.tar.gz` bundles for 1M, 2M and 4M.

A final gate step reads the generated manifest and propagates a non-zero exit when the gate is not satisfied. Uploading an artifact does not by itself make a gate PASS.

## Runtime drivers still required

The workflows deliberately reject missing or synthetic runtime evidence.

### Transaction relay

Expected executable:

```text
scripts/v2_3_0_mempool_tx_relay_evidence.sh
```

Required runtime manifest properties include:

- five nodes;
- relay convergence;
- duplicate suppression;
- capacity/rejection taxonomy;
- confirmation cleanup;
- deterministic final mempool sets;
- `public_testnet_ready=false`.

### Selected-segment lag injection

Existing script:

```text
scripts/v2_3_0_lag_injection_selected_segment.sh
```

`CI_MODE=1` creates schema-only synthetic evidence and is explicitly not closeout-eligible. Runtime mode must isolate `n5`, create a real selected-height gap of at least 64, use correlated locator/header/block requests, apply chunks and converge.

### Prune/restart/rejoin

Expected executable:

```text
scripts/v2_3_0_prune_restart_rejoin_runtime.sh
```

The runtime manifest must prove:

- `blocks_pruned_total > 0`;
- retained storage/memory digest equality;
- snapshot+delta restart;
- selected-tip and state-root equality after restart;
- network advancement while the node is offline;
- successful rejoin and final convergence.

The existing `scripts/v2_3_0_04_prune_restart_rejoin_evidence.sh` remains a test gate and cannot substitute for the operational drill.

## Release-control guardrails

Until a runtime-closeout run passes all jobs:

```text
VERSION=v2.2.20
Cargo workspace version=2.2.20
public_testnet_ready=false
Decision=NO_GO
```

Even a successful `GO_TO_START_V2_3_0_REVIEW` does not authorize public-testnet launch or start the 30-day public-testnet burn-in clock.
