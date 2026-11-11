//! Native-v3 GD-08 isolated ordering-kernel regression; no monetary admission.
use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    native_gd08_order_kernel_v3::{
        derive_dormant_native_gd08_order, DormantGd08AnchorSlot, DormantGd08Role,
    },
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    types::{compute_merkle_root, BlockId},
};

const CHAIN: &str = "native-v3-gd08-dormant-order-kernel";
const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
const BITS: u32 = 0x207f_ffff;

fn add(
    headers: &mut BTreeMap<BlockId, BlockHeaderV3>,
    mut parents: Vec<BlockId>,
    height: u64,
    timestamp_ns: u64,
    valid_pow: bool,
) -> BlockId {
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: "33".repeat(32),
        blue_score: height,
        height,
    };
    header.nonce = (0..2048_u64)
        .find(|nonce| {
            header.nonce = *nonce;
            evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted == valid_pow
        })
        .expect("easy target must find valid KHeavyHash PoW");
    let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
    headers.insert(hash.clone(), header);
    hash
}

// A named fixture type keeps the shape explicit without disabling Clippy.
type Gd08Corpus = (
    BlockId,
    Vec<BlockId>,
    BlockId,
    BTreeMap<BlockId, BlockHeaderV3>,
    BTreeMap<BlockId, DormantGd08AnchorSlot>,
);

fn corpus(valid_side_pow: bool) -> Gd08Corpus {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header)]);
    let a = add(
        &mut headers,
        vec![genesis.hash.clone()],
        1,
        GENESIS_NS + 500_000_000,
        true,
    );
    let b = add(
        &mut headers,
        vec![genesis.hash.clone()],
        1,
        GENESIS_NS + 600_000_000,
        valid_side_pow,
    );
    let join = add(
        &mut headers,
        vec![a.clone(), b.clone()],
        2,
        GENESIS_NS + 1_000_000_000,
        true,
    );
    let spine = vec![genesis.hash.clone(), a.clone(), join.clone()];
    let assignments = BTreeMap::from([
        (
            genesis.hash.clone(),
            DormantGd08AnchorSlot {
                anchor: 0,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            a,
            DormantGd08AnchorSlot {
                anchor: 1,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            b.clone(),
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::MergeBlue,
            },
        ),
        (
            join,
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::Selected,
            },
        ),
    ]);
    (genesis.hash, spine, b, headers, assignments)
}

#[test]
fn authenticated_real_pow_multiparent_order_is_stable_under_map_permutations() {
    let (genesis, spine, side, headers, assignments) = corpus(true);
    let stable = derive_dormant_native_gd08_order(
        CHAIN,
        GENESIS_NS,
        &genesis,
        &spine,
        &headers,
        &assignments,
    )
    .unwrap();
    let reversed = headers.clone().into_iter().rev().collect();
    let same = derive_dormant_native_gd08_order(
        CHAIN,
        GENESIS_NS,
        &genesis,
        &spine,
        &reversed,
        &assignments,
    )
    .unwrap();
    assert_eq!(stable, same);
    assert_eq!(stable.ordered.len(), 4);
    assert_eq!(stable.ordered.first(), Some(&genesis));
    assert_eq!(stable.ordered.last(), spine.last());
    assert!(stable.ordered.contains(&side));
    assert_eq!(stable.blue_count, 1);
    assert_eq!(stable.red_count, 0);
}

#[test]
fn hash_consistent_invalid_pow_side_branch_is_rejected_by_real_pow_validator() {
    // Unlike a nonce-only mutation with stale map key, this corpus is built
    // with *consistent* header IDs for the invalid side and the valid join.
    let (genesis, spine, _side, headers, assignments) = corpus(false);
    let err = derive_dormant_native_gd08_order(
        CHAIN,
        GENESIS_NS,
        &genesis,
        &spine,
        &headers,
        &assignments,
    )
    .unwrap_err();
    assert!(err.to_string().contains("invalid PoW"), "{err}");
}

