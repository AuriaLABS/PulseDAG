//! Non-normative native-v3 GHOSTDAG *local computation* profiling.
//!
//! Real PoW-valid KHeavyHash headers; 500ms intervals are GENERATED, not
//! peer-to-peer propagation measurements. Prints wall-clock microseconds and
//! Linux-process high-water RSS, WITHOUT enforcing noisy CI speed thresholds.
//! Diagnostic ancestry is capped at 128 headers. This suite never connects to
//! consensus admission, mining runtime, monetary rewards, UTXO or contracts.

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use sha2::{Digest, Sha256};

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial, CandidateGhostdagEvaluation,
    },
    pow_v3::evaluate_v3_header_pow,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_genesis_guard_v3::verify_dormant_frozen_genesis_retarget_v3_ns,
    retarget_v3::{expected_difficulty_for_v3_window_ns, V3RetargetSample},
    protocol::BLOCK_HEADER_VERSION_V3,
    types::BlockId,
};

const CHAIN: &str = "pulsedag-native-v3-dormant-profiling-NON-NORMATIVE";
const GENESIS_NS: u64 = 1_800_000_024_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn genesis() -> (BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
    let g = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    (g.hash.clone(), BTreeMap::from([(g.hash, g.header)]))
}

fn mined(
    mut parents: Vec<BlockId>,
    timestamp_ns: u64,
    height: u64,
    blue_score: u64,
    marker: u8,
) -> (BlockId, BlockHeaderV3) {
    parents.sort_unstable();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: format!("{marker:02x}").repeat(32),
        state_root: "55".repeat(32), // Placeholder, never monetary authority.
        blue_score,
        height,
    };
    for nonce in 0..65_536_u64 {
        header.nonce = nonce;
        let pow = evaluate_v3_header_pow(&header, CHAIN).expect("canonical test PoW");
        if pow.pow.accepted {
            return (pow.block_hash, header);
        }
    }
    panic!("test-only easy PoW search bound was exceeded");
}

/// Single selected-parent chain, exactly 500ms apart: zero measured peers.
fn chain(non_genesis: usize) -> (BlockId, BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
    let (g, mut headers) = genesis();
    let mut prev = g.clone();
    for index in 1..=non_genesis {
        let (id, h) = mined(
            vec![prev],
            GENESIS_NS + (index as u64) * 500_000_000,
            index as u64,
            index as u64,
            (index % 251) as u8,
        );
        headers.insert(id.clone(), h);
        prev = id;
    }
    (g, prev, headers)
}

/// Eight genesis siblings and one correctly scored join, with a fixed
/// independent 500ms generation window. Proposed parent cap UNAPPROVED.
fn eight_parent_fork(trial_k: usize) -> (BlockId, BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
    let (g, mut headers) = genesis();
    let mut parents = Vec::with_capacity(8);
    for i in 0..8_u8 {
        let (id, h) = mined(
            vec![g.clone()],
            GENESIS_NS + 500_000_000 + (i as u64) * 100_000,
            1,
            1,
            0xa0 + i,
        );
        headers.insert(id.clone(), h);
        parents.push(id);
    }
    let score = 2 + trial_k.min(7) as u64;
    let (tip, h) = mined(
        parents,
        GENESIS_NS + 1_000_000_000,
        2,
        score,
        0xd0 + (trial_k % 32) as u8,
    );
    headers.insert(tip.clone(), h);
    (g, tip, headers)
}

fn evaluate(
    genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    k: usize,
    expected_score: u64,
) {
    let result = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        genesis,
        std::slice::from_ref(&tip.to_owned()),
        headers,
        k,
    )
    .expect("complete real-PoW ancestry and proposed native-v3 trial");
    assert_eq!(result.blocks.len(), headers.len());
    assert_eq!(result.blocks[tip].blue_score, expected_score);
}

fn vmhwm_kib() -> Option<u64> {
    // Process-wide resident memory HIGH-WATER, not allocs per call.
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn measure(
    label: &str,
    genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    k: usize,
    expected_score: u64,
) {
    let mut micros = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        evaluate(genesis, tip, headers, k, expected_score);
        micros.push(start.elapsed().as_micros());
    }
    micros.sort_unstable();
    println!(
        "NATIVE_V3_PERF_PROBE scope=local_not_network case={label} headers={} K={k} samples_us={micros:?} median_wall_us={} process_vmhwm_kib={:?}",
        headers.len(),
        micros[1],
        vmhwm_kib()
    );
}

