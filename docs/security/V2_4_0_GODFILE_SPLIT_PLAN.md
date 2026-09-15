# v2.4.0 god-file split plan (#1130)

Status: **planning only**. Does not change consensus, P2P wire, or Task31 identity.
Does not authorize a public-testnet GO.

## Why this is P2

The 2026-09-14 audit filed `#1130` because review risk, not because a single
fail-open bug lived in these files. Fail-open children of `#1132` are already
closed. Clippy `--all-targets -D warnings` still requires the remaining
`block_request.rs` `#[allow(dead_code)]` markers on helpers used from tests.

## Snapshot on current `main`

Approximate sizes (blob bytes):

| Path | Bytes | Notes |
|---|---:|---|
| `crates/pulsedag-p2p/src/lib.rs` | ~459 KiB | Still the swarm/runtime god-file |
| `crates/pulsedag-p2p/src/messages/` | extracted | Capability, DAG sync, fast-sync, wire limits |
| `live_fast_sync_v1.rs` | ~19 KiB | Already out of `lib.rs` |
| `live_protocol_sync_v1.rs` | ~19 KiB | Already out of `lib.rs` |
| `runtime.rs` | ~1.3 KiB | Thin |
| `apps/pulsedagd/src/main.rs` | large | Node orchestration |
| `apps/pulsedagd/src/block_request.rs` | ~1.7k lines | Allows stay until helpers are `#[cfg(test)]` *and* clippy-clean |

The original "11,406 lines all in one file" finding is **partially stale**:
message codecs already live under `messages/`.

## Allowed split order

One concern per PR. No wire/`codec_id` change. No Kaspa bump.

1. **Inventory only** (this document).
2. Move `#[cfg(test)]` modules out of `pulsedag-p2p/src/lib.rs` into
   `src/tests_*.rs` or `tests/` **without** changing production items.
3. Peel identity / peer-score / dial tables from `lib.rs` if they are already
   sectioned by comments.
4. Peel swarm event loop last.
5. Repeat for `pulsedagd` `main.rs` (config and request paths already have
   sibling files).

## Explicit non-goals

- Deleting `block_request` allows without a green `lint.yml` proof (#1156).
- Compiling `tx.rs` only under `cfg(test)` (#1162). Production `tx_protocol`
  re-exports that module (#1160).
- Treating this plan as Task31 evidence.
