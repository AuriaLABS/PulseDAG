# v2.3.0 start decision from v2.2.20 closeout

Date: 2026-07-19 UTC

## Decision

Decision: `GO_TO_START_V2_3_0_REVIEW`

The formal `v2.3.0` review may begin because the companion `v2.2.20` closeout record is PASS and all required closeout gates completed successfully in Actions run `29662737906` on candidate `e65c6c199e07214303b49f7863f5b4988a8ce107`.

## Authorized scope

The review may evaluate:

- remaining release-control and public-testnet prerequisites;
- security, RPC exposure, deployment, rollback and operations posture;
- network bootstrapping and isolation requirements;
- public-testnet monitoring and incident-response readiness;
- the separate proposal for a `2.3.0` version bump.

## Not authorized by this decision

- changing `VERSION` or Cargo package versions;
- tagging or publishing `v2.3.0` artifacts;
- claiming `v2.3.0` release readiness;
- launching a public testnet;
- setting `public_testnet_ready=true`;
- starting or backdating the 30-day burn-in clock;
- enabling smart contracts or embedded pool logic.

## Required next decision

A `2.3.0` version bump remains blocked until explicit maintainer approval is recorded after the formal review. Public-testnet launch remains subject to a separate GO/NO-GO decision and evidence set.
