//! Cross-language *frozen full-byte* native-v3 fork/merge vectors, DORMANT ONLY.
//! This checks Python-stdlib SHA256/KHeavyHash reference against native Rust
//! header/PoW/complete-DAG verifiers. It is NOT a consensus implementation,
//! production genesis freeze, approved GHOSTDAG K or monetary authorization.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{canonical_block_header_bytes_v3, compute_block_hash_v3, BlockHeaderV3},
    native_anticone_matrix_v3::derive_dormant_native_v3_anticone_matrix,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    native_merge_set_v3::derive_dormant_native_v3_all_parent_merge_sets,
    pow_v3::{evaluate_v3_header_pow, v3_pow_preimage_bytes, verify_v3_header_pow},
    types::BlockId,
};
use serde_json::Value;

const FROZEN: &str = include_str!("../../../docs/fixtures/native_v3_ghostdag_fork_crosslang.json");

fn text_field<'a>(v: &'a Value, name: &str) -> &'a str {
    v[name].as_str().expect("frozen string field")
}

#[test]
fn frozen_python_fork_headers_and_pow_match_native_rust_exactly() {
    let frozen: Value = serde_json::from_str(FROZEN).unwrap();
    assert_eq!(
        text_field(&frozen, "schema"),
        "pulsedag.dormant-v3-crosslang-fork-pow-work-v1"
    );
    assert_eq!(
        text_field(&frozen, "status"),
        "DORMANT_TEST_ONLY_UNAPPROVED"
    );
    let chain = text_field(&frozen, "chain_id");
    let ns = frozen["genesis_ns"].as_u64().unwrap();
    let actual_genesis = dormant_native_genesis_envelope_v3_ns(chain, ns).unwrap();

    let mut headers: BTreeMap<BlockId, BlockHeaderV3> = BTreeMap::new();
    for name in ["G", "A", "B", "X0", "X1"] {
        let vec = &frozen["blocks"][name];
        let header: BlockHeaderV3 = serde_json::from_value(vec["header"].clone()).unwrap();
        let block_id = text_field(vec, "id");
        assert_eq!(
            compute_block_hash_v3(&header, chain).unwrap(),
            block_id,
            "{name}: exact chain-bound SHA-256 header identity"
        );
        assert_eq!(
            hex::encode(canonical_block_header_bytes_v3(&header, chain).unwrap()),
            text_field(vec, "header_bytes_hex"),
            "{name}: Python-vs-Rust canonical header bytes"
        );
        assert_eq!(
            hex::encode(v3_pow_preimage_bytes(&header, chain).unwrap()),
            text_field(vec, "pre_pow_bytes_hex"),
            "{name}: nonce-free KHeavyHash preimage"
        );
        let evaluation = evaluate_v3_header_pow(&header, chain).unwrap();
        assert_eq!(
            evaluation.pow.hash_hex,
            text_field(vec, "pow_hash"),
            "{name}: independent Python vs native Rust KHeavyHash digest"
        );
        assert_eq!(
            evaluation.pow.accepted,
            vec["accepted"].as_bool().unwrap(),
            "{name}: full 256-bit difficulty comparison"
        );
        if name == "G" {
            assert_eq!(block_id, actual_genesis.hash);
            assert_eq!(header, actual_genesis.header);
            assert_eq!(text_field(vec, "work"), "0");
        } else {
            assert!(evaluation.pow.accepted, "{name}: true PoW required");
            verify_v3_header_pow(&header, chain).unwrap();
            assert_eq!(text_field(vec, "work"), "2");
        }
        assert!(
            headers.insert(block_id.to_string(), header).is_none(),
            "duplicate independently frozen header"
        );
    }

    let sibling_a = text_field(&frozen["blocks"]["A"], "id");
    let sibling_b = text_field(&frozen["blocks"]["B"], "id");
    let selected = text_field(&frozen, "chosen_parent");
    let other = text_field(&frozen, "other_parent");
    assert_eq!(selected, sibling_a.min(sibling_b));
    assert_eq!(other, sibling_a.max(sibling_b));
    let frontier = vec![selected.to_string(), other.to_string()];
    let genesis = actual_genesis.hash;
    let sets =
        derive_dormant_native_v3_all_parent_merge_sets(chain, ns, &genesis, &frontier, &headers)
            .expect("the complete frozen DAG must authenticate all side-branch PoW");
    assert_eq!(
        sets.by_comparison_parent.get(selected).unwrap(),
        &vec![other.to_string()]
    );
    let anti =
        derive_dormant_native_v3_anticone_matrix(chain, ns, &genesis, &frontier, &headers).unwrap();
    assert_eq!(anti.anticone_by_block[selected], vec![other.to_string()]);
    assert_eq!(anti.anticone_by_block[other], vec![selected.to_string()]);

    for (k, join_name, expected_name) in [(0_u64, "X0", "K0"), (1_u64, "X1", "K1")] {
        let join = &frozen["blocks"][join_name];
        // This is the independently executed dormant Rust classifier, not
        // a hardcoded Python expected-work assertion.
        let trial = derive_dormant_native_v3_ghostdag_trial(
            chain,
            ns,
            &genesis,
            &[text_field(join, "id").to_string()],
            &headers,
            k as usize,
        )
        .expect("fully authenticated frozen K trial");
        let metadata = &trial.blocks[text_field(join, "id")];
        assert_eq!(metadata.selected_parent.as_deref(), Some(selected));
        assert_eq!(metadata.blue_score, if k == 0 { 2 } else { 3 });
        assert_eq!(metadata.blue_work.decimal(), if k == 0 { "4" } else { "6" });
        assert_eq!(
            metadata.merge_blues,
            if k == 0 {
                vec![]
            } else {
                vec![other.to_string()]
            }
        );
        assert_eq!(
            metadata.merge_reds,
            if k == 0 {
                vec![other.to_string()]
            } else {
                vec![]
            }
        );

        let commit: BlockHeaderV3 = serde_json::from_value(join["header"].clone()).unwrap();
        assert_eq!(commit.parents, frontier);
        let expected = &frozen["expected"][expected_name];
        assert_eq!(text_field(expected, "selected_parent"), selected);
        let accept_other_blue =
            k >= 1 && anti.anticone_by_block[selected].contains(&other.to_string());
        let expected_score = 2 + u64::from(accept_other_blue);
        let expected_work = 4 + 2 * u64::from(accept_other_blue);
        assert_eq!(commit.blue_score, expected_score);
        assert_eq!(expected["blue_score"].as_u64().unwrap(), expected_score);
        assert_eq!(text_field(expected, "blue_work"), expected_work.to_string());
        let blues: Vec<&str> = expected["merge_blues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        let reds: Vec<&str> = expected["merge_reds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert_eq!(
            blues,
            if accept_other_blue {
                vec![other]
            } else {
                vec![]
            }
        );
        assert_eq!(
            reds,
            if accept_other_blue {
                vec![]
            } else {
                vec![other]
            }
        );

        // The frozen join itself also verifies as a PoW-valid child whose
        // full side ancestry is authenticated, not merely a standalone hash.
        let closure = derive_dormant_native_v3_all_parent_merge_sets(
            chain,
            ns,
            &genesis,
            &[text_field(join, "id").to_string()],
            &headers,
        )
        .expect("native Rust must verify full immutable join ancestry");
        assert_eq!(
            closure
                .topological_evaluation
                .verified_dag
                .authenticated_non_genesis_headers,
            3
        );
    }
}
