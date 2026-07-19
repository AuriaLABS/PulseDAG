# Private-testnet operator helpers

This directory contains supported host-local tooling for PulseDAG private-testnet operations.

## Node lifecycle controller

`node_lifecycle.py` is the canonical implementation. `node_lifecycle.sh` is a thin compatibility wrapper.

The controller manages one node under an operator-owned root:

```text
<root>/
├── releases/<release-id>/
│   ├── pulsedagd
│   └── manifest.json
├── current -> releases/<release-id>
├── previous -> releases/<release-id>
├── logs/pulsedagd.log
├── run/pulsedagd.pid
├── run/lifecycle.lock
└── state/lifecycle.json
```

Release directories are immutable. Activation uses atomic symlink replacement, and all mutating operations are serialized with a file lock.

## Required arguments

Every command receives:

- `--root`: persistent operator-owned lifecycle directory;
- `--env-file`: host-local private-testnet environment file;
- optionally `--preflight-script`: Task 07 configuration contract path.

Example command prefix:

```bash
lifecycle=(
  python3 scripts/private_testnet/node_lifecycle.py
  --root /var/lib/pulsedag/lifecycle
  --env-file /etc/pulsedag/private-testnet.env
)
```

## Common operations

Install the first binary without starting it:

```bash
"${lifecycle[@]}" install \
  --binary ./dist/pulsedagd \
  --release-id v2.3.0-rc1
```

Validate configuration, ownership, links, checksums, and bootnode resolution:

```bash
"${lifecycle[@]}" verify
```

Start, inspect, restart, and stop:

```bash
"${lifecycle[@]}" start
"${lifecycle[@]}" status
"${lifecycle[@]}" restart
"${lifecycle[@]}" stop
```

Upgrade to a new immutable release and wait for health:

```bash
"${lifecycle[@]}" upgrade \
  --binary ./dist/pulsedagd-next \
  --release-id v2.3.0-rc2
```

If the new process exits or `/health` does not become ready, the controller restores the prior release and restarts it when the node was previously running.

Explicit rollback:

```bash
"${lifecycle[@]}" rollback
```

## Safety properties

- Environment files are parsed as data; shell expansion and command substitution are rejected.
- Task 07 preflight runs before process start.
- DNS bootnodes resolve before start unless an operator explicitly uses `--allow-unresolved-bootnodes` for an offline drill.
- Managed directories must be owned by the current user and must not be group/world writable.
- PID reuse is detected using Linux process start ticks; a mismatched live PID is never signalled.
- Logs, PID files, lifecycle state, release checksums, and timestamps are persisted outside the repository.
- Reusing a release identifier with different binary content is rejected.
- `VERSION`, release publication, public-testnet readiness, and the 30-day clock are outside this controller's authority.

## Development contract

Run:

```bash
python3 -m py_compile scripts/private_testnet/node_lifecycle.py
bash -n scripts/private_testnet/node_lifecycle.sh
bash scripts/tests/test_v2_3_0_node_lifecycle.sh
```

Code comments, docstrings, diagnostics, and operator documentation must remain in English.
