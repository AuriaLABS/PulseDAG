use anyhow::{anyhow, Result};
use pulsedag_core::pow::{
    canonical_pow_adapter, pow_accepts, pow_hash_score_u64, pow_preimage_bytes,
};
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

/// Internal abstraction for mining backends.
///
/// The default implementation is [`CpuMiningBackend`], which intentionally
/// delegates to the existing strided CPU mining path so current miner behavior
/// stays unchanged while leaving room for future backends.
pub trait MiningBackend: Send + Sync {
    fn name(&self) -> &'static str;

    fn mine_header(
        &self,
        header: BlockHeader,
        max_tries: u64,
        threads: usize,
        target_bits: u32,
    ) -> Result<NonceSearchResult>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuMiningBackend;

impl MiningBackend for CpuMiningBackend {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn mine_header(
        &self,
        header: BlockHeader,
        max_tries: u64,
        threads: usize,
        target_bits: u32,
    ) -> Result<NonceSearchResult> {
        mine_header_strided(header, max_tries, threads, target_bits)
    }
}

pub fn miner_pow_preimage_bytes(header: &BlockHeader) -> Vec<u8> {
    pow_preimage_bytes(header)
}

pub fn miner_pow_hash_hex(header: &BlockHeader) -> String {
    canonical_pow_adapter()
        .evaluate_header(header)
        .map(|attempt| attempt.final_hash.hash_hex)
        .unwrap_or_default()
}

pub fn miner_pow_score_u64(header: &BlockHeader) -> u64 {
    pow_hash_score_u64(header)
}

pub fn miner_pow_accepts(header: &BlockHeader) -> bool {
    pow_accepts(header)
}

fn miner_pow_eval_at_target_bits(header: &BlockHeader, target_bits: u32) -> Result<(bool, String)> {
    if target_bits == 0 {
        return Err(anyhow!("invalid target bits: 0"));
    }
    let mut h = header.clone();
    h.difficulty = target_bits;
    let attempt = canonical_pow_adapter()
        .evaluate_header(&h)
        .map_err(|reason| anyhow!("invalid PoW header material: {}", reason.code()))?;
    Ok((attempt.comparison.accepted(), attempt.final_hash.hash_hex))
}

pub fn miner_pow_accepts_target_bits(header: &BlockHeader, target_bits: u32) -> Result<bool> {
    Ok(miner_pow_eval_at_target_bits(header, target_bits)?.0)
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

                let (accepted, hash_hex) = miner_pow_eval_at_target_bits(&candidate, target_bits)?;
                if accepted {
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
    let (_, fallback_hash) = miner_pow_eval_at_target_bits(&fallback_header, target_bits)?;
    Ok(NonceSearchResult {
        header: fallback_header,
        accepted: false,
        tries: total_tries.max(1),
        final_hash_hex: fallback_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        miner_pow_accepts, miner_pow_hash_hex, miner_pow_preimage_bytes, miner_pow_score_u64,
        nonce_for_attempt, CpuMiningBackend, MiningBackend,
    };
    use pulsedag_core::pow::{
        canonical_pow_adapter, canonical_pow_engine, target_from_bits, target_hex, PowEngine,
    };
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
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 7,
            difficulty: 0x1f00ffff,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        assert_eq!(
            miner_pow_hash_hex(&header),
            canonical_pow_engine().evaluate_header(&header).hash_hex
        );
        assert!(!miner_pow_preimage_bytes(&header).is_empty());
    }

    #[test]
    fn cpu_miner_and_canonical_adapter_evaluate_same_nonce() {
        let header = BlockHeader {
            version: 1,
            parents: vec!["b".into(), "a".into()],
            timestamp: 2,
            nonce: 11,
            difficulty: 0x207fffff,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 2,
            height: 2,
        };
        let adapter = canonical_pow_adapter();
        let attempt = adapter.evaluate_header(&header).expect("adapter attempt");

        assert_eq!(miner_pow_hash_hex(&header), attempt.final_hash.hash_hex);
        assert_eq!(miner_pow_score_u64(&header), attempt.final_hash.score_u64);
        assert_eq!(miner_pow_accepts(&header), attempt.comparison.accepted());
        assert_eq!(
            miner_pow_preimage_bytes(&header),
            attempt.material.pre_pow_bytes
        );
    }

    #[test]
    fn easy_target_finds_solution() {
        let target_bits = 0x207fffff;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let mined = super::mine_header_strided(header, 10_000, 4, target_bits)
            .expect("mining should succeed");
        assert!(mined.accepted);
    }

    #[test]
    fn cpu_backend_uses_strided_cpu_path() {
        let target_bits = 1;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let backend = CpuMiningBackend;

        let via_backend = backend
            .mine_header(header.clone(), 16, 1, target_bits)
            .expect("backend mining should run");
        let direct = super::mine_header_strided(header, 16, 1, target_bits)
            .expect("direct CPU mining should run");

        assert_eq!(backend.name(), "cpu");
        assert_eq!(via_backend.accepted, direct.accepted);
        assert_eq!(via_backend.tries, direct.tries);
        assert_eq!(via_backend.header.nonce, direct.header.nonce);
        assert_eq!(via_backend.final_hash_hex, direct.final_hash_hex);
    }

    #[test]
    fn cpu_backend_preserves_max_tries_floor() {
        let target_bits = 1;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };

        let mined = CpuMiningBackend
            .mine_header(header, 0, 4, target_bits)
            .expect("backend should normalize max_tries like CPU path");

        assert_eq!(mined.tries, 1);
        assert_eq!(mined.header.nonce, 0);
    }

    #[test]
    fn invalid_target_fails_cleanly() {
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: 1,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let err = super::miner_pow_accepts_target_bits(&header, 0).expect_err("must fail");
        assert!(err.to_string().contains("invalid target bits"));
        let _ = target_hex(&target_from_bits(0x1d00ffff));
    }
}
