# Local wallet / keyless node boundary

PulseDAG v2.4.x separates wallet key custody from the network node.

## Normal architecture

`pulsedag-wallet` is the local custody/signing boundary. It owns encrypted keystore handling, bounded `WalletSession` unlock state, deterministic seed derivation, watch-only backup verification, reviewed transaction plans and local transaction signing.

`pulsedagd` / `pulsedag-rpc` is keyless. It must not generate, accept, decrypt, return or sign with wallet private keys, seeds, mnemonics or wallet passwords over HTTP/RPC.

The historical `/wallet/new`, `/wallet/sign` and `/wallet/transfer` paths are permanent fail-closed `404` tombstones. They are not an end-user wallet API and there is no Cargo feature that restores the removed raw-key handlers.

## Development and rehearsal automation

Development/rehearsal transaction generation uses the local `pulsedag-wallet-harness` binary rather than node custody. The harness creates an encrypted v2 deterministic-seed keystore locally, accepts its password only through stdin, reads public UTXO data, builds a reviewed transaction plan, signs inside a bounded `WalletSession`, and emits only a fully signed transaction envelope.

The node receives that envelope through canonical `POST /api/v1/tx/submit`. The local harness is automation for repository smoke/rehearsal flows; it is not an official end-user wallet UX or a production-custody claim.

## Security invariants

- no raw private key, seed, mnemonic or wallet password crosses the node RPC boundary;
- `WalletSession` is not exposed as an HTTP service;
- historical raw-key wallet route names remain fail-closed;
- `public_safe` remains wallet-secret-free and exposes only the narrow signed-transaction relay write boundary;
- the local rehearsal harness accepts passwords through stdin, never command-line arguments;
- protocol-level nonce/replay/RBF/submission-identity/domain decisions remain tracked separately in #821;
- this boundary does not authorize production/mainnet custody or change public-testnet launch control.
