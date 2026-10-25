//! TEST-ONLY checkpoint/prune/continue differential regression for native v3.
//! The checkpoint is pre-authorized IN THIS TEST by independently checking the
//! entire prefix first. A peer-provided checksum is NOT trusted authority.
//! This exercises one bounded linear-prefix / 3-way-fork / join / child shape.
//! It does not provide general anticone proofs, real finality, signed monetary
//! transactions, production pruning, or permission to activate v3 mining.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    monetary_v3::{subsidy_atoms_for_score, PRODUCTION_CADENCE_V3},
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial, CandidateBlockMetadata, CandidateWork512,
    },
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_v3::expected_difficulty_for_v3_window_ns,
    state::UtxoState,
    types::BlockId,
};

#[path = "support/native_v3_utxo_preview.rs"]
#[allow(dead_code)]
mod detached_utxo;
#[path = "support/native_v3_trial_order_preview.rs"]
#[allow(dead_code)]
mod trial_order_preview;

use detached_utxo::{apply, coin, fingerprint, total_supply, HypotheticalBlockEffects, PreviewSet};
use trial_order_preview::preview_unapproved_native_v3_order;

const CHAIN: &str = "native-v3-DISPOSABLE-checkpoint-differential";
const GENESIS_NS: u64 = 1_800_000_024_000_000_000;
const BITS: u32 = 0x207f_ffff;
const K: usize = 1;
const PREFIX: u64 = 32;

fn mine(
    parents: Vec<BlockId>,
    height: u64,
    score: u64,
    ns: u64,
    tag: u8,
) -> (BlockId, BlockHeaderV3) {
    let mut parents = parents;
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns: ns,
        difficulty: BITS,
        nonce: 0,
        merkle_root: format!("{tag:02x}").repeat(32),
        state_root: "55".repeat(32), // Deliberate non-monetary placeholder.
        blue_score: score,
        height,
    };
    for nonce in 0..4096 {
        header.nonce = nonce;
        let proof = evaluate_v3_header_pow(&header, CHAIN).unwrap();
        if proof.pow.accepted {
            return (proof.block_hash, header);
        }
    }
    panic!("easy PoW search failed");
}

fn state_root(state: &PreviewSet) -> String {
    let mut utxo = UtxoState::default();
    for output in state.values() {
        assert!(utxo
            .utxos
            .insert(output.outpoint.clone(), output.clone())
            .is_none());
        utxo.address_index
            .entry(output.address.clone())
            .or_default()
            .push(output.outpoint.clone());
    }
    utxo.validate_deterministic_indexes().unwrap();
    utxo.compute_state_root().unwrap()
}

/// Hypothetical *post-finality* entries, NOT spendable rewards or a v3
/// acceptance rule. The subsidy comes from the existing real monetary curve,
/// but this preview deliberately has neither reward claims nor fee transactions.
fn append_hypothetical_reward(
    state: &PreviewSet,
    block: &str,
    header: &BlockHeaderV3,
    score: u64,
) -> PreviewSet {
    let subsidy = subsidy_atoms_for_score(score, &PRODUCTION_CADENCE_V3).unwrap();
    assert!(subsidy > 0);
    let effect = HypotheticalBlockEffects {
        block: block.to_owned(),
        spent: vec![],
        created: vec![coin(
            block,
            subsidy,
            "trial-only-unsettled",
            true,
            header.height,
        )],
        test_issuance: subsidy,
        test_fee_burn: 0,
    };
    apply(state, &effect).unwrap()
}

/// Only an in-test preauthenticated root may be used here. Never accept a
/// network-supplied copy of the seal as proof of work/finality.
#[derive(Clone)]
struct PreauthenticatedTrialCheckpoint {
    chain: String,
    frozen_genesis: BlockId,
    boundary: BlockId,
    k: usize,
    boundary_meta: CandidateBlockMetadata,
    recent_selected: Vec<BlockId>,
    selected_parents: BTreeMap<BlockId, Option<BlockId>>,
    preview_state: PreviewSet,
    preview_state_root: String,
}

