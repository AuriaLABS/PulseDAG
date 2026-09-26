# Wallet PulseClock view v1

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-25 UTC
Parent issues: #1178, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P7
Depends on: `PULSECLOCK_V1.md`

Planning only. Does not ship a custody wallet or change `pulsedag-wallet` on the v2.4.0 candidate.

The in-tree matcher (`crates/pulsedag-core/src/wallet_pulse_v1.rs`) allows a future non-custodial surface to display vault/stream delays from observational PulseClock only. Host unix time is rejected. Missing PulseClock fails closed. Default `WalletPulseAdmissionV1::INACTIVE` keeps this off the current wallet.

`pulsedag-wallet` exposes `wallet_covenant_delay_pulse`. Session unlock may still use wall clock; that path must not feed covenant delay UI.

Read-only CLI `pulse --manifest <path> --relay <origin>` fetches observational `GET /api/v1/pulse` after `/release` advertises `explorer_api` and that endpoint. It does not unlock a keystore. A foreign `chain_id` or a domain other than `PulseDAG:pulse:v1` fails closed. This does not activate vault delay rendering.

## Authorization

Planning only.
