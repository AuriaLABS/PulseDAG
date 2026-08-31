## Purpose

<!-- Explain the single primary purpose of this pull request. -->

## Changes

<!-- Summarize the implementation in reviewable groups. -->

## Impact

- Developer impact:
- Operator/user impact:
- Compatibility impact:
- v3.0.0 launch impact: <!-- launch-blocking / launch-enabling / post-launch / historical-only -->
- Workstream impact: <!-- v2.5 scale/GPU / v2.6 programmability / wallet / security / release / infrastructure -->
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

## Integrated v3.0.0 launch guardrails

- [ ] No existing v2.4.x tag/binary/evidence is relabeled as v3.0.0.
- [ ] The incorporated v2.5 scale/resilience/GPU requirements are not silently weakened or removed.
- [ ] The incorporated v2.6 programmability/smart-contract requirements are not silently weakened or removed.
- [ ] No unsupported launch/readiness claim is introduced.
- [ ] No mainnet/testnet chain ID, genesis, bootnode peer ID, DNS endpoint or production secret is invented before its freeze record.
- [ ] Mainnet and parallel-testnet identities remain explicitly separated when this PR touches network identity, P2P, wallet signing, relay, contracts/proofs or configuration.
- [ ] Any release-candidate-affecting change identifies which replay, burn-in, rehearsal, security, GPU, wallet or programmability evidence must be rerun.
- [ ] CPU/NVIDIA/AMD PoW equivalence remains canonical where mining code is affected.
- [ ] No embedded pool logic, share accounting, vardiff or payouts are added to the standalone miner/node.
- [ ] Smart-contract/programming changes remain bound to the frozen v3 transaction/VM/proof/resource/activation contracts.
- [ ] Contract/application/proof execution remains deterministic and resource bounded.
- [ ] No cross-network signing/application-domain replay path is introduced.

## Remaining limitations

<!-- State known limitations explicitly. Write "None" when there are none. -->
