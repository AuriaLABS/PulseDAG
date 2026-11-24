# PulseDAG first-network exact-candidate evidence ledger

Format 2 replaces the unused prototype dual-launch format. No backward compatibility is required. The validator is a structural evidence checker, not a signer, network controller or source of authorization. The only GO authority is issue #781; humans must independently verify the referenced comment, actual artifacts, timestamps, accepted activity, incident/reset history and gate evidence. A structurally valid synthetic record cannot authorize a launch.

## Sequential decisions

Use a separate exact-candidate ledger for testnet entry and later mainnet acceptance. Testnet does not require a frozen mainnet identity. Mainnet may use a different source/tree SHA and must reference the prior testnet evidence, not pretend earlier tests used its own SHA.

- `stage`: `testnet` or `mainnet`.
- `decision`: `PENDING`, `DELAY`, `NO_GO`, or the stage's own `GO_PUBLIC_TESTNET` / `GO_V3_MAINNET`.
- `decision_record`: null unless GO; then `issue_comment` (exact #781 comment URL) and `decided_at` (UTC).
- `prior_testnet`: null for testnet. For mainnet, the object below.
- `format`: `pulsedag-v3-evidence-ledger`; `format_version`: integer 2.

All other original exact source/tree, candidate protocol identities, artifact/config digests, evidence status, timestamp, invalidation and duplicate-gate checks remain. For testnet `networks` contains only `testnet`; for mainnet both `testnet` and `mainnet` must be declared and every identity field must differ. The active network configuration must be registered and referenced by every GO evidence record.

## Required gate identifiers

Testnet T0: native-consensus-monetary, local-native-regressions, testnet-only-activation, testnet-identity, public-security, operator-release, contracts-inactive.

Mainnet M0: production-release-security, native-replay-checkpoint, production-adversarial-25n16m, production-burn-in-168h, contracts-ready-inactive, mainnet-identity, public-testnet-acceptance.

All required and additional evidence must PASS on the exact stage candidate before a GO record. Production burn-in timestamp span must be at least 168 hours. These labels require the substantive acceptance criteria in the [launch runbook](../runbooks/V3_0_0_TESTNET_FIRST_LAUNCH.md), not simply matching strings.

## Prior testnet evidence

Mainnet must reference prior testnet source_sha, tree_sha, config_digest (matching the declared testnet), ledger_digest, go_record (same structure as decision_record), first_block_at, period_start, period_end and acceptance_evidence_digest. All digests use canonical sha256 form. UTC dates must follow GO, first accepted block, then at least 30 consecutive days. An invalidating reset restarts the relevant period. Independent review must verify actual meaningful public activity, 25-node/16-miner evidence, incident remediation, and contracts READY but INACTIVE. The validator cannot prove these facts from JSON.

## Validation

Run `python3 scripts/release/validate_v3_evidence_ledger.py --self-test`. To check a populated ledger, pass its JSON path. No actual GO ledger is committed while the candidate is unfrozen. The self-test uses deliberately synthetic identities and URLs, never deployment inputs. PRE-GO, mining OFF, contracts INACTIVE and mainnet NO-GO remain unchanged.
