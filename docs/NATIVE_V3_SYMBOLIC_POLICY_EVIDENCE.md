# Native-v3 GHOSTDAG: symbolic policy evidence only

**Status: UNAPPROVED / NOT CONSENSUS.** Tracked under [#1309](https://github.com/AuriaLABS/PulseDAG/issues/1309) and developed further in [proposal #1341](https://github.com/AuriaLABS/PulseDAG/pull/1341). This independent incremental PR imports only two **byte-identical, non-activating standard-library Python research scripts** from #1341; it does **not** import the broader Rust candidate or the experimental monetary replay modifications. Merging this evidence alone does NOT approve any network rule.

## Frozen research behavior, not network authority

- `scripts/native_v3_ghostdag_policy_candidate.py` compares *symbolic* DAG parent candidates using **manually supplied** integer work, topologically ordered merge-set candidates, inclusive score/work and two illustrative K-cluster conditions. The currently proposed trial work formula and the numeric K values are not adopted rules. Symbolic node labels are **not** SHA256 hashes; this reference has no native-v3 headers, network-bound genesis, KHeavyHash, actual compact targets, timestamps, reward or UTXO replay.
- `scripts/native_v3_k_delay_sensitivity.py` computes non-empirical, assumed overlap tails at 2 blocks/s (500ms interval), using a homogeneous Poisson distribution and an **illustrative** Gamma-Poisson r=2 burst model for assumed delay windows. Those values are **not** measured propagation, anticone statistics, network safety guarantees, attack costs or a justified production K.
- `scripts/test_native_v3_ghostdag_policy_stress.py` exercises a **42-vertex**, eight-parent symbolic merge with eight independent chains, each depth five. It distinguishes K=0/6/31/**32**/35 outcomes, checks the cluster bound independently for every accepted blue node, tests reversed map/parent order and rejects invalid ancestry and non-integer K. The eight-parent layout and K=32 are **experimental inputs**, not frozen caps.

Exact-head CI: `.github/workflows/v3-native-ghostdag-symbolic-preflight.yml` runs both scripts with Python assertions on and off (`-O`), independently checks the deep-fork boundary cases and retains the assumed-latency JSON with `peer_delay_empirical_evidence=false`.

Run locally:

```bash
python3 scripts/native_v3_ghostdag_policy_candidate.py
python3 -O scripts/native_v3_ghostdag_policy_candidate.py
python3 -m unittest discover -s scripts -p 'test_native_v3_ghostdag_policy_stress.py' -v
python3 scripts/native_v3_k_delay_sensitivity.py --check
python3 scripts/native_v3_k_delay_sensitivity.py --json
```

## Blocking next steps

1. Gather independently pinned **real native-v3 peer first-seen propagation measurements**, worst-case and high-percentile clocks/delays, target production topology, adversarial K-bound and CPU/memory/DoS limits. Do not use synthetic Poisson tails as security evidence.
2. Independently prove proposed selected parent, blue/red, exact target-weighted work and overflow under **real native-v3 SHA256/KHeavyHash headers**, cross-language goldens and changing target/retarget, on the **current integrated main**. Keep the larger #1341 candidate dormant until its current-main exact-head CI, security and replay impact are reviewed.
3. Freeze and version V3-GD-01..07 on #1309 **only after** the named protocol-owner decision with evidence; then authorize separate native monetary storage, ordering, replay and P2P/mined admission work. The operational v3.0 monetary-mode startup remains fail-closed.
4. Contract-core completion is tracked separately in #1333. Smart contracts **must remain INACTIVE in all v3.0 phases**; #781 mainnet/public testnet remains **NO-GO**, including private monetary-mining rehearsal until all specific gates are proven.

No additional mining, transaction acceptance, consensus, retarget, startup, contract or stored-state behavior is changed by these research scripts.
