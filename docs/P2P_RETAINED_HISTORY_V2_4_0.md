# v2.4.x retained-history peer capability

Issue #824 adds pruning-awareness to selected-chain catch-up without changing consensus, block validity, pruning policy, or global peer connectivity.

## Wire contract

`TipInventoryStatus.prune_boundary_height` is an optional, backward-compatible capability field:

- `Some(0)` means the peer explicitly claims continuously bridgeable selected history from genesis.
- `Some(h)` for `h > 0` means `h` is the earliest selected-chain height from which the peer can continuously bridge historical catch-up to its current selected tip.
- `None` means the capability is unknown, including legacy peers that predate the field. Unknown must never be interpreted as an archival/full-history claim.

The advertised value is derived from the node's actually retained continuous selected-chain suffix. A special retained anchor below a gap does not lower the advertised boundary and therefore cannot make a pruned node look archival.

## Restart and pruning behavior

A full-history node advertises `Some(0)` while its retained selected-chain state is continuously bridgeable from genesis. After compact pruning and restart, the node advertises the earliest retained selected-chain boundary it can actually bridge. If the node cannot determine that boundary truthfully, it advertises `None` rather than guessing from configured retention such as `keep_blocks`.

The same value is propagated into the existing remote selected-tip inventory exposed by P2P diagnostics; no second capability source is introduced.

## Directed historical catch-up

For a local selected height `L` and a remote explicit boundary `B`, the remote peer is incompatible as a directed historical bridge when `B > L + 1`.

Both priority-gap and reconcile peer selection apply the same rule. Explicitly incompatible peers are excluded only from directed historical catch-up; they remain connected and usable for gossip/current data. When an explicitly compatible peer and a legacy/unknown peer can both satisfy the gap, the explicitly compatible peer is preferred. Unknown-only peers remain usable as a mixed-version fallback, with the existing `retained_history_gap` fail-fast path remaining the safety net if the fallback cannot bridge the requested history.

This metadata reduces impossible historical requests and retry loops. It does not replace the requirement for archival seed/bootstrap peers until PulseDAG has a checkpoint/state-sync mechanism.
