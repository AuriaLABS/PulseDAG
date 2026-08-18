use crate::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use crate::{BackendVerification, CpuMiningBackend, NonceSearchResult};
use anyhow::{anyhow, Result};
use pulsedag_core::types::BlockHeader;
use pulsedag_core::ProtocolActivationIdentity;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub trait ProtocolMiningBackend: Send + Sync {
    fn mine_header_for_protocol(
        &self,
        header: BlockHeader,
        max_tries: u64,
        threads: usize,
        target_bits: u32,
        identity: &ProtocolActivationIdentity,
    ) -> Result<NonceSearchResult>;
}

impl ProtocolMiningBackend for CpuMiningBackend {
    fn mine_header_for_protocol(
        &self,
        header: BlockHeader,
        max_tries: u64,
        threads: usize,
        target_bits: u32,
        identity: &ProtocolActivationIdentity,
    ) -> Result<NonceSearchResult> {
        mine_header_strided_for_protocol(header, max_tries, threads, target_bits, identity)
    }
}

#[cfg(feature = "gpu")]
impl ProtocolMiningBackend for crate::GpuMiningBackend {
    fn mine_header_for_protocol(
        &self,
        header: BlockHeader,
        _max_tries: u64,
        _threads: usize,
        target_bits: u32,
        identity: &ProtocolActivationIdentity,
    ) -> Result<NonceSearchResult> {
        let work = build_protocol_pow_work(&header, target_bits, Some(identity))?;
        Err(anyhow!(
            "OpenCL GPU backend selected platform[{}]={} device[{}]={}, but canonical kHeavyHash OpenCL mining is not implemented yet; refusing to mine with a non-canonical kernel. protocol_path={} canonical_pre_pow_bytes={} target_hex={} batch_size={} work_size={}. Use --backend cpu to mine on the CPU.",
            self.selected_device.platform_index,
            self.selected_device.platform_name,
            self.selected_device.device_index,
            self.selected_device.device_name,
            work.path.as_str(),
            work.material.pre_pow_bytes.len(),
            work.material.target.target_hex,
            self.config.batch_size,
            self.config.work_size,
        ))
    }
}

pub fn verify_backend_result_for_protocol(
    header: &BlockHeader,
    target_bits: u32,
    identity: &ProtocolActivationIdentity,
) -> Result<BackendVerification> {
    let work = build_protocol_pow_work(header, target_bits, Some(identity))?;
    let attempt = work.evaluate_nonce(header.nonce);
    Ok(BackendVerification {
        accepted: attempt.comparison.accepted(),
        final_hash_hex: attempt.final_hash.hash_hex,
        target_hex: attempt.material.target.target_hex,
    })
}

pub fn verify_backend_search_result_for_protocol(
    result: &NonceSearchResult,
    target_bits: u32,
    identity: &ProtocolActivationIdentity,
) -> Result<BackendVerification> {
    verify_backend_result_for_protocol(&result.header, target_bits, identity)
}

fn nonce_for_protocol_attempt(thread_id: usize, stride: usize, iteration: u64) -> u64 {
    thread_id as u64 + (stride as u64 * iteration)
}

fn evaluate_candidate(
    work: &ProtocolPowWork,
    header: &BlockHeader,
    nonce: u64,
) -> (BlockHeader, bool, String) {
    let mut candidate = header.clone();
    candidate.nonce = nonce;
    let attempt = work.evaluate_nonce(nonce);
    (
        candidate,
        attempt.comparison.accepted(),
        attempt.final_hash.hash_hex,
    )
}

