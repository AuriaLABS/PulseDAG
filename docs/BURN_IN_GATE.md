# v2.4.0 public-testnet burn-in gate

The public-testnet acceptance clock is a post-launch evidence gate. It is not started by a version bump, release-candidate freeze, private burn-in, tag, package build, or release publication.

## Before public launch

The v2.4.0 release candidate must first complete the required private validation sequence: exact-SHA validation, operational private burn-in, restart/snapshot/prune/restore/rejoin evidence, the mandatory 5-node/4-miner rehearsal, security disposition, release identity freeze, infrastructure readiness and an explicit public launch decision.

Until `GO_PUBLIC_TESTNET` is recorded and the public network is actually launched:

- `public_testnet_ready=false`;
- `thirty_day_public_testnet_clock_started=false`;
- `contracts_enabled=false`.

## Day 0

Day 0 is the exact UTC timestamp of the first separately authorized successful public-testnet launch, together with the recorded release/genesis/network/configuration identity and first accepted public block/height. The clock must never be backdated to private testing, a release tag, or an earlier candidate.

## 30-day evidence

During the accepted public-testnet period, preserve sufficient evidence to review consensus/state convergence, mining and transaction finality, P2P/sync health, storage/recovery, resource use, security/abuse controls, operator incidents and any hard-stop condition.

Any invalidated interval or hard-stop event must be handled according to the accepted public-testnet policy and recorded in the launch-control evidence. Smart contracts remain blocked until the required accepted public-testnet period is complete and a separate activation decision is recorded.

## Version-change rule

Any source or fixed release-configuration change after an exact v2.4.0 candidate freeze creates a new candidate identity and requires the affected validation to be repeated. Evidence from different SHAs must not be combined.
