use std::process::Command;

use pulsedag_core::types::BlockHeader;
use pulsedag_miner::mine_header_strided;

fn fixture_header(difficulty: u32) -> BlockHeader {
    BlockHeader {
        version: 1,
        parents: vec![
            "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
        ],
        timestamp: 1_713_370_000,
        difficulty,
        nonce: 0,
        merkle_root: "4e3c14f9f6f42753fe4f2ddc2b53bfcbf065a90f8f919f94f0f7a87f6ecf7cf9".to_string(),
        state_root: "8f8793fa75efec8df5a0013d70e30e954f05064fa5f0db3ed8cb563127f90c0d".to_string(),
        blue_score: 1_024,
        height: 1_024,
    }
}

#[test]
fn multithread_fallback_is_repeatable_for_benchmark_runs() {
    let max_tries = 50_000;
    let difficulty = u32::MAX;

    let r1 = mine_header_strided(fixture_header(difficulty), max_tries, 8).expect("first run");
    let r2 = mine_header_strided(fixture_header(difficulty), max_tries, 8).expect("second run");

    assert!(!r1.accepted);
    assert!(!r2.accepted);
    assert_eq!(r1.tries, max_tries);
    assert_eq!(r2.tries, max_tries);
    assert_eq!(r1.header.nonce, max_tries - 1);
    assert_eq!(r2.header.nonce, max_tries - 1);
    assert_eq!(r1.final_hash_hex, r2.final_hash_hex);
}

#[test]
fn miner_startup_usage_still_works_without_required_address() {
    let bin = env!("CARGO_BIN_EXE_pulsedag-miner");
    let output = Command::new(bin)
        .output()
        .expect("binary should execute for startup smoke");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: pulsedag-miner --miner-address"));
}
