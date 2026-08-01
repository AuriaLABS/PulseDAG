# v2.4.0 Task 14 — Explicit single-node operator profile

## Objective

Add a first-class, fail-closed single-node operator profile for local development, deterministic burn-in, and operator validation without changing consensus, PoW, the approved release version, or public-testnet state.

The implementation must replace downstream operator patches with an official configuration contract while preserving the existing multi-node isolation safeguards.

## Branch and tracking

- Base branch: `release/2.4.0`.
- Implementation branch: `feature/2.4.0-single-node-profile`.
- Tracking issue: `#784`.
- Related defect: `#783`.

The v2.3.0 Windows burn-in environment remains an independent compatibility target and must not be modified by this task.

## Deliverables

1. Typed configuration and environment parsing for an explicit single-node mode.
2. Fail-closed validation for contradictory public, bootnode, and multi-host settings.
3. Startup identity and safety-summary output that clearly reports intentional isolation.
4. Preflight support for the explicit profile without weakening ordinary seed/node checks.
5. An operator configuration example with loopback RPC and no committed credentials.
6. Unit and integration tests for valid, invalid, and transition configurations.
7. Documentation for moving from single-node operation back to the ordinary private multi-node profile.

## Required invariants

- Single-node mode is disabled by default.
- Empty bootnodes or `role=seed` do not implicitly activate it.
- P2P is disabled or isolated by policy while the profile is active.
- RPC remains loopback-only by default.
- No bootnode is required in the explicit profile.
- Public-testnet readiness remains false.
- The 30-day public-testnet clock remains not started and cannot be enabled by this profile.
- Smart contracts remain disabled.
- Mining remains an external application.
- Existing ordinary private-testnet seed/node validation remains fail-closed.
- `VERSION`, Cargo package versions, tags, and release artifacts remain unchanged.

## Proposed configuration surface

The preferred minimal surface is:

```text
PULSEDAG_SINGLE_NODE_MODE=true
```

An equivalent typed profile is acceptable when it provides equal or stronger validation. The final implementation must expose one canonical operator contract rather than multiple ambiguous aliases.

## Invalid combinations

Startup or preflight must fail for at least these combinations:

- single-node mode with a non-loopback RPC bind;
- single-node mode with one or more bootnodes;
- single-node mode with public-testnet readiness enabled;
- single-node mode with the 30-day public-testnet clock enabled;
- single-node mode with a multi-host rehearsal profile;
- single-node mode with contradictory real-network advertisement claims;
- ordinary seed/node configuration attempting to inherit single-node behavior from an empty bootnode list.

Validation errors must identify the conflicting settings and the corrective action.

## Runtime identity

Startup logs and status surfaces must make the active topology unambiguous. At minimum, report:

- operator mode;
- whether P2P is enabled;
- whether connected peers are expected;
- RPC bind policy;
- network and chain identifiers;
- whether isolated mining is authorized by the explicit profile.

Do not represent intentional isolation as private-testnet or public-testnet readiness.

## Transition contract

Documentation and tests must cover the transition from single-node to ordinary private multi-node operation:

1. stop the isolated node cleanly;
2. disable the explicit single-node mode;
3. configure persistent P2P identity and valid bootnodes as required by role;
4. restore the normal private-testnet preflight;
5. verify that zero-peer mining protection is active again;
6. preserve or explicitly migrate storage only under a documented compatible chain identity.

Task 15 owns the topology-aware mining-template behavior. Task 14 must expose enough typed state for Task 15 to distinguish intentional single-node operation from accidental isolation.

## Validation

At minimum, add automated coverage equivalent to:

```text
explicit single-node + zero bootnodes + loopback RPC -> PASS
explicit single-node + public RPC bind -> FAIL
explicit single-node + bootnode configured -> FAIL
explicit single-node + public readiness flag -> FAIL
ordinary seed + empty bootnodes -> existing seed behavior only
ordinary node + empty bootnodes -> existing failure
single-node disabled after transition -> ordinary private validation restored
```

Normal repository lint, workspace, RPC, release, and security checks remain mandatory.

## Acceptance criteria

- An operator can start an intentional isolated node without patching repository scripts or configuration at runtime.
- The mode cannot be enabled accidentally.
- Existing v2.3.0 and ordinary v2.4.0 multi-node safety behavior remains unchanged.
- Startup identity and errors are actionable and unambiguous.
- Tests cover valid, invalid, and transition configurations.
- No version bump, release tag, public-testnet launch, or 30-day clock change is included.

## Out of scope

- Changing consensus or difficulty rules.
- Implementing the Task 15 mining-template guard changes.
- Fixing exporter route drift from Task 16.
- Fixing submit timeout semantics or liveness contention from Task 17.
- Publishing platform-specific credentials, wallets, or runtime data.
- Public-testnet launch, smart-contract activation, GPU mining, or pool protocols.