#[test]
fn rejects_incomplete_wrong_anchor_cross_chain_and_inconsistent_hash() {
    let (genesis, spine, side, headers, assignments) = corpus(true);
    let check = |h: &BTreeMap<_, _>, s: &[BlockId], a: &BTreeMap<_, _>| {
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &genesis, s, h, a)
    };
    let mut wrong = assignments.clone();
    wrong.get_mut(&side).unwrap().anchor = 1;
    assert!(check(&headers, &spine, &wrong).is_err());
    wrong = assignments.clone();
    wrong.remove(&side);
    assert!(check(&headers, &spine, &wrong).is_err());

    let mut missing = headers.clone();
    missing.remove(&side);
    assert!(check(&missing, &spine, &assignments).is_err());

    let mut duplicate_spine = spine.clone();
    duplicate_spine.insert(1, genesis.clone());
    assert!(check(&headers, &duplicate_spine, &assignments).is_err());

    let mut forged = headers.clone();
    forged.get_mut(&side).unwrap().nonce ^= 1;
    assert!(check(&forged, &spine, &assignments).is_err());

    assert!(derive_dormant_native_gd08_order(
        "untrusted-foreign-chain",
        GENESIS_NS,
        &genesis,
        &spine,
        &headers,
        &assignments,
    )
    .is_err());
}
#[test]
fn blue_and_red_sibling_forks_order_by_authenticated_anchor_roles() {
    // Five REAL native-v3 PoW headers: three equally high competitors and
    // their authenticated 3-parent merge. Symbolic work/K authority is NOT
    // assumed. Only the supplied dormant role/anchor contract is exercised.
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let g = genesis.hash;
    let mut headers = BTreeMap::from([(g.clone(), genesis.header)]);
    let selected = add(
        &mut headers,
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        true,
    );
    let blue = add(
        &mut headers,
        vec![g.clone()],
        1,
        GENESIS_NS + 600_000_000,
        true,
    );
    let red = add(
        &mut headers,
        vec![g.clone()],
        1,
        GENESIS_NS + 700_000_000,
        true,
    );
    let tip = add(
        &mut headers,
        vec![selected.clone(), blue.clone(), red.clone()],
        2,
        GENESIS_NS + 1_000_000_000,
        true,
    );
    let spine = vec![g.clone(), selected.clone(), tip.clone()];
    let roles = BTreeMap::from([
        (
            g.clone(),
            DormantGd08AnchorSlot {
                anchor: 0,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            selected.clone(),
            DormantGd08AnchorSlot {
                anchor: 1,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            blue.clone(),
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::MergeBlue,
            },
        ),
        (
            red.clone(),
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::MergeRed,
            },
        ),
        (
            tip.clone(),
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::Selected,
            },
        ),
    ]);
    let order =
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &headers, &roles).unwrap();
    assert_eq!(
        order.ordered,
        vec![
            g.clone(),
            selected.clone(),
            blue.clone(),
            red.clone(),
            tip.clone()
        ]
    );
    assert_eq!(order.blue_count, 1);
    assert_eq!(order.red_count, 1);

    // Reinsert verified headers and role assignments in reverse order; the
    // independently authenticated preview must have identical order/digest.
    let reversed_headers = headers.clone().into_iter().rev().collect();
    let reversed_roles = roles.clone().into_iter().rev().collect();
    let replay = derive_dormant_native_gd08_order(
        CHAIN,
        GENESIS_NS,
        &g,
        &spine,
        &reversed_headers,
        &reversed_roles,
    )
    .unwrap();
    assert_eq!(order, replay);

    // An unrelated valid PoW sibling that does not belong to the tip ancestor
    // closure must not be credited or change the digest.
    let mut extended = headers.clone();
    let unselected = add(
        &mut extended,
        vec![g.clone()],
        1,
        GENESIS_NS + 800_000_000,
        true,
    );
    assert!(!order.ordered.contains(&unselected));
    assert_eq!(
        order,
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &extended, &roles,)
            .unwrap()
    );

    // Roles are not *derived* here. When externally supplied roles change,
    // the preview and cryptographic digest must change rather than silently
    // preserving an old monetary/order commitment.
    let mut swapped_roles = roles;
    swapped_roles.get_mut(&blue).unwrap().role = DormantGd08Role::MergeRed;
    swapped_roles.get_mut(&red).unwrap().role = DormantGd08Role::MergeBlue;
    let swapped =
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &headers, &swapped_roles)
            .unwrap();
    assert_eq!(swapped.ordered, vec![g, selected, red, blue, tip]);
    assert_ne!(order.digest, swapped.digest);
    assert_eq!(swapped.blue_count, 1);
    assert_eq!(swapped.red_count, 1);
}