#[test]
fn native_v3_diagnostic_budget_fails_closed_above_128_headers() {
    let (genesis, tip, headers) = chain(129);
    assert!(
        derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis,
            std::slice::from_ref(&tip),
            &headers,
            32,
        )
        .is_err(),
        "129 non-genesis ancestors MUST NOT bypass the 128-header local cap"
    );
}

#[test]
#[ignore = "opt-in local CPU/RSS evidence only; not peer-propagation measurement"]
fn native_v3_real_pow_dag_work_profile_is_non_normative() {
    for count in [16_usize, 64, 120] {
        let (g, tip, headers) = chain(count);
        measure(
            &format!("linear_{count}_500ms"),
            &g,
            &tip,
            &headers,
            32,
            count as u64,
        );
    }
    for k in [0_usize, 6, 7, 32] {
        let (g, tip, headers) = eight_parent_fork(k);
        measure(
            &format!("eight_parent_K{k}"),
            &g,
            &tip,
            &headers,
            k,
            2 + k.min(7) as u64,
        );
    }
}

/// TEST-ONLY digest format. This is computed ONLY after *full* authenticated
/// ancestry derivation from frozen genesis. The digest has NO intrinsic
/// authority: it is not a checkpoint witness/proof, signature or finality
/// certificate. It cannot let truncated ancestry bypass the 128-header cap.
fn informational_trial_prefix_fingerprint(
    chain_id: &str,
    trusted_genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    trial: &CandidateGhostdagEvaluation,
) -> Result<String, &'static str> {
    fn commit_str(hash: &mut Sha256, value: &str) -> Result<(), &'static str> {
        let size = u32::try_from(value.len()).map_err(|_| "oversized value")?;
        hash.update(size.to_le_bytes());
        hash.update(value.as_bytes());
        Ok(())
    }

    if chain_id.is_empty() || trusted_genesis.is_empty() || tip.is_empty() {
        return Err("missing identity");
    }
    if !trial.blocks.contains_key(trusted_genesis)
        || !trial.blocks.contains_key(tip)
        || trial.blocks.len() > 128
    {
        return Err("unverified or over-budget prefix");
    }
    // SHA256 commits to exact FULL verified prefix metadata in deterministic
    // hash order, including every selected parent, K color list and u512 work.
    // Headers are an input to the fully authenticated trial, NOT independent
    // authority for this stand-alone diagnostic.
    let mut digest = Sha256::new();
    digest.update(b"PulseDAG:NONNORMATIVE:full-v3-prefix-audit:v1");
    commit_str(&mut digest, chain_id)?;
    commit_str(&mut digest, trusted_genesis)?;
    commit_str(&mut digest, tip)?;
    digest.update((trial.k_trial as u64).to_le_bytes());
    digest.update((trial.blocks.len() as u64).to_le_bytes());
    for (block_hash, meta) in &trial.blocks {
        let header = headers.get(block_hash).ok_or("prefix header missing")?;
        commit_str(&mut digest, block_hash)?;
        digest.update(header.height.to_le_bytes());
        digest.update(header.timestamp_ns.to_le_bytes());
        digest.update(header.difficulty.to_le_bytes());
        digest.update(header.nonce.to_le_bytes());
        digest.update((header.parents.len() as u64).to_le_bytes());
        for parent in &header.parents {
            commit_str(&mut digest, parent)?;
        }
        commit_str(&mut digest, meta.selected_parent.as_deref().unwrap_or(""))?;
        digest.update(meta.blue_score.to_le_bytes());
        for limb in meta.blue_work.0 {
            digest.update(limb.to_le_bytes());
        }
        for group in [&meta.merge_blues, &meta.merge_reds] {
            digest.update((group.len() as u64).to_le_bytes());
            for colored_hash in group {
                commit_str(&mut digest, colored_hash)?;
            }
        }
    }
    Ok(hex::encode(digest.finalize()))
}

