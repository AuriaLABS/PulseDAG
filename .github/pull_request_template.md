## Purpose

<!-- Explain the single primary purpose of this pull request. -->

## Changes

<!-- Summarize the implementation in reviewable groups. -->

## Impact

- Developer impact:
- Operator/user impact:
- Compatibility impact:
- v3.0.0 launch impact: <!-- launch-blocking / launch-enabling / post-launch / historical-only -->
- Network impact: <!-- none / mainnet / parallel testnet / both -->

## Risk and rollback

<!-- Describe failure modes, migration concerns, evidence invalidation, and the rollback path. -->

## Validation

<!-- List exact commands, workflows, runtime drills, and evidence artifacts. -->

- [ ] Formatting completed.
- [ ] Relevant tests passed.
- [ ] Clippy passes with warnings denied where applicable.
- [ ] Repository hygiene passes.
- [ ] New or changed code comments are in English.
- [ ] No credentials, generated runtime data, wallet secrets, or temporary patch files are included.
- [ ] Exact candidate SHA/evidence scope is recorded when this change affects release or launch behavior.

## v3.0.0 launch guardrails

- [ ] No existing v2.4.x tag/binary/evidence is relabeled as v3.0.0.
- [ ] No unsupported launch/readiness claim is introduced.
- [ ] No mainnet/testnet chain ID, genesis, bootnode peer ID, DNS endpoint or production secret is invented before its freeze record.
- [ ] Mainnet and parallel-testnet identities remain explicitly separated when this PR touches network identity, P2P, wallet signing, relay or configuration.
- [ ] Any release-candidate-affecting change identifies which burn-in/rehearsal/security/package evidence must be rerun.
- [ ] No embedded pool logic in the standalone miner.
- [ ] No smart-contract runtime enablement unless included in a separately reviewed/frozen launch or activation decision.

## Remaining limitations

<!-- State known limitations explicitly. Write "None" when there are none. -->
