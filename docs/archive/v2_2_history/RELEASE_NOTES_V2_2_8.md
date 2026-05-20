# PulseDAG v2.2.8 Release Notes

## Summary
v2.2.8 closes the ambitious pre-private-testnet hardening baseline with aligned versions, honest runtime/public status messaging, CLI profile override stability, and release closure artifacts.

## Version
- `v2.2.8`

## What changed
- PoW hardening.
- Canonical PoW preimage foundation.
- Target/difficulty foundation.
- Unified block acceptance behavior.
- Mining RPC hardening.
- P2P inventory/announcement/deduplication.
- Orphan and missing-parent recovery foundations.
- Private/local/operator profiles.
- Local multi-node lab workflow.
- Observability closure for runtime/mining/P2P/sync signals.

## Intentionally not included
- No production-readiness claim.
- No public testnet launch.
- No official full private testnet launch.
- No smart contracts.
- No pool logic inside miner.
- No final long burn-in closure.

## Known limitations
- Final PoW algorithm production compatibility remains deferred where still incomplete.
- Full multi-node burn-in is deferred to v2.3.0.
- Operator dashboards/runbooks may still require expansion.
- P2P/sync still needs broader multi-machine validation.

## Upgrade notes
- Align `VERSION` and workspace package version to `v2.2.8` / `2.2.8`.
- Rebuild and verify `/status`, `/release`, and `/pow` reflect this release framing.
- Preserve explicit CLI overrides when using `--network` profiles.

## Next milestone
- v2.3.0: complete private-testnet readiness, multi-node burn-in, sync/recovery depth, and operator readiness closure.
