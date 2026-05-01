# ROADMAP v2.3.0 — Private Testnet Readiness

## Positioning in the release sequence

- **v2.2.7 is the foundation-closing predecessor.** It finalizes PoW/mining/P2P groundwork and prepares repository boundaries for real private-testnet execution.
- **v2.3.0 is the first real private testnet readiness release.**
- **Smart contracts remain out of scope** for this phase.
- **No public testnet claim yet.** v2.3.0 is explicitly private testnet only.

## Goal

v2.3.0 defines the minimum complete operational baseline required to run a real multi-node private testnet and validate network behavior under repeatable operator drills.

## Scope and required deliverables

### 1) Real P2P network path

- Use **real P2P with `libp2p-real`** as the active networking path.
- Eliminate dependency on mocked or local-only transport assumptions for testnet flows.
- Verify stable peer discovery/connectivity behavior for long-running multi-node sessions.

### 2) Mining architecture constraints

- **External miner only** for block production inputs.
- **No pool logic in miner** (no embedded pool coordination, payout, or pooling control path inside miner).
- Maintain clear interface boundaries between node, miner, and any future pool components.

### 3) PoW correctness and evidence

- Deliver **complete PoW validation/evidence** end-to-end.
- Ensure all consensus-critical PoW checks are enforced at block acceptance.
- Produce operator-verifiable evidence artifacts/logs demonstrating validation decisions.

### 4) Multi-node PoW operation and network data propagation

- Run **active PoW across multiple nodes** through the external miner + node RPC path.
- Implement and validate **block propagation** across multi-node topology.
- Implement and validate **transaction propagation** across multi-node topology.
- Define measurable propagation expectations and failure diagnostics.

### 5) Node convergence, sync, and recovery

- Implement and validate **sync/catch-up between nodes** (including lagging/offline node recovery).
- Ensure deterministic catch-up behavior after temporary partition or delayed startup.

### 6) Mempool safety controls

- Enforce a **bounded mempool** policy with documented limits and eviction behavior.
- Validate bounded behavior under transaction burst conditions.

### 7) State lifecycle operations

- Run and document **snapshot/prune/restore drills**.
- Demonstrate repeatable node recovery from persisted state snapshots.
- Validate prune + restore paths do not compromise chain correctness.

### 8) Testnet operations enablement

- Provide **private testnet scripts** for bootstrap, node lifecycle, and standard drills.
- Provide **metrics, dashboards, and operator runbooks** for health checks, troubleshooting, and incident response.

## Exit criteria for v2.3.0

v2.3.0 is considered complete only when all items above are implemented, exercised in multi-node private environments, and documented with reproducible operational procedures.

## Non-goals

- Public testnet launch/announcement.
- Smart contract execution/runtime features.
- Feature claims beyond private testnet readiness gates.
