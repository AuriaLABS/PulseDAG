//! Non-normative native-v3 GHOSTDAG *local computation* profiling.
//!
//! Real PoW-valid KHeavyHash headers; 500ms intervals are GENERATED, not
//! peer-to-peer propagation measurements. Prints wall-clock microseconds and
//! Linux-process high-water RSS, WITHOUT enforcing noisy CI speed thresholds.
//! Diagnostic ancestry is capped at 128 headers. This suite never connects to
//! consensus admission, mining runtime, monetary rewards, UTXO or contracts.

use std::{collections::BTreeMap, time::Instant};

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3, types::BlockId,
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
