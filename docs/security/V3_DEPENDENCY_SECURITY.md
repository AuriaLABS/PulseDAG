# 2026-11-13 Quinn RustSec post-merge drift — PATCH CANDIDATE, NOT GO

**New evidence supersedes all older Quinn lock-only advisory allowances below.**
After merging security #1458, the exact `main@48c5832262b701fb4570676526011f439c18611e`
RustSec run [43440084861](https://github.com/AuriaLABS/PulseDAG/actions/runs/43440084861)
**failed closed** on eight raw vulnerability advisories: the five previously
enumerated `RUSTSEC-2026-0338..0342` on `quinn-proto@0.11.16`, plus new
`RUSTSEC-2026-0523` and `RUSTSEC-2026-0524` on the same crate, and
`RUSTSEC-2026-0538` on `quinn-udp@0.5.14`. Two informational unmaintained
warnings (`paste@1.0.15`, `bincode@1.3.3`), zero yanked. The prior
raw inventory proof rejected the unexpected advisories as intended.

**Proposed actual source remediation (NOT a RustSec exception):** update the
public **Cargo-generated** lock to `quinn-proto@0.11.19` and
`quinn-udp@0.5.16`. The exact Cargo resolver removes orphaned
`windows-sys@0.59.0` and rewires unchanged `nu-ansi-term@0.50.3`
to existing `windows-sys@0.61.2`, with the same package source and
checksum. Those exact changes must be source-reviewed and their Windows
compilation tested; unknown transitive drift must fail.

The revised CI insists on **ZERO raw RustSec vulnerabilities**, no persistent
`.cargo/audit.toml` ignores and no CLI `--ignore` bypass. It also checks
that every optional QUIC/Hickory transport remains **uncompiled** from all
four launch roots (P2P, node, miner, wallet), even though patched packages
remain present in `Cargo.lock`. A real new advisory, source mismatch,
optional feature activation, absent Windows validation, or nonmatching
generated lock must FAIL. Historical 5-Quinn-advisory per-run exceptions
below are retired only after the **exact patched candidate** satisfies these
checks, not retroactively.

**Public testnet NO-GO / mainnet NO-GO / monetary mining OFF /
contracts INACTIVE.** Complete exact-head CI, review and post-merge CI
before considering this security debt closed; this is not release approval.

---

# Current v3.0 dependency migration candidate — 2026-11-13 (NO-GO)

This section **supersedes historical Kaspa/libp2p 0.56 and old Hickory
lockfile disposition statements further below**. Archived sections remain for
the audit trail only. The following candidate is not mainnet/testnet approved.

- The public workspace now requests exactly **libp2p =0.57.1** with
  `default-features=false` and precisely `tokio,gossipsub,identify,kad,macros,
  tcp,noise,yamux,ping`. Cargo-resolved **`hickory-proto@0.26.3`** and
  **`hickory-resolver@0.26.3`** replace vulnerable 0.25.2, whose advisory
  IDs `RUSTSEC-2026-0118/0119/0487/0489/0490/0491` may no longer occur in
  the public root. The DNS/mDNS features remain unselected and their crate
  packages must not appear in exact release-root compiled artifacts.
- The public root additionally pins **`rustix@1.1.6`**, patched for
  `RUSTSEC-2026-0486`. The security gate requires both the exact patched
  lock version and positive node and wallet compiler-artifact evidence.
- The former production-workspace **dev-only** `kaspa-hashes/pow` dependencies,
  transitive `workflow-core`, `async-std`, `pin-utils`, `faster-hex` and
  `atty` must be **absent from the main `Cargo.lock`**. They are not claimed
  globally removed from source or developer tools: `tools/pow-upstream-reference`
  has its **own separately generated, committed lockfile**, pinned to upstream
  revision `cfafeb4c093fa37a303f1b9f19c58f986b870ce3`, and independently
  runs all original 96×5=480 Kaspa/native KHeavyHash nonce comparisons plus
  CPU miner throughput parity on the same release runner.
- The public lock still contains **`quinn-proto@0.11.16`** via unused
  reqwest/libp2p optional paths. Its five known
  `RUSTSEC-2026-0338..0342` advisories remain exact, visible raw
  vulnerabilities, and are permitted only as individually enumerated
  **per-run** exceptions AFTER exact clean compiler-artifact absence is proved
  in all four release roots. No persistent `.cargo/audit.toml` ignores.
- **Any novel RustSec advisory, new package version, missing evidence,
  unexpected compiled DNS/QUIC packages, stale Kaspa root dependencies or
  altered upstream oracle must fail closed.** A successful dependency check is
  not a proof of overall security, a freeze of genesis or GD-08 monetary
  policy, or authorization to open public testnet.

The first migration feasibility job
[43222868271](https://github.com/AuriaLABS/PulseDAG/actions/runs/43222868271)
passed P2P/node/wallet/miner builds and 480 upstream oracle comparisons.
The branch-specific separate-lock generation job
[43307598949](https://github.com/AuriaLABS/PulseDAG/actions/runs/43307598949)
successfully generated, compiled, tested and committed the two
resolver-generated lockfiles to a **development branch**, not main.
The release security gate and full exact-head CI still require independent
review and completion before integration. All launch flags remain NO-GO/OFF,
smart contracts remain INACTIVE.

---

# PulseDAG v3 dependency-security boundary

Status: **active development gate; not final launch security approval**.

Issue authority: #1127 (historical #803). Launch authority: #781. Integrated program: #794. Remaining crate-graph work: #1139.

## Historical v2.4 exception retirement

The v2.4 lock-only vulnerability exception expired on **2026-08-31 UTC** and is not renewed for the active v3 development line. The active `.cargo/audit.toml` contains no vulnerability advisory ignores.

The libp2p 0.56 migration removes the historical lock-only vulnerable versions that were retained by the v2.4 libp2p 0.54 graph:

- `ring 0.16.20`;
- `rustls-webpki 0.101.7`;
- `hickory-proto 0.24.4`;
- `h2 0.3.27`.

## #1127 / historical #803 lru remediation

`lru 0.12.5` was reachable through the old libp2p stack (`libp2p-identify 0.45.0` and `libp2p-swarm 0.45.1`). The supported parent-stack migration in PR #1018 moves PulseDAG P2P to `libp2p 0.56.x`, where the selected identify/swarm line no longer resolves `lru 0.12.5`.

The remediation rule is fail-closed:

- no direct or transitive `lru` leaf override;
- `lru 0.12.5` must be absent from the resolved lock graph;
- historical v2.4 lock-only vulnerable versions above must be absent;
- PulseDAG's selected libp2p feature set remains explicit with default features disabled;
- clean compiler-artifact reachability is captured for `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner` and `pulsedag-wallet`;
- optional `dns`, `mdns`, `quic` and `upnp` libp2p packages must not be compiler-reachable from the selected PulseDAG feature set.

## #1127 / historical #803 linkme remediation

`linkme 0.2.10` was reachable through the legacy chain `kaspa-core 0.15.0 -> intertrait 0.2.2 -> linkme 0.2.10`. PulseDAG does not patch the `linkme` leaf or carry a private `intertrait` fork for the active remediation.

The supported parent-stack migration moves the direct PoW dependencies to the official Rusty Kaspa `v2.0.1` source, pinned to exact upstream commit:

`cfafeb4c093fa37a303f1b9f19c58f986b870ce3`

The active direct dependencies are `kaspa-hashes 2.0.1` and `kaspa-pow 2.0.1` from `https://github.com/kaspanet/rusty-kaspa` at that exact revision. This upstream line removes the legacy `intertrait 0.2.2` dependency path, so `linkme 0.2.10` and `linkme-impl 0.2.10` no longer resolve.

Because Rusty Kaspa 2.0.1 declares Rust 1.91.0 / edition 2024, the active v3 lint and dependency-security gates move to Rust 1.91.0. Historical frozen v2.4 workflows remain historical and are not reinterpreted by this migration.

The linkme remediation rule is fail-closed:

- `kaspa-hashes` and `kaspa-pow` must remain pinned to the reviewed official upstream revision above;
- the resolved direct Kaspa packages must be version `2.0.1` from that upstream revision;
- `intertrait 0.2.2` must be absent;
- `linkme 0.2.10` must be absent;
- existing `pulsedag-core` consensus/PoW tests must pass without changing PoW vectors or adapter logic;
- `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner` and `pulsedag-wallet` must compile against the same exact lock graph.

## Hickory 0.25.2 lock-only disposition

The Kaspa 2.0.1 lock graph currently contains `hickory-proto 0.25.2`, covered by `RUSTSEC-2026-0118` and `RUSTSEC-2026-0119`. These advisories are not hidden: the raw `cargo audit` report must contain exactly those vulnerability records and CI preserves the raw failing audit evidence.

The temporary development disposition is valid only while clean compiler-artifact evidence proves `hickory-proto 0.25.2` is absent from **all** launch roots: `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner` and `pulsedag-wallet`. If the package becomes compiler-reachable from any root, the gate fails closed. The CLI audit uses explicit per-run ignores only after that reachability proof; `.cargo/audit.toml` remains free of vulnerability ignores.

This is not a final v3 launch disposition. The exact candidate security review in #1127 must either remove this lock-only residue through a supported parent migration or explicitly renew the reviewed unreachable disposition for the final candidate.

## Newly published Quinn 0.11.16 advisory inventory — #1298

The exact `Cargo.lock` currently retains `quinn-proto 0.11.16`, now flagged
by **five vulnerabilities**: `RUSTSEC-2026-0338`, `0339`, `0340`,
`0341`, and `0342`. The October 14, 2026 `main` RustSec job failed
**correctly** on the newly detected raw advisory set; these findings cannot
be waived just to turn a build green.

The resolved lock graph reaches Quinn through **two branches**, not just
optional libp2p QUIC:

- `libp2p 0.56.0 -> libp2p-quic 0.13.1 -> quinn-proto 0.11.16`;
- `reqwest 0.12.28 -> quinn 0.11.9 -> quinn-proto 0.11.16`.

Therefore a libp2p feature-set check **alone is insufficient**. The
development-only disposition must verify clean compiler-artifact inventories
for *every* launch root — `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner`,
`pulsedag-wallet` — proving `quinn-proto@0.11.16` and **all** `quinn`,
`quinn-proto`, `quinn-udp`, `libp2p-quic` source packages absent.
Any compiled package from this stack, a version change, an extra advisory or
missing compiler evidence **fails closed**.

The raw cargo-audit artifact still shows all seven vulnerabilities (two
Hickory plus five Quinn). Only *after* exact-candidate compiler proof,
the workflow's **per-command, version-specific** advisory IDs may be passed
to cargo-audit for a temporary *development checkpoint*.
`.cargo/audit.toml` remains `ignore = []` with no persistent waiver.

This is **not** a final v3 public security or launch disposition. A supported
patched upstream Quinn/libp2p/reqwest graph is preferred, or the actual exact
release candidate requires independently reviewed lock-only evidence and
explicit renewed disposition. Tracks #1298, #1127, and #781. Public GO
remains blocked, independently of any scanner/checkpoint PASS.

## Remaining launch blocker: `atty` via `hexplay`

This remediation does **not** close #1127 or #1139. The known reachable warning remaining from the inherited Kaspa / workflow parent graph is:

- `atty 0.2.14` — `RUSTSEC-2024-0375` (unmaintained), `RUSTSEC-2021-0145` (Windows unsoundness).

### Reachability path (do not leaf-patch)

`atty 0.2.14` is **not** a direct PulseDAG dependency. On the current lock it is pulled through the supported parent path:

`kaspa-hashes` / `kaspa-pow` (Rusty Kaspa 2.0.1) → workflow / debug formatting stack → `hexplay 0.3.0` → `atty 0.2.14`

`hexplay 0.3.0` is a hex-dump pretty-printer. PulseDAG must not:

- add a `[patch.crates-io]` override for `atty` or `hexplay`;
- fork Kaspa solely to drop a debug pretty-printer;
- hide `RUSTSEC-2024-0375` or `RUSTSEC-2021-0145` in `.cargo/audit.toml`.

Removal of this path requires a **supported parent migration** (newer official Rusty Kaspa / workflow line that no longer depends on `hexplay`/`atty`, or an upstream drop of that debug dep). Until that exists, the warning remains visible in raw `cargo audit` and is a public-testnet GO blocker.

### Windows allocator invariant (`RUSTSEC-2021-0145`)

`RUSTSEC-2021-0145` is Windows-specific unsoundness: `atty` may dereference a potentially unaligned pointer. The advisory states the pointer is aligned in practice **unless a custom global allocator is used**. The Windows `System` allocator (`HeapAlloc`) provides sufficient alignment.

PulseDAG therefore keeps this fail-closed invariant while `atty` remains in the lock graph:

- no `#[global_allocator]` in first-party `*.rs` sources;
- no custom allocator crate wired as the process global allocator on Windows launch roots (`pulsedagd`, `pulsedag-miner`, `pulsedag-wallet`, `pulsedag-p2p`);
- the historical validator `scripts/validate_v2_4_0_rustsec_warning_disposition.py` still asserts the absence of `global_allocator` in first-party Rust sources.

This invariant **mitigates the Windows unaligned-read precondition**. It does **not**:

- make `atty` maintained;
- authorize public-testnet GO;
- waive `RUSTSEC-2024-0375` or `RUSTSEC-2021-0145`;
- replace the required parent-stack removal of `hexplay`/`atty`.

Windows exact-candidate security revalidation remains pending for any public network decision.

## Visible unmaintained parent residue: `derivative 2.2.0`

The Kaspa 2.0.1 graph also makes `derivative 2.2.0` (`RUSTSEC-2024-0388`, unmaintained derive-macro helper) compiler-reachable. Classification:

- informational / unmaintained, not a vulnerability ID in the raw audit vulnerability set;
- not a first-party PulseDAG crate;
- not authorized for leaf override or `.cargo/audit.toml` ignore;
- owner for final matrix: same parent-stack review as #1127/#1139.

This record keeps the warning **visible**. It is not a public-testnet GO grant and does not replace the `atty` blocker. Removal tracks a supported Kaspa/workflow parent upgrade, not a PulseDAG fork of `derivative`.

Other informational warnings likewise remain visible; no warning is hidden merely to obtain a green audit.

The raw `cargo-audit` informational warning inventory additionally includes
`unsafe-libyaml 0.2.11` (`RUSTSEC-2024-0449`, unmaintained). This
warning is retained verbatim in the dependency-audit gate's exact expected
inventory, not suppressed or reclassified as a vulnerability. Its final
replacement/retirement must be reviewed together with the parent graph
before any public network launch.

Runtime `bincode 1.3.3` remains a separate tracked item with an explicit plan in `docs/security/V3_BINCODE_MIGRATION_PLAN.md`.

## Visible informational unsoundness: `faster-hex 0.9.0`

RustSec `RUSTSEC-2026-0306` covers an x86/x86_64 out-of-bounds read in
`faster_hex::hex_decode_unchecked`. The active Kaspa parent stack still
resolves and compiles `faster-hex 0.9.0`, so PulseDAG does not classify this
package as lock-only.

The v3 dependency gate keeps the advisory visible and fails closed if the
affected API symbol is referenced by PulseDAG, Kaspa, or workflow source
packages in the resolved graph. Current reviewed call sites use the checked
`hex_decode` / `hex_encode` APIs instead. This is a development
disposition only: it does not mark final v3 launch security ready, and a
supported parent-stack move to `faster-hex >=0.10.1` remains preferred when
Kaspa permits that dependency line.

## Final launch boundary

A PASS of the active dependency workflow means the current development candidate satisfies this dependency-remediation checkpoint. It does not mean `security_ready=true`, does not authorize mainnet/testnet launch and does not replace the final exact-candidate security review required by #1127/#781.


## 2026-11-04 consolidated patch candidate — #1380 (NOT final GO)

The stacked candidate in PRs #1382, #1383, #1384 updates the *actual* generated
`Cargo.lock` with reviewed, supported crate index entries rather than weakening
`[advisories] ignore=[]` or changing consensus, network, monetary or contract code.
The final consolidated PR must pass the **same exact-head** RustSec, full workspace,
Linux/Windows, miner/node/wallet and package validation before any merge.

| Advisory / inventory | Previous locked crate | Candidate locked crate | Development-only interpretation |
| --- | --- | --- | --- |
| RUSTSEC-2026-0381 | `webpki-roots 1.0.7` | `1.0.10` | Patched TLS trust roots; runtime TLS behavior still requires full integration CI |
| RUSTSEC-2026-0382 | `rustls-webpki 0.103.15` | `0.103.16` | Patched certificate verification; not a substitute for packaged target evidence |
| RUSTSEC-2026-0418 | `libz-sys 1.1.28` | `1.1.31` | Patched build-time risk; inspect prior CI artifact/caches for inadvertent environment disclosure |
| RUSTSEC-2026-0410 | `zstd-sys 2.0.16+zstd.1.5.7` | `2.1.2+zstd.1.5.7` | Patched wasm32 advisory; building on Linux alone did not prove exposure on wasm32 |
| INFO RUSTSEC-2026-0386 | `tokio 1.52.1` | `1.53.3` | Patched unsound `AsyncFd` constructor warning; audit actual API call sites separately |
| Yanked | `async-executor 1.14.0` | `1.14.1` | Cargo-resolved compatible replacement |
| Yanked | `chacha20 0.10.0` | `0.10.2` | Cargo-resolved compatible `rand 0.10.1` dependency (separate `chacha20 0.9.1` remains) |

The Zstd/Tokio generation additionally removed an unused `bindgen 0.72.1`
lock entry and adjusted five resolved `windows-sys` references. Those edges must
be proven through the Windows compilation and packaging matrix, not assumed safe.

**Fail-closed raw inventory on this candidate:** the seven *older* Hickory
(`RUSTSEC-2026-0118/0119`) and Quinn (`RUSTSEC-2026-0338..0342`) advisories
must remain visible and require fresh per-root compiler-artifact unreachability
proof on the *new* exact binary SHA. A raw audit with more than seven advisories,
any new informational unsoundness warning, any yanked package or any compiled
Hickory/QUIC dependency fails. After Tokio has been patched, the expected
informational warning set remains exactly the ten legacy warnings; expected
yanked count becomes zero, with **no persistent vulnerability waivers**. This
expected inventory is NOT itself the proof: the GitHub CI must obtain it from
an actual fresh RustSec database and pass full reachability and configured audit.

Historic `atty`, `faster-hex` and unmaintained dependencies remain launch
security review items even if the development-only RustSec workflow becomes
green. A successful audit on Ubuntu does not prove Windows reachability,
security for release packaging or the authenticity of a checkpoint. Network
launch #781 remains NO-GO; native-v3 monetary mining OFF and smart contracts
INACTIVE/fail-closed. GHOSTDAG rules #1309 and pruned monetary replay #1367
remain independent blockers.


## 2026-11-05 RustSec advisory database drift: `pin-utils` (#1387)

The latest *integrated-main* [RustSec run 42015756913](https://github.com/AuriaLABS/PulseDAG/actions/runs/42015756913) correctly **FAILED CLOSED** at `main@54fbc8507edb5952315e86f82203ab115c975f85`. The raw audit came from advisory database commit `26fd2d43f7280a1fb925a3743ec463017afe7119` (updated `2026-11-05T09:50:49+01:00`; artifact 13410052625).

The exact result is **7 known raw Hickory/Quinn vulnerability advisories, 11 informational warnings, zero yanked crates**. The *only* newly observed advisory compared with the 2026-11-04 inventory above is `RUSTSEC-2026-0441`, category **unmaintained**, package `pin-utils 0.1.0`. The upstream repository was archived; the advisory itself records **no fixed versions**. This is not a newly proven memory-safety bug, and changing `pin-utils` to another patch version does **not** cure its maintenance status. Nor does a green test prove absence of every dependency risk.

`Cargo.lock` and the clean Rust compiler artifact inventories establish the exact existing path:

```text
pulsedag-core
  -> kaspa-hashes 2.0.1 (rusty-kaspa Git revision cfafeb4c093fa37a303f1b9f19c58f986b870ce3)
    -> workflow-wasm 0.18.0
      -> workflow-core 0.18.0
        -> async-std 1.13.2
          -> pin-utils 0.1.0
```

The package is **compiled in all four** Linux launch roots (P2P, daemon, miner, wallet). It is not a lock-only historical residue, so the narrow Hickory/Quinn unreachability procedure is **not** applicable. The exact-Cargo.lock and exact-four-root compiler-path assertions in this candidate intentionally reject future upstream/version/compilation changes until someone reviews and updates the record.

**Short-term development-CI disposition under review:** retain the new warning in the **exact raw informational inventory** (11 rather than 10) and print the compiled upstream path. `.cargo/audit.toml` stays `ignore=[]`; there is **no `--ignore RUSTSEC-2026-0441`**, no relaxation of the seven raw vulnerability identities, and zero yanked remains mandatory. Do not represent an updated informational inventory as a dependency upgrade or vulnerability fix.

**Remaining actual remediation:** pursue a separately reviewed compatible move off the archived upstream `pin-utils` and `async-std` via the pinned Kaspa/workflow stack. Any such migration is sensitive to native hash/PoW and must rerun independent KHeavyHash/SHA256 golden vectors, deterministic GHOSTDAG/retarget diagnostics, CPU/GPU independence, P2P/wallet interoperability, Windows+Linux compilation, and full RustSec revalidation. Existing `atty` and `faster-hex` unsoundness warnings remain separate launch-security items.

No changes to consensus policy, monetary mining admission, contracts or public launch are authorized by recognizing this warning. **#781 mainnet/testnet NO-GO; native v3 monetary mining OFF; contracts INACTIVE.**

## 2026-11-08 — explicit per-launch-root inherited advisory evidence (#1387)

The active development inventory gate now emits `ci-evidence/dependency-v3-security/reachable-unmaintained-launch-debt.json` as a **machine-readable compiler exposure record**. For EACH of `pulsedag-p2p`, `pulsedagd`, `pulsedag-miner`, and `pulsedag-wallet`, it demands BOTH `async-std@1.13.2:RUSTSEC-2025-0052` and `pin-utils@0.1.0:RUSTSEC-2026-0441` in the **clean Cargo compiler-artifact** package set. This deliberately cannot be mistaken for a lock-only Hickory/Quinn exception. Missing, unknown or quietly version-changed records fail the inventory gate and require a fresh reviewed upstream-dependency/security determination; they are not automatically interpreted as a vulnerability fix.

The summary's `remaining_launch_blockers` now includes this inherited unmaintained debt alongside `atty`, and `reachable_unmaintained_compiler_debt_by_launch_root` names each compiled root and advisory. The CLI success message explicitly says **inventory pass only, not release security approval**. Five standalone negative Python tests protect the report. The raw RustSec DB and advisory counts remain authoritative; `.cargo/audit.toml` is untouched, and **no advisory ignore has been added**.

This update **does NOT upgrade or fix pin-utils or async-std**, prove absence of a memory-safety bug, mitigate the `atty` or `faster-hex` advisory separately, or grant public-testnet/mainnet GO. A separately validated parent-stack migration away from the archived packages is still needed, with native v3 PoW/KHeavyHash/CPU-GPU/CI verification.
