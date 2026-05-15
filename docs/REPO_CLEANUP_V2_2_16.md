# PulseDAG v2.2.16 repository cleanup policy

This document records the repository hygiene rules for the v2.2.16 hardening line.

## Goals

- Keep source, release notes, roadmaps, checklists, and curated evidence summaries tracked.
- Keep bulky generated runtime artifacts out of git.
- Keep local node data out of git.
- Keep editor and operating-system noise out of git.
- Preserve the external-miner architecture and avoid adding pool or embedded-node-miner artifacts during cleanup.

## Tracked by default

The following files should remain tracked when they are intentional project artifacts:

- Rust source files under `crates/` and `apps/`.
- Workspace files such as `Cargo.toml`, `Cargo.lock`, and `VERSION`.
- Documentation under `docs/`.
- Release evidence summaries written as Markdown.
- Release scripts under `scripts/`.
- Small deterministic fixtures used by tests.

## Ignored by default

The following files are generated or local-only and should not be committed:

- `target/` build output.
- `data/` node runtime databases.
- Evidence logs and runtime node directories under `evidence/**/logs/`, `evidence/**/runtime/`, and `evidence/**/node-*`.
- P2P rehearsal runtime directories and raw log folders.
- Temporary files such as `*.tmp`, `*.log`, editor swap files, and OS metadata.
- Local `.env` files, except an intentional `.env.example`.

## Evidence policy

Release evidence should be split into two classes:

1. Curated summaries that are stable and reviewable, usually Markdown files.
2. Raw runtime/log artifacts that are useful locally but too noisy for normal git history.

Raw artifacts should stay under `evidence/<version>/` locally and be attached to release discussions or CI artifacts when needed, instead of being committed by default.

## v2.2.16 cleanup scope

This cleanup updates repository ignore rules so the next miner/node contract evidence work does not accidentally commit generated logs, node runtime state, local data, or editor noise.

It intentionally avoids changing consensus, mining, P2P, storage, or RPC behavior.

## Recommended local cleanup commands

Run these from the repository root before opening release PRs:

```bash
git status --short
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
```

To inspect potentially ignored/generated files locally:

```bash
git status --ignored --short
```

Do not delete curated docs, release notes, roadmaps, checklists, or evidence summaries unless a PR explicitly explains why they are obsolete.
