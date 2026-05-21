# v2.2.18 Release Artifact Preparation

This runbook defines how to prepare **local release artifacts** for `v2.2.18`.

It is intentionally limited to artifact assembly and validation.

## Scope and guardrails

- Prepare artifacts for Linux `x86_64` first.
- Document Windows/WSL path as an operator workflow note.
- Keep GPU builds optional and separate from the base CPU release flow.
- Do **not** publish artifacts automatically.
- Do **not** sign artifacts when signing keys are unavailable.
- Do **not** include secrets (tokens, private keys, credentials, env files with secrets).
- Do **not** claim production readiness from this process alone.

## Required outputs

The process must generate the following files under a run-scoped artifact directory:

1. `pulsedagd` release binary
2. `pulsedag-miner` release binary
3. `checksums.txt`
4. `version.txt`
5. `git_commit.txt`
6. `build_environment.txt`
7. `release_notes_copy.md`
8. `evidence_checklist_copy.md`

## Linux x86_64 flow

### 1) Run artifact script

```bash
bash scripts/v2_2_18_build_release_artifacts.sh
```

Optional environment knobs:

- `RUN_ID` (default: UTC timestamp)
- `ARTIFACT_ROOT` (default: `artifacts/v2_2_18_release_artifacts`)
- `RELEASE_NOTES_PATH` (default: `docs/RELEASE_NOTES_V2_2_18.md`)
- `EVIDENCE_CHECKLIST_PATH` (default: `docs/CLOSING_CHECKLIST_V2_2_18.md`)

### 2) Verify artifact directory contents

After completion, inspect the output directory printed by the script. It should include:

- `bin/pulsedagd`
- `bin/pulsedag-miner`
- `checksums.txt`
- `version.txt`
- `git_commit.txt`
- `build_environment.txt`
- `release_notes_copy.md`
- `evidence_checklist_copy.md`

## Windows/WSL documentation path

Use WSL to execute the same script and retain artifacts in a Linux path (recommended), then copy to a Windows-accessible directory if needed.

Example:

```bash
# inside WSL
bash scripts/v2_2_18_build_release_artifacts.sh

# optional: copy to Windows Downloads
cp -r artifacts/v2_2_18_release_artifacts/<run_id> /mnt/c/Users/<user>/Downloads/
```

Keep the same guardrails: no auto-publish, no unsigned signing claims, no secrets in copied outputs.

## Optional GPU artifact path (separate)

GPU-enabled miner artifacts are out of scope for the base CPU flow and should be tracked as a separate run with clear labeling.

Suggested pattern:

- Use a distinct run id suffix (example: `...-gpu`)
- Keep separate checksum manifest and environment summary
- Document GPU driver/toolchain versions explicitly

## Non-goals

This process does not:

- create a GitHub/GitLab release,
- upload binaries,
- sign artifacts without keys,
- certify production readiness.
