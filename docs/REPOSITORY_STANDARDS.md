# Repository Standards

This document defines the repository-quality baseline for PulseDAG development.

## Goals

The repository should make the current product state obvious, keep historical evidence available without confusing it with active guidance, and allow a new contributor to build, test, and operate the project without reverse-engineering local conventions.

## Language

English is required for source-code comments, documentation comments, developer documentation, maintenance scripts, test diagnostics, commits, and pull requests.

User-facing localization may use another language only when localization is the feature. Technical prose must not mix languages.

## Active and historical material

- `README.md`, `CONTRIBUTING.md`, and current files directly under `docs/` describe active behavior.
- Historical roadmaps, release notes, closeout reports, and retired procedures belong under `docs/archive/`.
- Archive indexes must explain that archived files are historical and must use valid relative links.
- An active document must not instruct operators to run a missing or archived script.
- A historical file may not be deleted merely because it is old; it must first be classified as retained evidence, archive material, duplicate material, generated output, or safe deletion.

## Source and scripts

- Rust production code belongs in `apps/` or `crates/` according to ownership.
- Reusable operational scripts belong in `scripts/`.
- Script regression tests and fixtures belong in `scripts/tests/`.
- Temporary patchers, one-off migration payloads, and generated workflow helpers must be removed before merge unless they are explicitly retained as supported maintenance tools.
- Code comments explain why, invariants, failure modes, and operational constraints rather than repeating syntax.
- Public APIs and non-obvious internal contracts require documentation comments.
- Commented-out code is not retained; Git history is the archive.

## Configuration and secrets

- Versioned examples belong in `configs/` and use obvious placeholders.
- Local configuration belongs in ignored `.env` or runtime files.
- Private keys, seed phrases, tokens, credentials, private certificates, and operator secrets are prohibited in version control.
- Private-testnet identities and RocksDB state must use persistent operator-owned paths outside the source tree.

## Generated and runtime files

The following are never active source artifacts:

- Rust build output;
- node databases and snapshots generated at runtime;
- logs, PID files, temporary files, coverage output, and editor metadata;
- uncompressed evidence directories and ad-hoc archives;
- generated mining templates or local wallet data.

Curated evidence summaries may be committed when they are small, reviewed, checksummed where appropriate, and clearly tied to an evaluated commit.

## Documentation quality

- Local Markdown links must resolve.
- Current docs use current paths and commands.
- Examples are safe by default and do not expose remote administrative APIs without authentication.
- Readiness, release, and public-testnet statements must match formal decision records.
- Long documents should include clear headings and avoid duplicating canonical specifications.

## Pull-request quality

- One primary purpose per pull request.
- Separate protocol behavior, repository cleanup, version bumps, and launch decisions whenever practical.
- Explain risk, rollback, and validation.
- Add regression coverage for behavior changes.
- Do not merge temporary CI, diagnostic payloads, or unrelated generated changes.

## Cleanup workflow

1. Run `scripts/list_cleanup_candidates.sh` to produce an inventory.
2. Classify every proposed move or deletion.
3. Preserve required historical and release evidence under `docs/archive/`.
4. Remove generated, duplicate, stale, or unsafe material in a focused pull request.
5. Run `bash scripts/repository_hygiene.sh --strict`.
6. Attach the cleanup report and note any intentionally retained exception.

The hygiene gate is fail-closed for tracked generated files, secret-like files, broken active-document links, missing referenced scripts/configuration, version inconsistency, and detectable non-English code comments.
