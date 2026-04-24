use pulsedag_core::types::BlockHeader;
use pulsedag_miner::mine_header_strided;
use std::time::Instant;

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
#[ignore = "Run manually for operator baseline capture"]
fn pow_thread_scaling_baseline() {
    let max_tries = 2_000_000u64;
    let difficulty = u32::MAX;

    println!(
        "pow baseline config: max_tries={} difficulty={} accepted_expected=false",
        max_tries, difficulty
    );

    for threads in [1usize, 2usize, 4usize, 8usize] {
        let start = Instant::now();
        let result = mine_header_strided(fixture_header(difficulty), max_tries, threads)
            .expect("threaded nonce search should complete");
        let elapsed = start.elapsed();
        let seconds = elapsed.as_secs_f64();
        let hps = result.tries as f64 / seconds;

        println!(
            "threads={threads} tries={} accepted={} elapsed_s={:.3} hps={:.0}",
            result.tries, result.accepted, seconds, hps
        );

        assert_eq!(result.tries, max_tries);
        assert!(!result.accepted);
    }
}
