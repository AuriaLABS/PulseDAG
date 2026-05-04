use anyhow::{anyhow, Result};
use pulsedag_core::pow::{canonical_pow_engine, pow_preimage_bytes, pow_accepts, pow_hash_score_u64, PowEngine};
use pulsedag_core::types::BlockHeader;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct NonceSearchResult {
    pub header: BlockHeader,
    pub accepted: bool,
    pub tries: u64,
    pub final_hash_hex: String,
}

pub fn miner_pow_preimage_bytes(header: &BlockHeader) -> Vec<u8> {
    pow_preimage_bytes(header)
}

pub fn miner_pow_hash_hex(header: &BlockHeader) -> String {
    canonical_pow_engine().evaluate_header(header).hash_hex
}


pub fn miner_pow_score_u64(header: &BlockHeader) -> u64 {
    pow_hash_score_u64(header)
}

pub fn miner_pow_accepts(header: &BlockHeader) -> bool {
    pow_accepts(header)
}

pub fn miner_pow_accepts_target_bits(header: &BlockHeader, target_bits: u32) -> Result<bool> {
    if target_bits == 0 {
        return Err(anyhow!("invalid target bits: 0"));
    }
    let mut h = header.clone();
    h.difficulty = target_bits;
    let eval = canonical_pow_engine().evaluate_header(&h);
    Ok(eval.accepted)
}

fn nonce_for_attempt(thread_id: usize, stride: usize, iteration: u64) -> u64 {
    thread_id as u64 + (stride as u64 * iteration)
}

pub fn mine_header_strided(
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
    target_bits: u32,
) -> Result<NonceSearchResult> {
    let max_tries = max_tries.max(1);
    let effective_threads = threads.max(1).min(max_tries as usize);
    let found = Arc::new(AtomicBool::new(false));
    let tries = Arc::new(AtomicU64::new(0));
    let winner: Arc<Mutex<Option<(BlockHeader, String)>>> = Arc::new(Mutex::new(None));
    let mut handles = Vec::with_capacity(effective_threads);

    for thread_id in 0..effective_threads {
        let found = Arc::clone(&found);
        let tries = Arc::clone(&tries);
        let winner = Arc::clone(&winner);
        let thread_header = header.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut local_tries = 0u64;
            let mut iteration = 0u64;

            loop {
                let nonce = nonce_for_attempt(thread_id, effective_threads, iteration);
                if nonce >= max_tries {
                    break;
                }

                if found.load(Ordering::Relaxed) {
                    break;
                }

                let mut candidate = thread_header.clone();
                candidate.nonce = nonce;
                local_tries = local_tries.saturating_add(1);

                let hash_hex = miner_pow_hash_hex(&candidate);
                if miner_pow_accepts_target_bits(&candidate, target_bits)? {
                    let already_found = found.swap(true, Ordering::SeqCst);
                    if !already_found {
                        let mut guard = winner.lock().map_err(|_| {
                            anyhow!("winner mutex poisoned during candidate selection")
                        })?;
                        *guard = Some((candidate, hash_hex));
                    }
                    break;
                }

                iteration = iteration.saturating_add(1);
            }

            tries.fetch_add(local_tries, Ordering::Relaxed);
            Ok(())
        });
        handles.push(handle);
    }

    for handle in handles {
        let thread_result = handle
            .join()
            .map_err(|_| anyhow!("a mining thread panicked during execution"))?;
        thread_result?;
    }

    let total_tries = tries.load(Ordering::Relaxed).min(max_tries);
    let winner_candidate = winner
        .lock()
        .map_err(|_| anyhow!("winner mutex poisoned when finalizing result"))?
        .clone();
    if let Some((winner_header, winner_hash)) = winner_candidate {
        return Ok(NonceSearchResult {
            header: winner_header,
            accepted: true,
            tries: total_tries,
            final_hash_hex: winner_hash,
        });
    }

    let mut fallback_header = header;
    fallback_header.nonce = max_tries.saturating_sub(1);
    let fallback_hash = miner_pow_hash_hex(&fallback_header);
    Ok(NonceSearchResult {
        header: fallback_header,
        accepted: false,
        tries: total_tries.max(1),
        final_hash_hex: fallback_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::{miner_pow_hash_hex, miner_pow_preimage_bytes, nonce_for_attempt};
    use pulsedag_core::pow::{canonical_pow_engine, target_from_bits, target_hex, PowEngine};
    use pulsedag_core::types::BlockHeader;

    #[test]
    fn worker_partitioning_is_non_overlapping_for_prefix_space() {
        let threads = 6usize;
        let samples_per_thread = 30u64;
        let mut seen = std::collections::BTreeSet::new();

        for tid in 0..threads {
            for i in 0..samples_per_thread {
                let n = nonce_for_attempt(tid, threads, i);
                assert!(seen.insert(n), "duplicate nonce generated in schedule: {n}");
            }
        }

        assert_eq!(seen.len(), (threads as u64 * samples_per_thread) as usize);
    }

    #[test]
    fn strided_schedule_is_deterministic_per_worker() {
        let threads = 4usize;
        let worker_two: Vec<u64> = (0..8).map(|i| nonce_for_attempt(2, threads, i)).collect();
        assert_eq!(worker_two, vec![2, 6, 10, 14, 18, 22, 26, 30]);
    }

    #[test]
    fn miner_and_core_compute_same_hash() {
        let header = BlockHeader { version: 1, parents: vec!["a".into()], timestamp: 1, nonce: 7, difficulty: 0x1f00ffff, merkle_root: "m".into(), state_root: "s".into(), blue_score: 1, height: 1 };
        assert_eq!(miner_pow_hash_hex(&header), canonical_pow_engine().evaluate_header(&header).hash_hex);
        assert!(!miner_pow_preimage_bytes(&header).is_empty());
    }

    #[test]
    fn easy_target_finds_solution() {
        let target_bits = 0x207fffff;
        let header = BlockHeader { version: 1, parents: vec!["a".into()], timestamp: 1, nonce: 0, difficulty: target_bits, merkle_root: "m".into(), state_root: "s".into(), blue_score: 1, height: 1 };
        let mined = super::mine_header_strided(header, 10_000, 4, target_bits).expect("mining should succeed");
        assert!(mined.accepted);
    }

    #[test]
    fn invalid_target_fails_cleanly() {
        let header = BlockHeader { version: 1, parents: vec!["a".into()], timestamp: 1, nonce: 0, difficulty: 1, merkle_root: "m".into(), state_root: "s".into(), blue_score: 1, height: 1 };
        let err = super::miner_pow_accepts_target_bits(&header, 0).expect_err("must fail");
        assert!(err.to_string().contains("invalid target bits"));
        let _ = target_hex(&target_from_bits(0x1d00ffff));
    }
}
