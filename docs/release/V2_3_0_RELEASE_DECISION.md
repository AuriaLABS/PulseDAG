# PulseDAG v2.3.0 release decision

## Current decision

`PENDING_MAINTAINER_DECISION`

## Proposal under review

- Base tag: `v2.2.20`.
- Base commit: `14a1c38249830ee6912d8e70d6d223126cf7f63b`.
- Task 13 activation baseline: `928c25b81ed13b539d9d2b5930609cc97430b9a3`.
- Exact proposal SHA: recorded by the Task 13 proposal workflow.
- Proposal document: `docs/release/V2_3_0_RELEASE_PROPOSAL.md`.
- Draft release notes: `docs/release/V2_3_0_RELEASE_NOTES_DRAFT.md`.

## Accepted prerequisite

Task 12 protected private-testnet `GO`:

- candidate `22fa09b19da2893fa73b91b198b26675bd1e6e32`;
- workflow run `29773225491`;
- artifact SHA-256 `a31246a014e88287e653b732c5edf54af08d26f5d0ffac19f60b49f369db88ce`;
- all nine mandatory phases `PASS`;
- 56/56 controller checksums verified;
- independent 55-snapshot endpoint audit passed;
- `version_bump_authorized=false` preserved.

## Required decision outcomes

Replace `PENDING_MAINTAINER_DECISION` with exactly one of:

- `APPROVE_RELEASE_CANDIDATE`;
- `REQUEST_CHANGES`;
- `NO_GO`.

A decision update must include the maintainer, date, exact proposal SHA, rationale, unresolved risks, and required follow-up.

## Approval effect

`APPROVE_RELEASE_CANDIDATE` authorizes only a separate follow-up PR that:

1. changes `VERSION` from `v2.2.20` to `v2.3.0`;
2. changes Cargo package versions from `2.2.20` to `2.3.0`;
3. updates final release notes and candidate metadata;
4. reruns every required CI, P2P, release, packaging, smoke, evidence, and hygiene gate on the exact versioned candidate;
5. records the final private-testnet release decision before any tag or publication.

Approval does not authorize a public testnet or start the 30-day public-testnet clock.

## Current guardrails

- `VERSION=v2.2.20`.
- Cargo version `2.2.20`.
- No `v2.3.0` tag.
- No v2.3.0 artifact publication.
- `public_testnet_ready=false`.
- `thirty_day_public_testnet_clock_started=false`.
- Smart contracts remain out of scope.