#[test]
fn informational_full_prefix_hash_must_never_admit_pruned_native_v3_history() {
    // A complete 120-block chain CAN be independently authenticated from
    // frozen genesis, so a diagnostic digest over its full verified prefix
    // can be compared as data. That digest is not an authorized checkpoint.
    let (genesis, tip, headers) = chain(120);
    let full = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &genesis,
        std::slice::from_ref(&tip),
        &headers,
        32,
    )
    .expect("unpruned complete 120-block authenticated test DAG");
    let baseline =
        informational_trial_prefix_fingerprint(CHAIN, &genesis, &tip, &headers, &full).unwrap();
    let reordered: BTreeMap<_, _> = headers
        .iter()
        .rev()
        .map(|(id, header)| (id.clone(), header.clone()))
        .collect();
    assert_eq!(
        informational_trial_prefix_fingerprint(CHAIN, &genesis, &tip, &reordered, &full).unwrap(),
        baseline,
        "map receipt order must not change full-prefix informational fingerprint"
    );

    // An attacker with a valid header set can still lie about blue-work,
    // selected parent, K or identity in a supplied metadata summary.
    let mut forged = full.clone();
    forged.blocks.get_mut(&tip).unwrap().blue_work.0[0] ^= 1;
    assert_ne!(
        informational_trial_prefix_fingerprint(CHAIN, &genesis, &tip, &headers, &forged).unwrap(),
        baseline
    );
    assert_ne!(
        informational_trial_prefix_fingerprint("another-chain", &genesis, &tip, &headers, &full)
            .unwrap(),
        baseline
    );
    let alternate_k = CandidateGhostdagEvaluation {
        k_trial: 7,
        blocks: full.blocks.clone(),
    };
    assert_ne!(
        informational_trial_prefix_fingerprint(CHAIN, &genesis, &tip, &headers, &alternate_k)
            .unwrap(),
        baseline
    );

    // Most importantly: even possession of the ORIGINAL digest cannot
    // authorize a verifier to skip omitted ancestors or invent a checkpoint.
    let mut truncated = headers.clone();
    truncated.retain(|id, header| id == &genesis || header.height >= 60);
    assert!(!truncated.values().any(|header| header.height == 59));
    assert!(
        derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis,
            std::slice::from_ref(&tip),
            &truncated,
            32
        )
        .is_err(),
        "an informational full-prefix digest MUST NOT bypass missing PoW ancestry"
    );
    assert!(
        informational_trial_prefix_fingerprint(CHAIN, &genesis, &tip, &truncated, &full).is_err(),
        "cannot reproduce a full authenticated prefix fingerprint from missing headers"
    );
}

/// TEST ONLY. This does NOT accept pruned history, peer snapshots or an
/// attacker-supplied "trusted" checkpoint. A saved cache is untrusted input:
/// independently re-evaluate every authenticated header from frozen genesis
/// and compare ALL selected-parent/K-cluster metadata, not merely its digest.
///
/// In particular, a dishonest peer can re-hash a forged cache and produce
/// matching forged checksum bytes. A checksum is NOT an authority token.
fn revalidate_untrusted_full_history_cache(
    trusted_chain: &str,
    trusted_genesis: &str,
    expected_tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    expected_k: usize,
    cached: &CandidateGhostdagEvaluation,
    advertised_digest: &str,
) -> Result<(), &'static str> {
    if cached.k_trial != expected_k || advertised_digest.len() != 64 {
        return Err("cached K or digest format mismatches the trusted request");
    }
    let fresh = derive_dormant_native_v3_ghostdag_trial(
        trusted_chain,
        GENESIS_NS,
        trusted_genesis,
        std::slice::from_ref(&expected_tip.to_owned()),
        headers,
        expected_k,
    )
    .map_err(|_| "could not authenticate entire header history")?;
    if fresh.blocks.len() != headers.len() {
        // Refuse extra unchecked entries as well as missing history.
        return Err("header cache contains unverified or omitted entries");
    }
    if fresh != *cached {
        return Err("persisted selected-parent/blue-work/merge coloring changed");
    }
    let fresh_digest = informational_trial_prefix_fingerprint(
        trusted_chain,
        trusted_genesis,
        expected_tip,
        headers,
        &fresh,
    )?;
    if advertised_digest != fresh_digest {
        return Err("persisted digest is not the independently derived digest");
    }
    Ok(())
}

