//! Compare separately frozen Python-stdlib GD-08 anchor/role/order SHA256
//! digests against the dormant native-v3 Rust authenticated PoW/GHOSTDAG trial.
//! This is **NOT** a frozen consensus policy or monetary state/reward oracle.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    native_gd08_order_kernel_v3::derive_dormant_native_gd08_order_from_ghostdag_trial,
    pow_v3::verify_v3_header_pow,
    types::BlockId,
};
use serde_json::Value;

const INPUT: &str = include_str!("../../../docs/fixtures/native_v3_ghostdag_fork_crosslang.json");
const GOLDEN: &str = include_str!("../../../docs/fixtures/native_v3_gd08_order_crosslang.json");

fn field<'a>(json: &'a Value, key: &str) -> &'a str {
    json[key].as_str().expect("frozen independent string field")
}

#[test]
fn native_real_pow_gd08_k0_k1_order_and_digest_match_independent_python() {
    let original: Value = serde_json::from_str(INPUT).expect("frozen real PoW vectors");
    let golden: Value = serde_json::from_str(GOLDEN).expect("frozen Python GD-08 SHA256 vectors");
    assert_eq!(
        field(&golden, "schema"),
        "pulsedag.dormant-v3-gd08-crosslang-order-digest-v1"
    );
    assert_eq!(field(&golden, "status"), "DORMANT_TEST_ONLY_UNAPPROVED");
    assert_eq!(field(&original, "chain_id"), field(&golden, "chain_id"));
    let chain = field(&original, "chain_id");
    let ns = original["genesis_ns"]
        .as_u64()
        .expect("full u64 nanoseconds");
    assert_eq!(Some(ns), golden["genesis_ns"].as_u64());
    let genesis = dormant_native_genesis_envelope_v3_ns(chain, ns)
        .expect("authenticate frozen-genesis identity");
    assert_eq!(genesis.hash, field(&original["blocks"]["G"], "id"));

    let mut digests = Vec::new();
    for (name, tip_name, k) in [("K0", "X0", 0_usize), ("K1", "X1", 1_usize)] {
        let mut headers: BTreeMap<BlockId, BlockHeaderV3> = BTreeMap::new();
        for block_name in ["G", "A", "B", tip_name] {
            let block = &original["blocks"][block_name];
            let expected_hash = field(block, "id");
            let header: BlockHeaderV3 =
                serde_json::from_value(block["header"].clone()).expect("real native v3 header");
            assert_eq!(
                compute_block_hash_v3(&header, chain).unwrap(),
                expected_hash,
                "validate every authenticated header id"
            );
            if block_name == "G" {
                assert_eq!(genesis.header, header, "genesis is independently frozen");
            } else {
                verify_v3_header_pow(&header, chain)
                    .expect("reject if any non-genesis header has invalid actual PoW");
            }
            assert!(
                headers.insert(expected_hash.to_string(), header).is_none(),
                "no duplicate headers in independently generated corpus"
            );
        }
        let tip = field(&original["blocks"][tip_name], "id").to_string();
        assert_eq!(field(&golden["cases"][name], "tip"), tip);
        let derived = derive_dormant_native_gd08_order_from_ghostdag_trial(
            chain,
            ns,
            &genesis.hash,
            &tip,
            &headers,
            k,
        )
        .expect("complete native PoW-authenticated dormant GHOSTDAG-derived order");
        let expected_order: Vec<String> = golden["cases"][name]["ordered"]
            .as_array()
            .expect("frozen Python order array")
            .iter()
            .map(|v| v.as_str().expect("exact block id").to_string())
            .collect();
        assert_eq!(
            derived.ordered, expected_order,
            "native Rust must match independent Python topo/anchor order"
        );
        assert_eq!(
            derived.digest,
            field(&golden["cases"][name], "digest"),
            "full SHA256 domain/chain/anchor/role bytes must match Python"
        );
        assert_eq!(
            derived.blue_count,
            golden["cases"][name]["blue_count"].as_u64().unwrap() as usize
        );
        assert_eq!(
            derived.red_count,
            golden["cases"][name]["red_count"].as_u64().unwrap() as usize
        );
        digests.push(derived.digest.clone());

        // A different insertion order and unrelated OTHER candidate tip cannot
        // change the full authenticated ancestry, selected spine or GD-08 digest.
        let mut shuffled: BTreeMap<BlockId, BlockHeaderV3> =
            headers.clone().into_iter().rev().collect();
        let unused_tip = if name == "K0" { "X1" } else { "X0" };
        let unrelated = &original["blocks"][unused_tip];
        let unrelated_header: BlockHeaderV3 =
            serde_json::from_value(unrelated["header"].clone()).unwrap();
        shuffled.insert(field(unrelated, "id").to_string(), unrelated_header);
        let same = derive_dormant_native_gd08_order_from_ghostdag_trial(
            chain,
            ns,
            &genesis.hash,
            &tip,
            &shuffled,
            k,
        )
        .expect("only tip's authenticated ancestor closure contributes");
        assert_eq!(derived, same);

        // Keep the asserted map key (claimed block ID) while mutating the
        // REAL PoW-authenticated side header. Neither nonce nor merkle-root
        // forgery may enter the dormant GD-08 ordering kernel as authority.
        // This is a negative, test-only admission check, not monetary replay.
        let side_id = field(&original, "other_parent").to_string();
        let mut forged_nonce = headers.clone();
        forged_nonce
            .get_mut(&side_id)
            .expect("frozen merge-side header")
            .nonce ^= 1;
        assert!(
            derive_dormant_native_gd08_order_from_ghostdag_trial(
                chain,
                ns,
                &genesis.hash,
                &tip,
                &forged_nonce,
                k,
            )
            .is_err(),
            "tampering with a side PoW nonce under its former hash must fail closed"
        );
        let mut forged_merkle_root = headers.clone();
        forged_merkle_root
            .get_mut(&side_id)
            .expect("frozen merge-side header")
            .merkle_root = "ff".repeat(32);
        assert!(
            derive_dormant_native_gd08_order_from_ghostdag_trial(
                chain,
                ns,
                &genesis.hash,
                &tip,
                &forged_merkle_root,
                k,
            )
            .is_err(),
            "tampering with side committed transactions under its former hash must fail closed"
        );

        // Maliciously removing an actual side ancestor cannot lead to a digest.
        let mut omitted = headers.clone();
        omitted.remove(field(&original, "other_parent"));
        assert!(
            derive_dormant_native_gd08_order_from_ghostdag_trial(
                chain,
                ns,
                &genesis.hash,
                &tip,
                &omitted,
                k,
            )
            .is_err(),
            "missing side ancestry fails closed"
        );
        assert!(
            derive_dormant_native_gd08_order_from_ghostdag_trial(
                "foreign-chain-identity",
                ns,
                &genesis.hash,
                &tip,
                &headers,
                k,
            )
            .is_err(),
            "foreign genesis/chain identity fails closed"
        );
    }
    assert_ne!(
        digests[0], digests[1],
        "K0 red vs K1 blue trial roles must produce different cryptographic digest"
    );
}
