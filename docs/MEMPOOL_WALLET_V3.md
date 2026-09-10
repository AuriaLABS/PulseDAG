# Wallet-facing mempool v3 contract

Status: launch-readiness contract for #1036. This document is descriptive of the current production RPC/wallet behavior and does not change admission policy, fee numbers, resource limits, expiry, replacement authority, consensus, or launch authority.

## Fee estimate surface

The local `pulsedag-wallet` application exposes a public-only command:

`fee-estimate --manifest <path> --relay <origin>`

The command reads only the validated watch-only manifest to obtain the expected `network_profile` and `chain_id`; it does not unlock a keystore or accept secret command-line input. Before fetching an estimate it requires `/release` to match that network identity and to advertise both the existing `mempool` capability and the canonical `/api/v1/mempool/fee-estimate` endpoint.

The wallet consumes the versioned `MempoolFeeEstimateV3` wire contract without converting fee-rate values through JSON floating-point numbers. `min_relay_fee_rate_per_kb`, economy/standard/priority recommendations and optional observed min/max values remain decimal strings and are validated as unsigned `u128` decimal integers. The wallet also validates the estimator version, canonical lowercase SHA-256 policy fingerprint and the bounded `pressure_bps` range before returning the result.

The estimate is observational. It is not an admission guarantee and does not override package/conflict, capacity, expiry, replacement or transaction-protocol rules.

## Broadcast rejection surface

`tx-broadcast --signed <path> --relay <origin>` preserves the node's machine-readable rejection tuple when `POST /api/v1/tx/submit` returns `ok=false`:

- `rejection_code` — including stable `MEMPOOL_V3_*` policy codes;
- `rejection_message` — the public rejection detail;
- `rejection_classification` — the RPC transaction-rejection classification when present.

The wallet does not infer a replacement state from fee, fee rate, nonce or txid differences. Current frozen transaction-protocol behavior remains no-RBF/fail-closed as documented in `MEMPOOL_REPLACEMENT_V3.md` and `TRANSACTION_PROTOCOL_V2.md`.

## Guardrails

This wallet-facing slice does not change `MempoolPolicyV3`, its version/fingerprint/defaults, the compatibility values `0 / u64::MAX / 4096 / false`, the fee estimator algorithm, live admission, eviction, expiry, package/conflict classification, RBF/replacement behavior, storage schema, P2P, consensus validation, signing bytes or mining behavior. It does not freeze final minimum/maximum fee safety numbers or resource/expiry values, and it does not authorize #781 or #794 GO.