#[test]
fn restart_cache_revalidation_refuses_self_attested_consensus_metadata() {
    let (g, tip, headers) = chain(120);
    let exact = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&tip),
        &headers,
        32,
    )
    .unwrap();
    let digest = informational_trial_prefix_fingerprint(CHAIN, &g, &tip, &headers, &exact).unwrap();
    assert!(
        revalidate_untrusted_full_history_cache(CHAIN, &g, &tip, &headers, 32, &exact, &digest)
            .is_ok(),
        "full independent re-evaluation must accept its exact cache"
    );

    let mut tampered = exact.clone();
    tampered.blocks.get_mut(&tip).unwrap().blue_work.0[0] ^= 1;
    let forged_digest =
        informational_trial_prefix_fingerprint(CHAIN, &g, &tip, &headers, &tampered).unwrap();
    assert!(
        revalidate_untrusted_full_history_cache(
            CHAIN,
            &g,
            &tip,
            &headers,
            32,
            &tampered,
            &forged_digest
        )
        .is_err(),
        "self-consistent forged cache/checksum must NOT bypass recomputation"
    );
    let mut wrong_parent = exact.clone();
    wrong_parent.blocks.get_mut(&tip).unwrap().selected_parent = None;
    let parent_digest =
        informational_trial_prefix_fingerprint(CHAIN, &g, &tip, &headers, &wrong_parent).unwrap();
    assert!(revalidate_untrusted_full_history_cache(
        CHAIN,
        &g,
        &tip,
        &headers,
        32,
        &wrong_parent,
        &parent_digest
    )
    .is_err());
    assert!(revalidate_untrusted_full_history_cache(
        CHAIN,
        &g,
        &tip,
        &headers,
        32,
        &exact,
        &"00".repeat(32)
    )
    .is_err());
    assert!(
        revalidate_untrusted_full_history_cache(CHAIN, &g, &tip, &headers, 7, &exact, &digest)
            .is_err()
    );
    assert!(revalidate_untrusted_full_history_cache(
        "foreign-v3-network",
        &g,
        &tip,
        &headers,
        32,
        &exact,
        &digest
    )
    .is_err());

    let mut truncated = headers.clone();
    truncated.retain(|id, h| id == &g || h.height >= 60);
    assert!(
        revalidate_untrusted_full_history_cache(CHAIN, &g, &tip, &truncated, 32, &exact, &digest)
            .is_err(),
        "historical gaps remain FAIL-CLOSED, regardless of cached digest"
    );

    let mut corrupted = headers.clone();
    corrupted.get_mut(&tip).unwrap().nonce ^= 1;
    assert!(
        revalidate_untrusted_full_history_cache(CHAIN, &g, &tip, &corrupted, 32, &exact, &digest)
            .is_err(),
        "cached metadata cannot excuse changed block-hash bytes"
    );

    let mut extra = headers.clone();
    extra.insert(
        "untrusted-extra-not-on-selected-closure".into(),
        headers[&tip].clone(),
    );
    assert!(
        revalidate_untrusted_full_history_cache(CHAIN, &g, &tip, &extra, 32, &exact, &digest)
            .is_err(),
        "unverified extra entries must not be imported as consensus state"
    );

    // Multilateral DAG data has colored side branches: not just linear work.
    let (g2, tip2, multi) = eight_parent_fork(6);
    let multi_fresh = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &g2,
        std::slice::from_ref(&tip2),
        &multi,
        6,
    )
    .unwrap();
    let multi_hash =
        informational_trial_prefix_fingerprint(CHAIN, &g2, &tip2, &multi, &multi_fresh).unwrap();
    assert!(revalidate_untrusted_full_history_cache(
        CHAIN,
        &g2,
        &tip2,
        &multi,
        6,
        &multi_fresh,
        &multi_hash
    )
    .is_ok());
    let mut colors = multi_fresh.clone();
    let join = colors.blocks.get_mut(&tip2).unwrap();
    join.merge_blues.clear();
    let invented_hash =
        informational_trial_prefix_fingerprint(CHAIN, &g2, &tip2, &multi, &colors).unwrap();
    assert!(
        revalidate_untrusted_full_history_cache(
            CHAIN,
            &g2,
            &tip2,
            &multi,
            6,
            &colors,
            &invented_hash
        )
        .is_err(),
        "an invented blue/red list cannot pass even with a new digest"
    );
}

