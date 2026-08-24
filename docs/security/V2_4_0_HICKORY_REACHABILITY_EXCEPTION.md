# v2.4.0 Hickory exception compatibility note

The former Hickory-only Task31 exception has been superseded by
`V2_4_0_LOCK_ONLY_RUSTSEC_EXCEPTIONS.md` after the current RustSec advisory set
expanded beyond Hickory.

Hickory remains covered narrowly as `RUSTSEC-2026-0119` on
`hickory-proto 0.24.4`, and the permanent exact-candidate gate still proves that
Hickory/DNS/mDNS packages are not compiled. The generic record is now the sole
authoritative exception policy and preserves the `2026-08-31 UTC` review
deadline and all no-public-GO guardrails.
