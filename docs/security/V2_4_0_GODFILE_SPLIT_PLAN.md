# v2.4.0 god-file split plan (#1130)

Status: **planning + test-module inventory**. Does not change consensus, P2P wire,
or Task31 identity. Does not authorize a public-testnet GO.

## Why this is P2

The 2026-09-14 audit filed `#1130` because review risk, not because a single
fail-open bug lived in these files. Fail-open children of `#1132` are already
closed. Clippy `--all-targets -D warnings` still requires the remaining
`block_request.rs` `#[allow(dead_code)]` markers on helpers used from tests.

## Snapshot on current `main`

`crates/pulsedag-p2p/src/lib.rs` is still **11,406 lines / ~459 KiB**.
Message codecs already live under `messages/`. Live sync files are already
extracted.

## `#[cfg(test)]` modules still inside `lib.rs`

Counted 2026-09-16 against `main`:

| Line | Module | First peel? |
|---:|---|---|
| 4241 | `protocol_block_hash_tests` | **Yes** — ~90 lines, three tests, no swarm |
| 6966 | `tests` | No — large topology/inbound suite |
| 8902 | (next `cfg(test)` block) | No — after the big `tests` module |
| 9586 | (next `cfg(test)` block) | No |
| 11218 | (last `cfg(test)` block) | No |

Do **not** move all five in one PR. The GitHub contents API cannot rewrite
`lib.rs` as a single blob safely; use a one-shot `git apply` workflow like
`#1153`, and only for `protocol_block_hash_tests` first.

Proposed destination: `crates/pulsedag-p2p/src/protocol_block_hash_tests.rs`
with `#[cfg(test)] mod protocol_block_hash_tests;` left in `lib.rs`.

## Allowed split order

1. Inventory (this document).
2. Move `protocol_block_hash_tests` only.
3. Move remaining `cfg(test)` blocks one at a time.
4. Peel identity / peer-score / dial tables.
5. Peel swarm event loop last.
6. Repeat for `pulsedagd` `main.rs`.

## Explicit non-goals

- Deleting `block_request` allows without a green `lint.yml` proof (#1156).
- Compiling `tx.rs` only under `cfg(test)` (#1162). Production `tx_protocol`
  re-exports that module (#1160).
- Treating this plan as Task31 evidence.