type ResumeOutcome = (
    BTreeMap<BlockId, CandidateBlockMetadata>,
    Vec<pulsedag_core::retarget_v3::V3RetargetSample>,
    PreviewSet,
);

fn add_two_work(work: CandidateWork512) -> Result<CandidateWork512, &'static str> {
    let mut limbs = work.0;
    limbs[0] = limbs[0]
        .checked_add(2)
        .ok_or("candidate u512 low-limb overflow")?;
    Ok(CandidateWork512(limbs))
}

/// A deliberately narrow, independently computed, checkpoint-seeded
/// K=1 continuation for THREE equal-target siblings and one 3-parent rejoin.
/// Uses no pruned header or genesis replay. It is NOT a general GHOSTDAG engine.
fn continue_from_test_checkpoint(
    checkpoint: &PreauthenticatedTrialCheckpoint,
    externally_authenticated: &PreauthenticatedTrialCheckpoint,
    retained: &BTreeMap<BlockId, BlockHeaderV3>,
    suffix: &BTreeMap<BlockId, BlockHeaderV3>,
    join: &str,
    child: &str,
) -> Result<ResumeOutcome, String> {
    if checkpoint.chain != externally_authenticated.chain
        || checkpoint.frozen_genesis != externally_authenticated.frozen_genesis
        || checkpoint.boundary != externally_authenticated.boundary
        || checkpoint.k != externally_authenticated.k
        || checkpoint.boundary_meta != externally_authenticated.boundary_meta
        || checkpoint.recent_selected != externally_authenticated.recent_selected
        || checkpoint.selected_parents != externally_authenticated.selected_parents
        || checkpoint.preview_state_root != externally_authenticated.preview_state_root
        || fingerprint(&checkpoint.preview_state)
            != fingerprint(&externally_authenticated.preview_state)
        || checkpoint.chain != CHAIN
        || checkpoint.k != K
        || checkpoint.recent_selected.len() != 20
        || state_root(&checkpoint.preview_state) != checkpoint.preview_state_root
    {
        return Err("checkpoint not authorized by independently verified test prefix".into());
    }
    if retained.len() != checkpoint.recent_selected.len()
        || checkpoint
            .recent_selected
            .iter()
            .any(|id| !retained.contains_key(id))
    {
        return Err("retained authenticated retarget lookback incomplete".into());
    }
    if suffix.len() != 5
        || suffix.contains_key(&checkpoint.boundary)
        || retained.contains_key(join)
        || retained.contains_key(child)
    {
        return Err("unexpected post-boundary topology".into());
    }
    let joined = suffix.get(join).ok_or("join absent")?;
    let last = suffix.get(child).ok_or("child absent")?;
    let mut siblings: Vec<BlockId> = suffix
        .keys()
        .filter(|id| id.as_str() != join && id.as_str() != child)
        .cloned()
        .collect();
    siblings.sort_unstable();
    if siblings.len() != 3
        || siblings.iter().any(|id| {
            let h = &suffix[id];
            h.parents != [checkpoint.boundary.clone()]
                || h.height != PREFIX + 1
                || h.blue_score != PREFIX + 1
                || h.timestamp_ns <= retained[&checkpoint.boundary].timestamp_ns
                || h.difficulty != BITS
        })
        || joined.parents != siblings
        || joined.height != PREFIX + 2
        || joined.blue_score != PREFIX + 3
        || last.parents != [join.to_owned()]
        || last.height != PREFIX + 3
        || last.blue_score != PREFIX + 4
        || joined.timestamp_ns
            <= siblings
                .iter()
                .map(|id| suffix[id].timestamp_ns)
                .max()
                .unwrap()
        || last.timestamp_ns <= joined.timestamp_ns
        || joined.difficulty != BITS
        || last.difficulty != BITS
    {
        return Err("post-boundary topology, score, target or timestamp malformed".into());
    }
    for (id, header) in suffix {
        if compute_block_hash_v3(header, CHAIN).map_err(|e| e.to_string())? != *id
            || verify_v3_header_pow(header, CHAIN).is_err()
        {
            return Err("missing or invalid genuine KHeavyHash suffix".into());
        }
    }

    // All three have identical target work and score, so the candidate's
    // final deterministic hash tie breaks selected parent and blue/red.
    let selected = siblings[0].clone();
    let blue = siblings[1].clone();
    let red = siblings[2].clone();
    let sibling_work = add_two_work(checkpoint.boundary_meta.blue_work)?;
    let joined_work = add_two_work(add_two_work(sibling_work)?)?;
    let child_work = add_two_work(joined_work)?;
    let mut continuation = BTreeMap::new();
    for id in &siblings {
        continuation.insert(
            id.clone(),
            CandidateBlockMetadata {
                selected_parent: Some(checkpoint.boundary.clone()),
                blue_score: PREFIX + 1,
                blue_work: sibling_work,
                merge_blues: vec![],
                merge_reds: vec![],
            },
        );
    }
    continuation.insert(
        join.to_owned(),
        CandidateBlockMetadata {
            selected_parent: Some(selected.clone()),
            blue_score: PREFIX + 3,
            blue_work: joined_work,
            merge_blues: vec![blue.clone()],
            merge_reds: vec![red.clone()],
        },
    );
    continuation.insert(
        child.to_owned(),
        CandidateBlockMetadata {
            selected_parent: Some(join.to_owned()),
            blue_score: PREFIX + 4,
            blue_work: child_work,
            merge_blues: vec![],
            merge_reds: vec![],
        },
    );
    let mut retained_and_new = retained.clone();
    retained_and_new.extend(suffix.iter().map(|(id, h)| (id.clone(), h.clone())));
    let mut selected_map = checkpoint.selected_parents.clone();
    for (id, meta) in &continuation {
        selected_map.insert(id.clone(), meta.selected_parent.clone());
    }
    let window = selected_parent_retarget_window_v3_ns(
        CHAIN,
        &child.to_owned(),
        &retained_and_new,
        &selected_map,
    )
    .map_err(|e| format!("authenticated retained retarget context: {e}"))?;

    let mut ledger = checkpoint.preview_state.clone();
    // Parent-before-child with the candidate's proposed blue/red ordering.
    // Red blocks are not silently dropped from the bookkeeping experiment.
    for id in [selected, blue, red, join.to_owned(), child.to_owned()] {
        ledger =
            append_hypothetical_reward(&ledger, &id, &suffix[&id], continuation[&id].blue_score);
    }
    Ok((continuation, window, ledger))
}

