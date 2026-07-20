# PulseDAG v2.3.0 release-candidate approval record

- Decision: `APPROVE_RELEASE_CANDIDATE`
- Maintainer: `kalekoi`
- Decision date: `2026-07-20 UTC`
- Validated proposal SHA: `4a3d4e3df587f9bd6f438ddd7359a5148f0cff8e`
- Proposal merge commit: `fec0b304a2544245826e5f799d9932d157818d43`
- Task 12 accepted candidate: `22fa09b19da2893fa73b91b198b26675bd1e6e32`
- Task 12 workflow run: `29773225491`
- Task 12 artifact SHA-256: `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`
- Task 13 proposal workflow run: `29775577934`
- Task 13 proposal artifact SHA-256: `3394b467734f9064e86ea342030344938d9d1e74964d3176321ab4c6545a3b6f`

## Rationale

The protected five-node private-testnet rehearsal produced an independently reviewed `GO`. The Task 13 proposal bound the change inventory, compatibility and rollback statement, release-note draft, supported asset matrix, and fail-closed exact-candidate gate plan to one validated proposal SHA. The proposal manifest passed with `VERSION=v2.2.20`, Cargo `2.2.20`, `version_bump_authorized=false`, `public_testnet_ready=false`, and the public-testnet clock not started.

## Authorization boundary

This approval authorizes only a separate versioned release-candidate pull request that:

1. updates `VERSION` and workspace Cargo package versions to `2.3.0`;
2. regenerates `Cargo.lock` without dependency upgrades;
3. finalizes the v2.3.0 release notes and candidate decision metadata;
4. reruns every required workspace, P2P, RPC/release, packaging, smoke, evidence, lifecycle, observability, runbook, and repository-hygiene gate on the exact candidate SHA;
5. records a final private-testnet release decision before any tag or publication.

This approval does not authorize a `v2.3.0` tag, GitHub Release publication, public-testnet launch, `public_testnet_ready=true`, smart contracts, or the start/backdating of the 30-day public-testnet clock.

## Remaining risks and required follow-up

- The exact versioned candidate has not yet passed its post-version-bump gates.
- Release archives and provenance attestations have not yet been produced for v2.3.0.
- No tag or release may be created until the final candidate evidence is reviewed and the final private-testnet release decision is recorded.
