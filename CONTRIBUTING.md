# Contributing to PulseDAG

Thank you for improving PulseDAG. Changes should be easy to review, easy to operate, and safe to maintain.

## Development language

English is the required language for:

- source-code comments and documentation comments;
- developer-facing documentation;
- commit messages;
- pull-request titles and descriptions;
- test names, diagnostics, and maintenance scripts.

Localized user-facing text is allowed when localization is the explicit feature being implemented. Do not mix languages inside technical comments.

## Commenting standard

Comments must explain intent, invariants, trade-offs, failure modes, or non-obvious constraints. Do not restate code that is already clear from names and structure.

Use:

- `//!` for Rust module-level documentation;
- `///` for public Rust APIs and important internal contracts;
- `// SAFETY:` before every `unsafe` block, describing the invariant that makes it safe;
- `TODO(<issue-or-owner>):` only when the removal condition is explicit;
- short English comments in shell and Python scripts for non-obvious control flow.

Do not commit:

- commented-out code;
- obsolete TODOs without an owner or issue;
- comments that contradict current behavior;
- generated comments copied from tools without review.

## Code quality

- Prefer small, cohesive changes and narrowly scoped pull requests.
- Use descriptive names and typed structures instead of implicit conventions.
- Keep error messages actionable and include relevant context.
- Preserve deterministic behavior in consensus, storage, networking, and evidence code.
- Add regression tests for every bug fix.
- Do not weaken fail-closed validation or evidence thresholds to make a check pass.
- Avoid hidden global state and unbounded retries.
- Keep the standalone miner boundary intact; do not embed pool logic in the miner.
- Smart-contract runtime work is out of scope unless a separate roadmap explicitly authorizes it.

## Repository structure

- Active documentation belongs in `docs/`.
- Historical release material belongs in `docs/archive/` and must not be presented as current guidance.
- Runtime output belongs outside version control in `data/`, `logs/`, `run/`, `artifacts/`, or `ci-evidence/`.
- Reusable scripts belong in `scripts/`; test-only helpers belong in `scripts/tests/`.
- Example configuration belongs in `configs/` and must contain placeholders, never credentials.

See `docs/REPOSITORY_STANDARDS.md` for the complete structure and cleanup policy.

## Required validation

Run the checks relevant to the change. The normal baseline is:

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/repository_hygiene.sh --strict
```

Large runtime or release changes must also run the dedicated workflow and evidence gate for their task.

## Pull requests

A pull request must explain:

- what changed;
- why the change is needed;
- user, operator, or developer impact;
- risk and rollback behavior;
- validation performed;
- any remaining limitation.

Keep version bumps, release publication, and public-testnet launch decisions in separate explicitly authorized pull requests.

## Security and secrets

Never commit private keys, operator tokens, seed phrases, credentials, real `.env` files, production endpoints containing secrets, or unredacted incident data. Use placeholders in examples and report security issues privately to the maintainers.