/// Non-normative witness *inventory* derived ONLY by complete authentic
/// frozen-genesis history replay. Not a proof of safe pruning/finality or
/// the normative retarget sample set. "20" is a diagnostic candidate window;
/// the actual v3 retarget samples remain unapproved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestOnlyResumeContextInventory {
    chain_id: String,
    genesis: BlockId,
    tip: BlockId,
    trial_k: usize,
    /// Last up-to-20 NON-GENESIS selected-parent IDs (tip-first).
    recent_selected_tip_first: Vec<BlockId>,
    /// Union of every blue/red merge-side header in the complete trial graph.
    colored_side_headers_sorted: Vec<BlockId>,
    /// Informational, self-attestable checksum, NEVER a trust anchor.
    complete_history_audit_hash: String,
}

fn recompute_test_only_resume_context(
    chain_id: &str,
    genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    k: usize,
) -> Result<TestOnlyResumeContextInventory, &'static str> {
    if chain_id != CHAIN {
        return Err("wrong externally frozen chain identity");
    }
    // Full-genesis PoW authentication remains MANDATORY; no pruned bypass.
    let full = derive_dormant_native_v3_ghostdag_trial(
        chain_id,
        GENESIS_NS,
        genesis,
        std::slice::from_ref(&tip.to_owned()),
        headers,
        k,
    )
    .map_err(|_| "full PoW DAG ancestry is missing or invalid")?;
    if full.blocks.len() != headers.len() {
        return Err("unreachable/extra cache headers cannot become authority");
    }
    if !full.blocks.contains_key(genesis) || !full.blocks.contains_key(tip) {
        return Err("tip or genesis omitted from fully verified history");
    }
    let mut current = tip.to_owned();
    let mut walked = BTreeSet::new();
    let mut recent = Vec::new();
    loop {
        if !walked.insert(current.clone()) || walked.len() > full.blocks.len() {
            return Err("selected spine cycle/length exceeds verified history");
        }
        let meta = full
            .blocks
            .get(&current)
            .ok_or("unknown selected ancestor")?;
        if current == genesis {
            if meta.selected_parent.is_some() {
                return Err("genesis cannot have a selected parent");
            }
            break;
        }
        if recent.len() < 20 {
            recent.push(current.clone());
        }
        current = meta
            .selected_parent
            .clone()
            .ok_or("non-genesis selected ancestor has no parent")?;
    }

    let mut colored_side_headers = BTreeSet::new();
    for meta in full.blocks.values() {
        for hash in meta.merge_blues.iter().chain(&meta.merge_reds) {
            if hash == genesis || !full.blocks.contains_key(hash) {
                return Err("side header outside full PoW-authenticated prefix");
            }
            colored_side_headers.insert(hash.clone());
        }
    }
    let audit = informational_trial_prefix_fingerprint(chain_id, genesis, tip, headers, &full)?;
    Ok(TestOnlyResumeContextInventory {
        chain_id: chain_id.to_owned(),
        genesis: genesis.to_owned(),
        tip: tip.to_owned(),
        trial_k: k,
        recent_selected_tip_first: recent,
        colored_side_headers_sorted: colored_side_headers.into_iter().collect(),
        complete_history_audit_hash: audit,
    })
}

/// The supplied inventory is UNTRUSTED and cannot attest itself. Re-derive
/// against all authenticated headers; do not authorize compact-prune from it.
fn verify_untrusted_resume_inventory_for_review(
    chain_id: &str,
    genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    k: usize,
    inventory: &TestOnlyResumeContextInventory,
) -> Result<(), &'static str> {
    let expected = recompute_test_only_resume_context(chain_id, genesis, tip, headers, k)?;
    if expected != *inventory {
        return Err("proposed context inventory differs from independently derived history");
    }
    Ok(())
}

