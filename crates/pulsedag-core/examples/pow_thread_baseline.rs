use pulsedag_core::pow::pow_hash_score_u64;
use pulsedag_core::types::BlockHeader;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn fixture_header() -> BlockHeader {
    BlockHeader {
        version: 1,
        parents: vec![
            "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
        ],
        timestamp: 1_713_370_000,
        difficulty: u32::MAX,
        nonce: 0,
        merkle_root: "4e3c14f9f6f42753fe4f2ddc2b53bfcbf065a90f8f919f94f0f7a87f6ecf7cf9".to_string(),
        state_root: "8f8793fa75efec8df5a0013d70e30e954f05064fa5f0db3ed8cb563127f90c0d".to_string(),
        blue_score: 1_024,
        height: 1_024,
    }
}

fn run_hash_sweep(threads: usize, total_hashes: u64) -> (f64, u64) {
    let start = Instant::now();
    let threads = threads.max(1);
    let per_thread = total_hashes / threads as u64;
    let remainder = total_hashes % threads as u64;
    let header = Arc::new(fixture_header());

    let handles: Vec<_> = (0..threads)
        .map(|tid| {
            let template = Arc::clone(&header);
            thread::spawn(move || {
                let local_count = per_thread + if tid == 0 { remainder } else { 0 };
                let mut checksum = 0u64;
                for i in 0..local_count {
                    let mut candidate = (*template).clone();
                    candidate.nonce = i * threads as u64 + tid as u64;
                    checksum ^= pow_hash_score_u64(&candidate);
                }
                checksum
            })
        })
        .collect();

    let mut checksum = 0u64;
    for handle in handles {
        checksum ^= handle.join().expect("pow hashing thread should not panic");
    }

    let elapsed_secs = start.elapsed().as_secs_f64();
    let hps = total_hashes as f64 / elapsed_secs;
    (hps, checksum)
}

fn main() {
    let total_hashes = std::env::var("PULSEDAG_BENCH_TOTAL_HASHES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2_000_000);

    println!(
        "pow-thread-baseline total_hashes={} difficulty=u32::MAX",
        total_hashes
    );

    for threads in [1usize, 2usize, 4usize, 8usize] {
        let (hps, checksum) = run_hash_sweep(threads, total_hashes);
        println!("threads={threads} hps={hps:.0} checksum={checksum}");
    }
}
