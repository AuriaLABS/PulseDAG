# PulseDAG v3 — first network, native-only compatibility policy

**Project decision (2026-11-22):** PulseDAG has not launched any operating
network. Its first network will be the **public native-v3 testnet**, followed
by the first native-v3 mainnet only after the independent launch gates.

## No previously deployed network to support

- v1/v2/Core artifacts are internal source, fixtures, diagnostic tools and
  experiments. They are **not historical production chain identities**.
- There is no old chain history, UTXO set, account balance, deployed peer,
  wallet state, RPC protocol, serialized block or network bootstrap identity
  requiring backward compatibility or a migration to v3.
- Native-v3 block, transaction, P2P, storage and consensus formats can be
  finalized independently. Do not design compatibility layers, legacy-accept
  fallbacks, v2-to-v3 migrations or remote old-node sync as network requirements.
- Internal tests may temporarily reuse old Core fixtures to test isolated
  transport behavior. They must **never** be counted as proof of native-v3
  monetary consensus, network acceptance or production readiness.
- Prefer genuine canonical `BlockHeaderV3`, chain-bound nanosecond PoW and
  authentic native DAG ancestry in new tests. Retirement of prototype
  fixtures must not weaken negative safety tests.

## What the first testnet *still* must prove

1. Independently frozen genesis, chain ID, signature domain, bootnodes,
   wallet/network addresses and storage namespace dedicated to testnet. No
   testnet balances or history are transferable to mainnet.
2. Approved native-v3 GHOSTDAG and GD-08 ordering, signed transactions,
   conflict handling, fees/rewards, UTXO consistency and deterministic replay.
3. Authenticated real daemon P2P recovery with five nodes and at least
   32 genuinely admitted native-v3 blocks during isolation, followed by
   reconnection, pruning and restart evidence. Merely matching local replicas
   or valid standalone PoW headers is **not sufficient**.
4. Contracts remain `INACTIVE` / fail-closed for v3.0. Production monetary
   mining and direct GetBlock remain OFF until separately reviewed and
   authorized. No shortcut by making an unreviewed prototype path live.
5. Separate explicit `GO_PUBLIC_TESTNET` on
   [launch issue #781](https://github.com/AuriaLABS/PulseDAG/issues/781);
   at least 30 days of meaningful public-testnet evidence, then another
   independent `GO_V3_MAINNET`. Local/offline CI diagnostics are not a live
   private testnet requirement.

Track native P2P in [#1465](https://github.com/AuriaLABS/PulseDAG/issues/1465),
GD-08 in [#1410](https://github.com/AuriaLABS/PulseDAG/issues/1410), and
public-testnet readiness in [#1418](https://github.com/AuriaLABS/PulseDAG/issues/1418).

This policy **removes unnecessary compatibility obligations**, not consensus,
cryptographic validation, safety or launch requirements.