#[test]
fn preauthenticated_linear_prefix_can_be_pruned_and_trial_resumed_equal_to_full_dag() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let g = genesis.hash.clone();
    let mut all = BTreeMap::from([(g.clone(), genesis.header)]);
    let mut linear = Vec::new();
    let mut previous = g.clone();
    for height in 1..=PREFIX {
        let (id, header) = mine(
            vec![previous],
            height,
            height,
            GENESIS_NS + height * 500_000_000,
            height as u8,
        );
        all.insert(id.clone(), header);
        linear.push(id.clone());
        previous = id;
    }
    let anchor = previous;
    let prefix_trial = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&anchor),
        &all,
        K,
    )
    .unwrap();
    let mut pref_state = PreviewSet::new();
    for id in &linear {
        pref_state = append_hypothetical_reward(
            &pref_state,
            id,
            &all[id],
            prefix_trial.blocks[id].blue_score,
        );
    }
    let selected20: Vec<_> = linear.iter().rev().take(20).cloned().collect();
    let selected_parents = selected20
        .iter()
        .map(|id| (id.clone(), prefix_trial.blocks[id].selected_parent.clone()))
        .collect();
    let cp = PreauthenticatedTrialCheckpoint {
        chain: CHAIN.into(),
        frozen_genesis: g.clone(),
        boundary: anchor.clone(),
        k: K,
        boundary_meta: prefix_trial.blocks[&anchor].clone(),
        recent_selected: selected20.clone(),
        selected_parents,
        preview_state_root: state_root(&pref_state),
        preview_state: pref_state,
    };
    assert_eq!(cp.boundary_meta.blue_work.0[0], PREFIX * 2);
    let trusted_cp = cp.clone(); // Captured ONLY after full frozen-genesis validation.

    let mut suffix = BTreeMap::new();
    let mut siblings = Vec::new();
    for index in 0..3_u8 {
        let (id, header) = mine(
            vec![anchor.clone()],
            PREFIX + 1,
            PREFIX + 1,
            GENESIS_NS + (PREFIX + 1) * 500_000_000 + u64::from(index) * 20_000_000,
            0xb0 + index,
        );
        suffix.insert(id.clone(), header);
        siblings.push(id);
    }
    siblings.sort();
    let (join, joined) = mine(
        siblings.clone(),
        PREFIX + 2,
        PREFIX + 3,
        GENESIS_NS + (PREFIX + 2) * 500_000_000,
        0xc0,
    );
    suffix.insert(join.clone(), joined);
    let (tip, last) = mine(
        vec![join.clone()],
        PREFIX + 3,
        PREFIX + 4,
        GENESIS_NS + (PREFIX + 3) * 500_000_000,
        0xd0,
    );
    suffix.insert(tip.clone(), last);
    all.extend(suffix.clone());

    // This is the independent whole-DAG oracle, not used by the resume helper.
    let full = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&tip),
        &all,
        K,
    )
    .unwrap();
    let order = preview_unapproved_native_v3_order(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&tip),
        &all,
        K,
    )
    .unwrap();
    let mut complete_ledger = PreviewSet::new();
    for id in order.ordered_blocks.iter().filter(|id| *id != &g) {
        complete_ledger =
            append_hypothetical_reward(&complete_ledger, id, &all[id], full.blocks[id].blue_score);
    }
    let selected_full: BTreeMap<_, _> = full
        .blocks
        .iter()
        .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
        .collect();
    let full_window =
        selected_parent_retarget_window_v3_ns(CHAIN, &tip, &all, &selected_full).unwrap();

    // ACTUAL PRUNING of the local resume input; genesis and old PoW ancestors
    // are absent. Mainline full-genesis verifier must still reject it.
    let retained: BTreeMap<_, _> = selected20
        .iter()
        .map(|id| (id.clone(), all[id].clone()))
        .collect();
    assert!(!retained.contains_key(&g));
    assert!(!retained.contains_key(&linear[0]));
    let mut cut = retained.clone();
    cut.extend(suffix.clone());
    assert!(
        derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&tip),
            &cut,
            K
        )
        .is_err(),
        "production-facing diagnostics must NEVER accept pruned history"
    );

    let (continued, compact_window, compact_ledger) =
        continue_from_test_checkpoint(&cp, &trusted_cp, &retained, &suffix, &join, &tip).unwrap();
    for (id, meta) in &continued {
        assert_eq!(
            meta, &full.blocks[id],
            "GHOSTDAG selected-parent/K/blue-work differs for {id}"
        );
    }
    assert_eq!(continued[&join].merge_blues.len(), 1);
    assert_eq!(continued[&join].merge_reds.len(), 1);
    assert_eq!(
        compact_window, full_window,
        "20-sample real retarget adapter diverged"
    );
    assert_eq!(
        expected_difficulty_for_v3_window_ns(&compact_window).unwrap(),
        expected_difficulty_for_v3_window_ns(&full_window).unwrap()
    );
    assert_eq!(state_root(&compact_ledger), state_root(&complete_ledger));
    assert_eq!(fingerprint(&compact_ledger), fingerprint(&complete_ledger));
    assert_eq!(
        total_supply(&compact_ledger).unwrap(),
        total_supply(&complete_ledger).unwrap()
    );

    let mut forged = cp.clone();
    forged.boundary_meta.blue_work.0[0] += 2;
    assert!(
        continue_from_test_checkpoint(&forged, &trusted_cp, &retained, &suffix, &join, &tip)
            .is_err()
    );
    let mut forged = cp.clone();
    forged.preview_state_root = "00".repeat(32);
    assert!(
        continue_from_test_checkpoint(&forged, &trusted_cp, &retained, &suffix, &join, &tip)
            .is_err()
    );
    let mut missing = retained.clone();
    missing.remove(&selected20[16]); // In the 20-sample suffix window.
    assert!(
        continue_from_test_checkpoint(&cp, &trusted_cp, &missing, &suffix, &join, &tip).is_err()
    );
    let mut missing_red = suffix.clone();
    missing_red.remove(&siblings[2]);
    assert!(
        continue_from_test_checkpoint(&cp, &trusted_cp, &retained, &missing_red, &join, &tip)
            .is_err()
    );
    let mut corrupt = suffix.clone();
    corrupt.get_mut(&tip).unwrap().nonce ^= 1;
    assert!(
        continue_from_test_checkpoint(&cp, &trusted_cp, &retained, &corrupt, &join, &tip).is_err()
    );
    let mut wrong_chain = cp.clone();
    wrong_chain.chain = "forged-chain".into();
    assert!(continue_from_test_checkpoint(
        &wrong_chain,
        &trusted_cp,
        &retained,
        &suffix,
        &join,
        &tip
    )
    .is_err());
}


