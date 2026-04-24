use anyhow::{anyhow, Result};
use pulsedag_core::pow::{
    dev_pow_accepts, dev_surrogate_pow_hash, pow_hash_score_u64, pow_preimage_bytes,
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

pub fn miner_pow_preimage_bytes(header: &BlockHeader) -> Vec<u8> {
    pow_preimage_bytes(header)
}

pub fn miner_pow_hash_hex(header: &BlockHeader) -> String {
    dev_surrogate_pow_hash(header)
}

pub fn miner_pow_score_u64(header: &BlockHeader) -> u64 {
    pow_hash_score_u64(header)
}

pub fn miner_pow_accepts(header: &BlockHeader) -> bool {
    dev_pow_accepts(header)
}

pub fn mine_header_strided(
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
) -> Result<NonceSearchResult> {
    let max_tries = max_tries.max(1);
    let effective_threads = threads.max(1).min(max_tries as usize);
    let found = Arc::new(AtomicBool::new(false));
    let tries = Arc::new(AtomicU64::new(0));
    let winner: Arc<Mutex<Option<(BlockHeader, String)>>> = Arc::new(Mutex::new(None));
    let mut handles = Vec::with_capacity(effective_threads);

    for tid in 0..effective_threads {
        let found = Arc::clone(&found);
        let tries = Arc::clone(&tries);
        let winner = Arc::clone(&winner);
        let thread_header = header.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut local_tries = 0u64;
            let mut nonce = tid as u64;

            while nonce < max_tries {
                if found.load(Ordering::Relaxed) {
                    break;
                }

                let mut candidate = thread_header.clone();
                candidate.nonce = nonce;
                local_tries = local_tries.saturating_add(1);

                let hash_hex = dev_surrogate_pow_hash(&candidate);
                if dev_pow_accepts(&candidate) {
                    let already_found = found.swap(true, Ordering::SeqCst);
                    if !already_found {
                        let mut guard = winner.lock().map_err(|_| {
                            anyhow!("winner mutex poisoned during candidate selection")
                        })?;
                        *guard = Some((candidate, hash_hex));
                    }
                    break;
                }

                nonce = nonce.saturating_add(effective_threads as u64);
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
    let fallback_hash = dev_surrogate_pow_hash(&fallback_header);
    Ok(NonceSearchResult {
        header: fallback_header,
        accepted: false,
        tries: total_tries.max(1),
        final_hash_hex: fallback_hash,
    })
}
