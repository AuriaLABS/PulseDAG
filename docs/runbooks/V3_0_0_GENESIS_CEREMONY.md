# PulseDAG v3.0.0 genesis ceremony runbook

Status: **PRE-FREEZE / DO NOT START PUBLIC NETWORKS**

Authority: #781 is the final launch authority. This runbook creates the deterministic genesis evidence needed by `GENESIS_V3_0_0.md` and `V3_0_0_LAUNCH_MANIFEST.md`.

## Preconditions

Do not begin the production genesis ceremony until all are true:

1. one exact v3.0.0 candidate SHA/tree is frozen;
2. monetary policy is approved and its digest recorded;
3. reward index, subsidy schedule, coinbase maturity and fee/burn rules are implemented and tested;
4. mainnet and testnet chain IDs/network domains are approved but no network has been publicly launched;
5. genesis generator source and build process are reviewed;
6. production genesis allocation manifest is complete and contains no placeholders;
7. protocol/consensus/storage/contract activation identities are frozen;
8. #803 has no unresolved genesis/toolchain-relevant security blocker;
9. ceremony operators have clean environments and independent verification capability.

## Ceremony inputs

Prepare two immutable, separately named input manifests: one for mainnet and one for parallel testnet. Each contains exactly the fields required by `GENESIS_V3_0_0.md` and references the same exact v3 release candidate while using independent network identities.

Input manifests must be committed or otherwise content-addressed before generation. No manual edit is allowed between independent generation runs.

## Mainnet generation

1. Checkout the exact frozen candidate and verify `git rev-parse HEAD` and tree SHA.
2. Build the approved genesis generator reproducibly or verify its approved binary digest.
3. Verify the monetary-policy document/config and allocation-manifest digests.
4. Verify no allocation destination is a placeholder such as `genesis-treasury`.
5. Run the generator with only the frozen mainnet input manifest.
6. Export canonical genesis bytes, decoded representation, TXID(s), Merkle root, state root, block hash and all referenced config/policy digests.
7. Hash every output artifact with SHA-256.
8. Repeat from a clean independent environment/operator.
9. Compare every canonical byte and digest. Any mismatch is a hard stop.
10. Validate the generated genesis using the exact packaged v3 node in offline/isolated mode.
11. Verify block-1 transition rules, PoW/difficulty contract and monetary issuance vectors.

## Parallel-testnet generation

Repeat the same procedure from the independently frozen testnet input manifest. Do not derive testnet by editing the generated mainnet artifact.

After generation, assert:

- mainnet chain ID != testnet chain ID;
- mainnet network/signing domain != testnet domain;
- mainnet genesis hash != testnet genesis hash;
- chain-bound genesis TXID(s) differ where required;
- mainnet/testnet address/network encodings are distinguishable where applicable;
- each node rejects the other network's identity/config/genesis.

Any failed assertion is a hard stop.

## Economic verification

For each network, independently recompute:

- sum of genesis outputs;
- approved genesis issuance;
- allocation totals by destination/commitment;
- initial state root;
- first mineable-block subsidy;
- all subsidy transition vectors;
- terminal issuance/max-supply rule;
- fee and burn/distribution accounting;
- coinbase maturity spendability boundaries.

Mainnet genesis issuance must exactly match the approved mainnet monetary policy. No difference, even one atomic unit, is acceptable.

## Security review

Before accepting outputs:

- verify generator and release artifact hashes;
- verify no secrets exist in genesis/config/manifests;
- verify dependency/toolchain provenance;
- verify files are immutable/content-addressed;
- verify no operator-local environment value changed consensus bytes;
- verify generated artifacts contain no undocumented allocation or activation state.

## Freeze publication

Populate `docs/V3_0_0_LAUNCH_MANIFEST.md` with the exact outputs and change `Launch state` to `FROZEN` only when all launch-required fields and approvals are complete.

Attach/reference in #781 and #794:

- mainnet/testnet input manifest digests;
- canonical genesis bytes digests;
- genesis hashes, TXIDs, roots;
- monetary-policy digest;
- generator/release source and binary digests;
- independent reproduction evidence;
- separation-test results;
- reviewers/approvals.

## After freeze, before GO

Run the final production-like rehearsal using the exact frozen artifacts and configs without representing either network as publicly launched. Any candidate/genesis/policy/config change requires an explicit rebaseline and rerun of affected evidence.

## After GO

Only after #781 records `GO_V3_DUAL_LAUNCH`:

1. provision the frozen network-specific configs to launch hosts;
2. start mainnet and testnet bootnode meshes independently;
3. verify the live nodes report the exact frozen genesis/config identities;
4. start approved miners and verify reward/accounting behavior;
5. publish public endpoints and checksums;
6. record independent first accepted block identifiers and UTC timestamps in the launch manifest.

## Hard stops

Stop the ceremony or launch for any:

- output mismatch between independent genesis builds;
- `TBD` in a launch-required frozen field;
- placeholder/undocumented genesis allocation;
- supply mismatch;
- cross-network identity collision;
- unreproducible generator/build;
- genesis accepted only with undocumented runtime overrides;
- changed release candidate or consensus constants;
- unresolved security or custody blocker;
- inability to restore/reproduce the exact genesis artifacts.