# PulseDAG public signed-transaction relay v1

## Purpose

The public relay is a narrow network submission boundary for already signed PulseDAG transactions. It is intentionally separate from wallet key custody and from operator/admin RPC.

The canonical endpoint is:

```text
POST /api/v1/tx/submit
Content-Type: application/json
```

The JSON body is exactly:

```json
{
  "transaction": { "...": "fully formed PulseDAG Transaction" }
}
```

Unknown top-level fields are rejected. In particular, callers must never send a private key, seed, mnemonic, wallet password, decrypted keystore payload, or local wallet-session material to this endpoint.

## Admission semantics

The relay delegates the submitted transaction to the existing canonical RPC transaction-admission path (`accept_transaction(..., AcceptSource::Rpc)`). A transaction is propagated to peers only after local admission succeeds and accepted state is persisted.

This endpoint does not build transactions, select UTXOs, choose fees, create nonces, or sign. Those operations remain local wallet responsibilities.

Rejected transactions use the existing `TX_REJECTED` API error contract. Malformed or non-canonical relay envelopes use `invalid_transaction_payload`. Declared oversized requests use `request_too_large`, and rate-limit rejections use `rate_limited`.

## PublicSafe isolation

Enabling the relay does not expose other write surfaces on the `PublicSafe` listener. In particular, the following remain unavailable:

- transaction building (`/tx/build`);
- legacy wallet routes (`/wallet/*`);
- admin/operator routes (`/admin/*`);
- mining submission and mining control;
- snapshot, prune, sync-rebuild, or other operator mutation routes.

The unversioned compatibility path `/tx/submit` remains forbidden on `PublicSafe`; the public relay contract is the versioned `/api/v1/tx/submit` path only.

## Abuse controls

The relay inherits the configured `PublicSafe` hardening policy. The default public profile is bounded to a 128 KiB request body and 30 requests per 60 seconds per IP, with bounded rate-limit key tracking. Custom `RpcHardeningLimits` continue to apply in tests or deployments that intentionally override those defaults.

Cross-origin browser requests remain deny-by-default. Only exact origins present in `PULSEDAG_RPC_CORS_ALLOWLIST` may use the relay from a browser, and relay preflight responses advertise only `POST, OPTIONS`.

## Security boundary

The relay is not a wallet and cannot derive or access wallet secrets. The normal node build remains keyless with respect to the temporary legacy raw-private-key wallet RPC implementation. `legacy-wallet-rpc` remains explicit, non-default development/rehearsal compatibility and is not enabled by this relay.

This contract does not change transaction nonce semantics, replay/RBF policy, submission identity, or consensus signing-domain rules. Those remain tracked separately by issue #821.

This boundary does not authorize release publication, GO, Day 0, the 30-day clock, public-testnet launch, or any production/mainnet custody claim.
