//! Non-normative native-v3 GHOSTDAG *local computation* profiling.
//!
//! Real PoW-valid KHeavyHash headers; 500ms intervals are GENERATED, not
//! peer-to-peer propagation measurements. Prints wall-clock microseconds and
//! Linux-process high-water RSS, WITHOUT enforcing noisy CI speed thresholds.
//! Diagnostic ancestry is capped at 128 headers. This suite never connects to
//! consensus admission, mining runtime, monetary rewards, UTXO or contracts.

use std::{collections::BTreeMap, time::Instant};

use sha2::{Digest, Sha256};

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial, CandidateGhostdagEvaluation,
    },
    pow_v3::evaluate_v3_header_pow,
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