#[test]
fn delayed_side_branch_requires_its_first_real_selected_merge_anchor() {
    // A side branch created after a selected merge first enters the selected
    // ancestry at the NEXT selected block. Earlier anchoring would silently
    // leak a transaction into a prior ordering/reward segment.
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let g = genesis.hash;
    let mut headers = BTreeMap::from([(g.clone(), genesis.header)]);
    let a = add(
        &mut headers,
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        true,
    );
    let b = add(
        &mut headers,
        vec![g.clone()],
        1,
        GENESIS_NS + 600_000_000,
        true,
    );
    let join = add(
        &mut headers,
        vec![a.clone(), b.clone()],
        2,
        GENESIS_NS + 1_000_000_000,
        true,
    );
    let delayed = add(
        &mut headers,
        vec![a.clone()],
        2,
        GENESIS_NS + 1_100_000_000,
        true,
    );
    let tip = add(
        &mut headers,
        vec![join.clone(), delayed.clone()],
        3,
        GENESIS_NS + 1_500_000_000,
        true,
    );
    let spine = vec![g.clone(), a.clone(), join.clone(), tip.clone()];
    let roles = BTreeMap::from([
        (
            g.clone(),
            DormantGd08AnchorSlot {
                anchor: 0,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            a.clone(),
            DormantGd08AnchorSlot {
                anchor: 1,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            b.clone(),
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::MergeBlue,
            },
        ),
        (
            join.clone(),
            DormantGd08AnchorSlot {
                anchor: 2,
                role: DormantGd08Role::Selected,
            },
        ),
        (
            delayed.clone(),
            DormantGd08AnchorSlot {
                anchor: 3,
                role: DormantGd08Role::MergeRed,
            },
        ),
        (
            tip.clone(),
            DormantGd08AnchorSlot {
                anchor: 3,
                role: DormantGd08Role::Selected,
            },
        ),
    ]);
    let valid =
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &headers, &roles).unwrap();
    assert_eq!(
        valid.ordered,
        vec![g.clone(), a, b, join, delayed.clone(), tip]
    );
    assert_eq!(valid.ordered.len(), 6);

    // An authentic child cannot be attached to a selected ancestor that has
    // not yet observed it. The kernel must reject this before producing a
    // digest or an order that downstream accounting could misuse.
    let mut forged = roles.clone();
    forged.get_mut(&delayed).unwrap().anchor = 2;
    let error = derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &headers, &forged)
        .unwrap_err();
    assert!(error.to_string().contains("non-earliest merge anchor"));

    forged.get_mut(&delayed).unwrap().anchor = 4;
    assert!(
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &headers, &forged,)
            .is_err()
    );

    forged = roles.clone();
    forged.get_mut(&delayed).unwrap().role = DormantGd08Role::Selected;
    assert!(
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &headers, &forged,)
            .is_err()
    );

    let mut incomplete = headers.clone();
    incomplete.remove(&delayed);
    assert!(
        derive_dormant_native_gd08_order(CHAIN, GENESIS_NS, &g, &spine, &incomplete, &roles,)
            .is_err()
    );
}