#[test]
fn candidate_retention_inventory_revalidates_full_history_and_rejects_mutations() {
    // The candidate 20-recent-selected ancestor list is NOT the frozen
    // retarget sampling rule, nor the total required future DAG context.
    let (g, tip, headers) = chain(120);
    let full = recompute_test_only_resume_context(CHAIN, &g, &tip, &headers, 32).unwrap();
    assert_eq!(full.recent_selected_tip_first.len(), 20);
    assert_eq!(full.recent_selected_tip_first.first(), Some(&tip));
    assert!(full.colored_side_headers_sorted.is_empty());
    assert!(
        verify_untrusted_resume_inventory_for_review(CHAIN, &g, &tip, &headers, 32, &full).is_ok()
    );

    let mut forged = full.clone();
    forged.recent_selected_tip_first.pop();
    assert!(
        verify_untrusted_resume_inventory_for_review(CHAIN, &g, &tip, &headers, 32, &forged)
            .is_err()
    );
    let mut forged = full.clone();
    forged.complete_history_audit_hash = "ff".repeat(32);
    assert!(
        verify_untrusted_resume_inventory_for_review(CHAIN, &g, &tip, &headers, 32, &forged)
            .is_err()
    );
    let mut forged = full.clone();
    forged.trial_k = 7;
    assert!(
        verify_untrusted_resume_inventory_for_review(CHAIN, &g, &tip, &headers, 32, &forged)
            .is_err()
    );
    let mut truncated = headers.clone();
    truncated.retain(|id, h| id == &g || h.height >= 80);
    assert!(
        verify_untrusted_resume_inventory_for_review(CHAIN, &g, &tip, &truncated, 32, &full)
            .is_err(),
        "keeping the last 20 selected blocks is NOT sufficient proof to prune"
    );

    // Multiparent K6 creates 6 blue and 1 red side blocks; all seven
    // remain listed even though the selected-parent spine is only two deep.
    let (g2, tip2, multiparent) = eight_parent_fork(6);
    let inventory = recompute_test_only_resume_context(CHAIN, &g2, &tip2, &multiparent, 6).unwrap();
    assert_eq!(inventory.recent_selected_tip_first.len(), 2);
    assert_eq!(inventory.colored_side_headers_sorted.len(), 7);
    assert!(verify_untrusted_resume_inventory_for_review(
        CHAIN,
        &g2,
        &tip2,
        &multiparent,
        6,
        &inventory
    )
    .is_ok());

    let mut forged_colors = inventory.clone();
    forged_colors.colored_side_headers_sorted.pop();
    assert!(
        verify_untrusted_resume_inventory_for_review(
            CHAIN,
            &g2,
            &tip2,
            &multiparent,
            6,
            &forged_colors
        )
        .is_err(),
        "inventories omitting even one red/blue side header must fail"
    );
    let mut forged_identity = inventory.clone();
    forged_identity.chain_id = "other-network".into();
    assert!(verify_untrusted_resume_inventory_for_review(
        CHAIN,
        &g2,
        &tip2,
        &multiparent,
        6,
        &forged_identity
    )
    .is_err());
    let mut missing = multiparent.clone();
    let doomed_side = inventory.colored_side_headers_sorted[0].clone();
    missing.remove(&doomed_side);
    assert!(
        verify_untrusted_resume_inventory_for_review(CHAIN, &g2, &tip2, &missing, 6, &inventory)
            .is_err(),
        "verified DAG cannot skip one missing authentic colored side block"
    );
}


/// TEST ONLY: an untrusted cached selected-parent map MUST match a fresh
/// complete-genesis native GHOSTDAG trial before it is passed to the existing
/// v3 20-sample retarget adapter. That adapter independently checks hashes,
/// PoW and parent linkage but CANNOT by itself establish *which* parent wins.
/// This helper intentionally refuses pruned or >128-header input, and does
/// not produce a validated production difficulty/admission decision.
fn trial_retarget_window_rejecting_cached_selection(
    genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    k: usize,
    cached_selection: &BTreeMap<BlockId, Option<BlockId>>,
) -> Result<Vec<V3RetargetSample>, String> {
    let fresh = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        genesis,
        std::slice::from_ref(&tip.to_owned()),
        headers,
        k,
    )
    .map_err(|error| format!("full authenticated GHOSTDAG ancestry required: {error}"))?;
    if fresh.blocks.len() != headers.len() {
        return Err("unverified extra header entries in retarget context".into());
    }
    let derived: BTreeMap<BlockId, Option<BlockId>> = fresh
        .blocks
        .iter()
        .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
        .collect();
    if *cached_selection != derived {
        return Err("cached selected-parent metadata differs from full GHOSTDAG derivation".into());
    }
    selected_parent_retarget_window_v3_ns(CHAIN, &tip.to_owned(), headers, &derived)
        .map_err(|error| format!("retarget window rejected: {error}"))
}

