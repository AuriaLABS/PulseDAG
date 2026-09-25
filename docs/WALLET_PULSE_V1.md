# Wallet PulseClock view v1

Status: **FAIL-CLOSED MATCHER IN TREE** (not activated)

Date: 2026-09-25 UTC
Parent issues: #1178, #794
Source thesis: `ROADMAP_V3_0_0.md` Task P7
Depends on: `PULSECLOCK_V1.md`

Planning only. Does not ship a custody wallet or change `pulsedag-wallet` on the v2.4.0 candidate.

The in-tree matcher (`crates/pulsedag-core/src/wallet_pulse_v1.rs`) allows a future non-custodial surface to display vault/stream delays from observational PulseClock only. Host unix time is rejected. Missing PulseClock fails closed. Default `WalletPulseAdmissionV1::INACTIVE` keeps this off the current wallet.

`pulsedag-wallet` exposes `wallet_covenant_delay_pulse`. Session unlock may still use wall clock; that path must not feed covenant delay UI.

## Authorization

Planning only.
