# v2.2.20 Start Plan

Date: 2026-06-05

## Purpose

`v2.2.20` starts the next hardening cycle after the `v2.2.19` Docker closeout evidence.

This is **not** a public-testnet readiness declaration and does **not** start the 30-day burn-in clock.

## Starting point

`v2.2.19` closeout evidence established:

- `5N/1M baseline`: PASS
- `5N/2M intermediate`: PASS
- `5N/4M stress`: OBSERVE_FAIL, accepted as a tracked limitation

## Primary objective

Make `5N/4M` stress bounded, recoverable, and diagnostically complete without changing consensus semantics.

## Workstreams

### 1. Orphan recovery and parent fetch backpressure

- Bound parent fetch/orphan recovery fanout under high block pressure.
- Ensure pending missing parent queues expose deterministic metrics.
- Avoid repeated fetch storms against unavailable parents.

### 2. Peer retention under stress

- Explain and then reduce peer drop-to-zero behavior under 4-miner stress.
- Add diagnostics for disconnect reasons and peer scoring/state transitions.
- Preserve enough peer connectivity for recovery after mining quiescence.

### 3. Inflight request limits and retry/fallback behavior

- Cap inflight block requests per peer and globally.
- Make retries/fallbacks observable and bounded.
- Avoid unbounded timeout accumulation.

### 4. RPC responsiveness during stress

- Keep `/status`, `/p2p/status`, `/readiness`, `/sync/status`, `/sync/missing`, and `/orphans` responsive during stress and final capture.
- Prefer degraded/stale-but-fast responses over RPC starvation.

### 5. Docker evidence continuity

- Keep Docker rehearsals as the reproducible local evidence path through `docs/DOCKER_REHEARSALS_V2_2_20.md`, `docker-compose.rehearsal.yml`, and `scripts/docker_v2_2_20_rehearsal.sh`.
- Mandatory regression checks remain:
  - `5N/1M baseline`: PASS
  - `5N/2M intermediate`: PASS
- `5N/4M stress` should improve from `OBSERVE_FAIL` toward bounded failure or PASS.
- CI evidence workflows must upload `evidence.tar.gz` and `evidence.tar.gz.sha256` for `5N/1M`, `5N/2M`, and observe-only `5N/4M` runs.

## Exit criteria

Minimum for `v2.2.20` closeout:

- `VERSION=v2.2.20` and Cargo workspace version `2.2.20` are aligned.
- `cargo build --workspace --release --locked` passes.
- Docker `5N/1M baseline` remains PASS.
- Docker `5N/2M intermediate` remains PASS.
- Docker `5N/4M stress` does not cause unbounded RPC starvation during final capture.
- Orphan and pending-missing-parent backlogs are bounded or explicitly classified.
- Any remaining non-PASS stress result has deterministic reason, owner, expiry, and exit criteria.

Stretch goal:

- Docker `5N/4M stress` reaches PASS after quiescence with one final tip and zero missing-parent backlog.

## Guardrails

- No consensus-rule breaking changes.
- No PoW semantic changes.
- No smart contracts.
- No pool logic.
- Miner remains external.
- No public-testnet readiness claim.
- No v2.3.0 readiness claim.