#[test]
fn retarget_must_use_recomputed_genuine_kheavyhash_selected_parent_not_peer_cache() {
    // At 120 blocks, the actual adapter retains 20 newest-first samples,
    // while the frozen-genesis-window-only guard refuses this old boundary.
    // The *full-history* test-only candidate can derive the parent map,
    // but it is NOT an approved pruning/finality/checkpoint witness.
    let (genesis, tip, headers) = chain(120);
    let fresh = derive_dormant_native_v3_ghostdag_trial(
        CHAIN, GENESIS_NS, &genesis, std::slice::from_ref(&tip), &headers, 32
    )
    .unwrap();
    let selected: BTreeMap<BlockId, Option<BlockId>> = fresh
        .blocks
        .iter()
        .map(|(id, m)| (id.clone(), m.selected_parent.clone()))
        .collect();
    let window = trial_retarget_window_rejecting_cached_selection(
        &genesis, &tip, &headers, 32, &selected
    )
    .unwrap();
    assert_eq!(window.len(), 20);
    assert_eq!(window[0].timestamp_ns, GENESIS_NS + 120 * 500_000_000);
    assert_eq!(window[19].timestamp_ns, GENESIS_NS + 101 * 500_000_000);
    let decision = expected_difficulty_for_v3_window_ns(&window).unwrap();
    assert_eq!(decision.observed_intervals, 19);
    assert_eq!(decision.average_interval_ns, 500_000_000);
    assert!(
        verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, GENESIS_NS, &genesis, &tip, &headers, &selected
        )
        .is_err(),
        "the genesis-anchored limited adapter MUST reject a 120-deep window"
    );
    let mut cut = headers.clone();
    cut.retain(|id, h| id == &genesis || h.height >= 101);
    assert!(
        trial_retarget_window_rejecting_cached_selection(
            &genesis, &tip, &cut, 32, &selected
        )
        .is_err(),
        "having all twenty authentic retarget samples is not enough to accept a pruned prefix"
    );

    // Eight competing PoW-valid siblings share genesis; the join commits K6.
    // The generic retarget adapter alone can verify *either* listed sibling's
    // hash/PoW/linkage, but it cannot authenticate GHOSTDAG's actual winner.
    let (g2, join, fork_headers) = eight_parent_fork(6);
    let fork = derive_dormant_native_v3_ghostdag_trial(
        CHAIN, GENESIS_NS, &g2, std::slice::from_ref(&join), &fork_headers, 6
    )
    .unwrap();
    let derived_map: BTreeMap<BlockId, Option<BlockId>> = fork
        .blocks
        .iter()
        .map(|(id, meta)| (id.clone(), meta.selected_parent.clone()))
        .collect();
    let honest = trial_retarget_window_rejecting_cached_selection(
        &g2, &join, &fork_headers, 6, &derived_map
    )
    .unwrap();
    assert_eq!(honest.len(), 3);
    let expected = expected_difficulty_for_v3_window_ns(&honest).unwrap();
    assert_eq!(
        verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, GENESIS_NS, &g2, &join, &fork_headers, &derived_map
        )
        .unwrap(),
        expected
    );

    // The forged cached parent IS an actual PoW-valid direct parent. Even
    // that is insufficient: it did not win the authenticated candidate K6
    // GHOSTDAG selection. A malicious selection changes sample timestamps.
    let genuine_parent = derived_map[&join].clone().unwrap();
    let wrong_but_listed = fork_headers[&join]
        .parents
        .iter()
        .find(|parent| parent.as_str() != genuine_parent.as_str())
        .unwrap()
        .clone();
    let mut forged = derived_map.clone();
    forged.insert(join.clone(), Some(wrong_but_listed));
    let locally_well_formed = selected_parent_retarget_window_v3_ns(
        CHAIN, &join, &fork_headers, &forged
    )
    .expect("hash/PoW/linkage alone do not prove correct GHOSTDAG selection");
    assert_ne!(honest, locally_well_formed);
    assert!(
        trial_retarget_window_rejecting_cached_selection(
            &g2, &join, &fork_headers, 6, &forged
        )
        .is_err(),
        "re-deriving full native GHOSTDAG must reject a plausible forged winner"
    );
    assert!(
        trial_retarget_window_rejecting_cached_selection(
            &g2, &join, &fork_headers, 7, &derived_map
        )
        .is_err(),
        "a K6 block cannot silently inherit a different trial policy's selection"
    );
    let mut no_side = fork_headers.clone();
    no_side.remove(&genuine_parent);
    assert!(
        trial_retarget_window_rejecting_cached_selection(
            &g2, &join, &no_side, 6, &derived_map
        )
        .is_err(),
        "valid 20-sample retarget cannot excuse missing fork-side full ancestry"
    );
}