pub fn mine_header_strided_for_protocol(
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
    target_bits: u32,
    identity: &ProtocolActivationIdentity,
) -> Result<NonceSearchResult> {
    let work = Arc::new(build_protocol_pow_work(
        &header,
        target_bits,
        Some(identity),
    )?);
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
        let work = Arc::clone(&work);
        let thread_header = header.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut local_tries = 0u64;
            let mut iteration = 0u64;

            loop {
                let nonce = nonce_for_protocol_attempt(thread_id, effective_threads, iteration);
                if nonce >= max_tries || found.load(Ordering::Relaxed) {
                    break;
                }

                let (candidate, accepted, hash_hex) =
                    evaluate_candidate(&work, &thread_header, nonce);
                local_tries = local_tries.saturating_add(1);

                if accepted {
                    let already_found = found.swap(true, Ordering::SeqCst);
                    if !already_found {
                        let mut guard = winner.lock().map_err(|_| {
                            anyhow!("winner mutex poisoned during protocol candidate selection")
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
            .map_err(|_| anyhow!("a protocol mining thread panicked during execution"))?;
        thread_result?;
    }

    let total_tries = tries.load(Ordering::Relaxed).min(max_tries);
    let winner_candidate = winner
        .lock()
        .map_err(|_| anyhow!("winner mutex poisoned when finalizing protocol result"))?
        .clone();
    if let Some((winner_header, winner_hash)) = winner_candidate {
        return Ok(NonceSearchResult {
            header: winner_header,
            accepted: true,
            tries: total_tries,
            final_hash_hex: winner_hash,
        });
    }

    let fallback_nonce = max_tries.saturating_sub(1);
    let (fallback_header, _, fallback_hash) = evaluate_candidate(&work, &header, fallback_nonce);
    Ok(NonceSearchResult {
        header: fallback_header,
        accepted: false,
        tries: total_tries.max(1),
        final_hash_hex: fallback_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mine_header_strided, verify_backend_result_with_core};
    use pulsedag_core::{
        canonical_pow_v2_adapter, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V1,
        BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION,
    };

    fn header(version: u32, target_bits: u32) -> BlockHeader {
        BlockHeader {
            version,
            parents: vec!["11".repeat(32)],
            timestamp: 1_700_000_000,
            difficulty: target_bits,
            nonce: 0,
            merkle_root: "33".repeat(32),
            state_root: "44".repeat(32),
            blue_score: 1,
            height: 2,
        }
    }

    fn legacy_identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::legacy_default_for_chain("pulsedag-testnet", "55".repeat(32))
    }

    fn activated_identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            chain_id,
            "55".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    #[test]
    fn legacy_protocol_search_matches_existing_single_worker_path() {
        let target_bits = 0x0100_0001;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let legacy = legacy_identity();

        let protocol =
            mine_header_strided_for_protocol(header.clone(), 16, 1, target_bits, &legacy).unwrap();
        let historical = mine_header_strided(header, 16, 1, target_bits).unwrap();

        assert_eq!(protocol.accepted, historical.accepted);
        assert_eq!(protocol.tries, historical.tries);
        assert_eq!(protocol.header.nonce, historical.header.nonce);
        assert_eq!(protocol.final_hash_hex, historical.final_hash_hex);
    }

    #[test]
    fn activated_v2_search_uses_chain_bound_work_for_every_nonce() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
        let identity = activated_identity("pulsedag-testnet-v2");

        let mined =
            mine_header_strided_for_protocol(header, 10_000, 4, target_bits, &identity).unwrap();
        let expected = canonical_pow_v2_adapter()
            .evaluate_header(&mined.header, &identity.chain_id)
            .unwrap();

        assert!(mined.accepted);
        assert!(expected.comparison.accepted());
        assert_eq!(mined.final_hash_hex, expected.final_hash.hash_hex);
    }

    #[test]
    fn protocol_verification_matches_search_domain() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
        let identity = activated_identity("pulsedag-testnet-v2");
        let mined = CpuMiningBackend
            .mine_header_for_protocol(header, 10_000, 2, target_bits, &identity)
            .unwrap();
        let verification =
            verify_backend_result_for_protocol(&mined.header, target_bits, &identity).unwrap();

        assert!(mined.accepted);
        assert!(verification.accepted);
        assert_eq!(verification.final_hash_hex, mined.final_hash_hex);
    }

    #[test]
    fn wrong_chain_changes_v2_verification_hash() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
        let testnet = activated_identity("pulsedag-testnet-v2");
        let private = activated_identity("pulsedag-private-v2");

        let testnet_verification =
            verify_backend_result_for_protocol(&header, target_bits, &testnet).unwrap();
        let private_verification =
            verify_backend_result_for_protocol(&header, target_bits, &private).unwrap();

        assert_ne!(
            testnet_verification.final_hash_hex,
            private_verification.final_hash_hex
        );
    }

    #[test]
    fn mixed_identity_and_header_fail_before_search() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
        let mut identity = activated_identity("pulsedag-testnet-v2");
        identity.consensus_mode = ProtocolConsensusMode::Legacy;

        assert!(mine_header_strided_for_protocol(header, 1, 1, target_bits, &identity).is_err());
    }

    #[test]
    fn legacy_protocol_verification_matches_existing_core_gate() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let legacy = legacy_identity();
        let protocol = verify_backend_result_for_protocol(&header, target_bits, &legacy).unwrap();
        let historical = verify_backend_result_with_core(&header, target_bits).unwrap();

        assert_eq!(protocol, historical);
    }

    #[test]
    fn backend_search_result_is_reverified_in_protocol_domain() {
        let target_bits = 0x0100_0001;
        let identity = activated_identity("pulsedag-testnet-v2");
        let fake = NonceSearchResult {
            header: header(BLOCK_HEADER_VERSION_V2, target_bits),
            accepted: true,
            tries: 1,
            final_hash_hex: "fake".to_string(),
        };

        let verification =
            verify_backend_search_result_for_protocol(&fake, target_bits, &identity).unwrap();
        assert!(!verification.accepted);
        assert_ne!(verification.final_hash_hex, fake.final_hash_hex);
    }
}
