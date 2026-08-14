# Local wallet / keyless node boundary

PulseDAG v2.4.x separates wallet key custody from the network node.

## Normal architecture

`pulsedag-wallet` is the local custody/signing boundary. It owns encrypted keystore handling, bounded `WalletSession` unlock state, deterministic seed derivation, watch-only backup verification, reviewed transaction plans and local transaction signing.

A normal `pulsedagd` / `pulsedag-rpc` build is keyless. It must not generate, accept, decrypt, return or sign with wallet private keys, seeds, mnemonics or wallet passwords over HTTP/RPC.

The historical `/wallet/new`, `/wallet/sign` and `/wallet/transfer` paths remain only as fail-closed `404` tombstones in the normal build while old development/rehearsal harnesses are migrated. They are not an end-user wallet API.

## Temporary legacy compatibility

The non-default `legacy-wallet-rpc` Cargo feature restores the historical raw-key wallet handlers only for explicit development/rehearsal compatibility. It must not be enabled by normal release or pre-burn node builds.

Compatibility builds do not change the target architecture: signing belongs in the local wallet boundary. Once historical transaction-generation harnesses use local signing and signed-transaction submission, the compatibility feature and tombstones should be removed.

## Security invariants

- no raw private key, seed, mnemonic or wallet password crosses the normal node RPC boundary;
- `WalletSession` is not exposed as an HTTP service;
- `public_safe` remains wallet-secret-free and fail-closed;
- signed-transaction relay is a separate narrow public boundary and must never accept custody secrets;
- protocol-level nonce/replay/RBF/submission-identity/domain decisions remain tracked separately in #821;
- this boundary does not authorize production/mainnet custody or change public-testnet launch control.
