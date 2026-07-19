# v2.3.0 Task 08 — Repository professionalization and developer standards

## Objective

Make the repository easier to understand, review, operate, and extend without deleting required historical evidence or weakening release guardrails.

## Required changes

1. Add a contribution guide and repository standards document.
2. Require English for code comments, documentation comments, developer docs, commit messages, pull requests, tests, and maintenance scripts.
3. Add editor defaults and a pull-request template.
4. Replace version-pinned cleanup validation with a current, version-agnostic repository hygiene gate.
5. Detect tracked generated/runtime files, secret-like files, broken active-document links, stale references, and non-English code comments.
6. Produce a cleanup candidate inventory that reports historical root clutter, large source files, TODO/FIXME debt, archived references, and likely generated material.
7. Add regression tests and a dedicated GitHub Actions workflow.
8. Update `.gitignore` for common build, coverage, Python, editor, and runtime output.

## Commenting requirements

- Comments must be written in English.
- Comments explain intent, invariants, trade-offs, and failure behavior.
- Public Rust APIs and non-obvious internal contracts use documentation comments.
- Every unsafe block requires a `SAFETY:` explanation.
- TODO comments include an issue or owner and an explicit removal condition.
- Commented-out code is removed.

## Cleanup safety

- Do not mass-delete historical evidence.
- Classify every proposed move or deletion first.
- Active docs must not reference archived or missing scripts.
- Generated output and secrets are immediate removal candidates.
- Historical roadmaps and release evidence should be archived with indexes when they remain relevant.

## Validation

```bash
python3 scripts/check_code_comment_language.py
bash scripts/repository_hygiene.sh --strict
bash scripts/tests/test_repository_hygiene.sh
```

The dedicated workflow must upload a cleanup inventory and fail when mandatory hygiene checks fail.

## Guardrails

- Keep `VERSION=v2.2.20` and Cargo workspace version `2.2.20`.
- Keep `public_testnet_ready=false`.
- Do not start the 30-day public-testnet clock.
- No consensus, PoW, miner architecture, or smart-contract runtime change.