/// A second deliberately bounded checkpoint case: one already-authenticated
/// sibling (a side DAG frontier) shares the checkpoint boundary's height and
/// parent, then a NEW two-parent merge crosses the prune frontier. Runs both
/// K=0 (side RED) and K=1 (side BLUE); neither policy is network-approved.
/// The candidate checkpoint is usable only after byte-for-byte comparison
/// with a separately frozen full-genesis oracle captured before deletion.
#[test]
fn pruned_boundary_with_preserved_competing_side_frontier_matches_full_dag_for_k0_and_k1() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let g = genesis.hash.clone();
    let mut full_prefix = BTreeMap::from([(g.clone(), genesis.header)]);
    let mut linear = Vec::new();
    let mut parent = g.clone();
    for height in 1..=PREFIX {
        let (id, header) = mine(
            vec![parent], height, height,
            GENESIS_NS + height * 500_000_000,
            height as u8
        );
        full_prefix.insert(id.clone(), header);
        linear.push(id.clone());
        parent = id;
    }
    let main_boundary = linear.last().unwrap().clone();
    let pre_boundary_parent = linear[30].clone(); // height 31, retained.
    let (side, side_header) = mine(
        vec![pre_boundary_parent.clone()], PREFIX, PREFIX,
        GENESIS_NS + PREFIX * 500_000_000 + 50_000_000,
        0xee
    );
    assert_ne!(side, main_boundary);
    full_prefix.insert(side.clone(), side_header);
    let selected = std::cmp::min(side.clone(), main_boundary.clone());
    let other = std::cmp::max(side, main_boundary);
    for trial_k in [0_usize, 1] {
        // Independently derive the complete pre-trim 2-tip DAG, before
        // any header is removed from the future resume input.
        let prefix = derive_dormant_native_v3_ghostdag_trial(
            CHAIN, GENESIS_NS, &g,
            &[selected.clone(), other.clone()],
            &full_prefix, trial_k
        ).unwrap();
        assert_eq!(prefix.blocks[&selected].blue_score, PREFIX);
        assert_eq!(prefix.blocks[&other].blue_score, PREFIX);
        assert_eq!(prefix.blocks[&selected].blue_work.0[0], PREFIX * 2);
        assert_eq!(prefix.blocks[&other].blue_work.0[0], PREFIX * 2);

        // Trust anchor is a separate in-memory object, obtained only from
        // COMPLETE authenticated history. No supplied peer checksum suffices.
        let captured_selected = prefix.blocks[&selected].clone();
        let captured_other = prefix.blocks[&other].clone();
        let selected20: Vec<_> = std::iter::once(selected.clone())
            .chain(linear.iter().take(31).rev().take(19).cloned())
            .collect();
        assert_eq!(selected20.len(), 20);
        let selected_parents: BTreeMap<_, _> = selected20.iter()
            .map(|id| (id.clone(), prefix.blocks[id].selected_parent.clone()))
            .collect();
        let retained: BTreeMap<_, _> = selected20.iter()
            .chain(std::iter::once(&other))
            .map(|id| (id.clone(), full_prefix[id].clone()))
            .collect();
        assert_eq!(retained.len(), 21);
        assert!(!retained.contains_key(&g));
        assert!(!retained.contains_key(&linear[0]));

        let check_frontier = |headers: &BTreeMap<BlockId, BlockHeaderV3>,
                              proposed_other: &BlockId,
                              proposed_selected_meta: &CandidateBlockMetadata,
                              proposed_other_meta: &CandidateBlockMetadata|
         -> Result<(), String> {
            if headers.len() != 21
                || proposed_other != &other
                || proposed_selected_meta != &captured_selected
                || proposed_other_meta != &captured_other
                || selected20.iter().any(|id| !headers.contains_key(id))
            {
                return Err("checkpoint boundary/frontier differs from preauthenticated source".into());
            }
            let h = headers.get(proposed_other).ok_or("side header missing")?;
            if h.parents != [pre_boundary_parent.clone()]
                || h.blue_score != PREFIX
                || h.height != PREFIX
                || h.difficulty != BITS
                || !headers.contains_key(&pre_boundary_parent)
            {
                return Err("required crossing-boundary parent witness absent".into());
            }
            for (hash, header) in headers {
                if compute_block_hash_v3(header, CHAIN).map_err(|e| e.to_string())? != *hash
                    || verify_v3_header_pow(header, CHAIN).is_err()
                {
                    return Err("invalid retained authentic KHeavyHash context".into());
                }
            }
            Ok(())
        };
        check_frontier(&retained, &other, &captured_selected, &captured_other).unwrap();

        let (join, join_header) = mine(
            vec![selected.clone(), other.clone()], PREFIX + 1,
            PREFIX + 1 + trial_k as u64,
            GENESIS_NS + (PREFIX + 1) * 500_000_000,
            0xc8 + trial_k as u8
        );
        let (child, child_header) = mine(
            vec![join.clone()], PREFIX + 2,
            PREFIX + 2 + trial_k as u64,
            GENESIS_NS + (PREFIX + 2) * 500_000_000,
            0xd8 + trial_k as u8
        );
        let mut all = full_prefix.clone();
        all.insert(join.clone(), join_header.clone());
        all.insert(child.clone(), child_header.clone());
        let full = derive_dormant_native_v3_ghostdag_trial(
            CHAIN, GENESIS_NS, &g, std::slice::from_ref(&child), &all, trial_k
        ).unwrap();
        let expected_merge_blues = if trial_k == 0 { vec![] } else { vec![other.clone()] };
        let expected_merge_reds = if trial_k == 0 { vec![other.clone()] } else { vec![] };
        assert_eq!(full.blocks[&join].selected_parent.as_deref(), Some(selected.as_str()));
        assert_eq!(full.blocks[&join].merge_blues, expected_merge_blues);
        assert_eq!(full.blocks[&join].merge_reds, expected_merge_reds);
        assert_eq!(full.blocks[&join].blue_score, PREFIX + 1 + trial_k as u64);
        assert_eq!(full.blocks[&join].blue_work.0[0], (PREFIX + 1 + trial_k as u64) * 2);
        assert_eq!(full.blocks[&child].blue_score, PREFIX + 2 + trial_k as u64);
        assert_eq!(full.blocks[&child].blue_work.0[0], (PREFIX + 2 + trial_k as u64) * 2);

        // Independent checkpoint continuation (not running the whole-DAG
        // candidate verifier on cut history); it keeps the side frontier
        // whether RED or BLUE rather than silently dropping its effects.
        let mut retained_and_new = retained.clone();
        retained_and_new.insert(join.clone(), join_header.clone());
        retained_and_new.insert(child.clone(), child_header.clone());
        let mut resumed_selection = selected_parents.clone();
        resumed_selection.insert(other.clone(), Some(pre_boundary_parent.clone()));
        resumed_selection.insert(join.clone(), Some(selected.clone()));
        resumed_selection.insert(child.clone(), Some(join.clone()));
        let restored_window = selected_parent_retarget_window_v3_ns(
            CHAIN, &child, &retained_and_new, &resumed_selection
        ).unwrap();
        let full_selection: BTreeMap<_, _> = full.blocks.iter()
            .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
            .collect();
        let unpruned_window = selected_parent_retarget_window_v3_ns(
            CHAIN, &child, &all, &full_selection
        ).unwrap();
        assert_eq!(restored_window, unpruned_window);
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&restored_window).unwrap(),
            expected_difficulty_for_v3_window_ns(&unpruned_window).unwrap()
        );

        let mut snapshot = PreviewSet::new();
        for id in linear.iter().take(31).chain(std::iter::once(&selected)) {
            snapshot = append_hypothetical_reward(
                &snapshot, id, &full_prefix[id], PREFIX.min(full_prefix[id].height)
            );
        }
        let independently_captured_root = state_root(&snapshot);
        assert_ne!(independently_captured_root, state_root(&PreviewSet::new()));
        let mut resumed = snapshot;
        for (id, score) in [
            (other.clone(), PREFIX), (join.clone(), PREFIX + 1 + trial_k as u64),
            (child.clone(), PREFIX + 2 + trial_k as u64)
        ] {
            resumed = append_hypothetical_reward(&resumed, &id, &all[&id], score);
        }

        let order = preview_unapproved_native_v3_order(
            CHAIN, GENESIS_NS, &g, std::slice::from_ref(&child), &all, trial_k
        ).unwrap();
        let mut from_genesis = PreviewSet::new();
        for id in order.ordered_blocks.iter().filter(|id| *id != &g) {
            from_genesis = append_hypothetical_reward(
                &from_genesis, id, &all[id], full.blocks[id].blue_score
            );
        }
        assert_eq!(state_root(&resumed), state_root(&from_genesis));
        assert_eq!(fingerprint(&resumed), fingerprint(&from_genesis));
        assert_eq!(total_supply(&resumed).unwrap(), total_supply(&from_genesis).unwrap());

        assert!(derive_dormant_native_v3_ghostdag_trial(
            CHAIN, GENESIS_NS, &g, std::slice::from_ref(&child),
            &retained_and_new, trial_k
        ).is_err(), "full-genesis verifier must reject pruned ancestor closure");

        let mut omitted = retained.clone();
        omitted.remove(&other);
        assert!(check_frontier(&omitted, &other, &captured_selected, &captured_other).is_err());
        let mut missing_parent = retained.clone();
        missing_parent.remove(&pre_boundary_parent);
        assert!(check_frontier(&missing_parent, &other, &captured_selected, &captured_other).is_err());
        let mut changed = retained.clone();
        changed.get_mut(&other).unwrap().nonce ^= 1;
        assert!(check_frontier(&changed, &other, &captured_selected, &captured_other).is_err());
        let mut forged = captured_other.clone();
        forged.blue_work.0[0] += 2;
        assert!(check_frontier(&retained, &other, &captured_selected, &forged).is_err());

        // A newly received real-PoW side branch based on a PRUNED ancestor
        // is NOT authorized by a retained tip-only frontier.
        let (old_fork, old_header) = mine(
            vec![linear[3].clone()], 5, 5,
            GENESIS_NS + 5 * 500_000_000 + 100_000_000, 0xf7
        );
        let mut without_old_past = retained.clone();
        without_old_past.insert(old_fork, old_header);
        assert!(check_frontier(&without_old_past, &other, &captured_selected, &captured_other).is_err());
    }
}
