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

/// One deterministic lane in the protocol miner nonce space.
///
/// The lane enumerates candidates using the backend-neutral mapping
/// `nonce = lane + stride * iteration`. `max_tries` is an exclusive upper
/// bound, so every emitted nonce is in `0..max_tries`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolNoncePartition {
    lane: u64,
    stride: u64,
    max_tries: u64,
}

impl ProtocolNoncePartition {
    pub fn new(lane: u64, stride: u64, max_tries: u64) -> Result<Self> {
        let max_tries = max_tries.max(1);
        if stride == 0 {
            return Err(anyhow!("protocol nonce partition stride must be non-zero"));
        }
        if lane >= stride {
            return Err(anyhow!(
                "protocol nonce partition lane {lane} must be smaller than stride {stride}"
            ));
        }
        if lane >= max_tries {
            return Err(anyhow!(
                "protocol nonce partition lane {lane} is outside max_tries {max_tries}"
            ));
        }

        Ok(Self {
            lane,
            stride,
            max_tries,
        })
    }

    pub fn lane(self) -> u64 {
        self.lane
    }

    pub fn stride(self) -> u64 {
        self.stride
    }

    pub fn max_tries(self) -> u64 {
        self.max_tries
    }

    /// Return the nonce for this lane/iteration, or `None` once the lane has
    /// exhausted the exclusive `max_tries` bound. Arithmetic overflow is an
    /// error so accelerator backends can never wrap into duplicate work.
    pub fn nonce_at(self, iteration: u64) -> Result<Option<u64>> {
        let offset = self.stride.checked_mul(iteration).ok_or_else(|| {
            anyhow!(
                "protocol nonce partition overflow: stride {} * iteration {}",
                self.stride,
                iteration
            )
        })?;
        let nonce = self.lane.checked_add(offset).ok_or_else(|| {
            anyhow!(
                "protocol nonce partition overflow: lane {} + offset {}",
                self.lane,
                offset
            )
        })?;

        Ok((nonce < self.max_tries).then_some(nonce))
    }
}

/// Normalize a requested lane count without a lossy `u64 -> usize` cast.
///
/// `max_tries == 0` preserves the miner's historical behavior by normalizing
/// the search domain to one try. The returned lane count is always at least
/// one and never exceeds the number of candidate nonces.
pub fn protocol_nonce_lane_count(max_tries: u64, requested_lanes: usize) -> usize {
    let max_tries = max_tries.max(1);
    let requested_lanes = requested_lanes.max(1);

    match usize::try_from(max_tries) {
        Ok(max_lanes) => requested_lanes.min(max_lanes),
        Err(_) => requested_lanes,
    }
}

/// Build one active lane after applying the same normalization used by the CPU
/// protocol miner. Future GPU workers can reuse this exact contract.
pub fn protocol_nonce_partition(
    max_tries: u64,
    requested_lanes: usize,
    lane: usize,
) -> Result<ProtocolNoncePartition> {
    let max_tries = max_tries.max(1);
    let active_lanes = protocol_nonce_lane_count(max_tries, requested_lanes);
    if lane >= active_lanes {
        return Err(anyhow!(
            "protocol nonce lane {lane} is outside active lane count {active_lanes}"
        ));
    }

    let lane = u64::try_from(lane)
        .map_err(|_| anyhow!("protocol nonce lane index does not fit in u64"))?;
    let stride = u64::try_from(active_lanes)
        .map_err(|_| anyhow!("protocol nonce lane count does not fit in u64"))?;

    ProtocolNoncePartition::new(lane, stride, max_tries)
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
    let effective_threads = protocol_nonce_lane_count(max_tries, threads);
    let found = Arc::new(AtomicBool::new(false));
    let tries = Arc::new(AtomicU64::new(0));
    let winner: Arc<Mutex<Option<(BlockHeader, String)>>> = Arc::new(Mutex::new(None));
    let mut handles = Vec::with_capacity(effective_threads);

    for thread_id in 0..effective_threads {
        let partition = protocol_nonce_partition(max_tries, effective_threads, thread_id)?;
        let found = Arc::clone(&found);
        let tries = Arc::clone(&tries);
        let winner = Arc::clone(&winner);
        let work = Arc::clone(&work);
        let thread_header = header.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut local_tries = 0u64;
            let mut iteration = 0u64;

            loop {
                if found.load(Ordering::Relaxed) {
                    break;
                }

                let Some(nonce) = partition.nonce_at(iteration)? else {
                    break;
                };

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

                iteration = iteration.checked_add(1).ok_or_else(|| {
                    anyhow!(
                        "protocol nonce partition iteration overflow for lane {}",
                        partition.lane()
                    )
                })?;
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

    fn collect_partition(partition: ProtocolNoncePartition) -> Vec<u64> {
        let mut nonces = Vec::new();
        let mut iteration = 0u64;
        loop {
            match partition.nonce_at(iteration).unwrap() {
                Some(nonce) => nonces.push(nonce),
                None => break,
            }
            iteration = iteration.checked_add(1).unwrap();
        }
        nonces
    }

    #[test]
    fn single_nonce_lane_matches_linear_search_order() {
        let partition = protocol_nonce_partition(8, 1, 0).unwrap();
        assert_eq!(collect_partition(partition), (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn nonce_partitions_are_deterministic_disjoint_and_cover_domain() {
        let max_tries = 17u64;
        let lanes = protocol_nonce_lane_count(max_tries, 4);
        assert_eq!(lanes, 4);

        let first = (0..lanes)
            .map(|lane| protocol_nonce_partition(max_tries, lanes, lane).unwrap())
            .collect::<Vec<_>>();
        let second = (0..lanes)
            .map(|lane| protocol_nonce_partition(max_tries, lanes, lane).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(first, second);

        let mut seen = vec![0u8; usize::try_from(max_tries).unwrap()];
        for partition in first {
            for nonce in collect_partition(partition) {
                let index = usize::try_from(nonce).unwrap();
                seen[index] = seen[index].checked_add(1).unwrap();
            }
        }

        assert!(seen.iter().all(|count| *count == 1));
    }

    #[test]
    fn uneven_nonce_partitioning_preserves_stride_mapping() {
        let lanes = protocol_nonce_lane_count(10, 3);
        assert_eq!(lanes, 3);
        assert_eq!(
            collect_partition(protocol_nonce_partition(10, lanes, 0).unwrap()),
            vec![0, 3, 6, 9]
        );
        assert_eq!(
            collect_partition(protocol_nonce_partition(10, lanes, 1).unwrap()),
            vec![1, 4, 7]
        );
        assert_eq!(
            collect_partition(protocol_nonce_partition(10, lanes, 2).unwrap()),
            vec![2, 5, 8]
        );
    }

    #[test]
    fn requested_lanes_greater_than_tries_are_capped() {
        let lanes = protocol_nonce_lane_count(3, 64);
        assert_eq!(lanes, 3);
        for lane in 0..lanes {
            let partition = protocol_nonce_partition(3, 64, lane).unwrap();
            assert_eq!(
                collect_partition(partition),
                vec![u64::try_from(lane).unwrap()]
            );
        }
        assert!(protocol_nonce_partition(3, 64, 3).is_err());
    }

    #[test]
    fn max_tries_one_has_exactly_one_active_lane_and_nonce() {
        assert_eq!(protocol_nonce_lane_count(1, 32), 1);
        let partition = protocol_nonce_partition(1, 32, 0).unwrap();
        assert_eq!(partition.nonce_at(0).unwrap(), Some(0));
        assert_eq!(partition.nonce_at(1).unwrap(), None);
    }

    #[test]
    fn nonce_partition_overflow_fails_closed_without_wrapping() {
        let addition_overflow =
            ProtocolNoncePartition::new(u64::MAX - 2, u64::MAX - 1, u64::MAX).unwrap();
        assert_eq!(addition_overflow.nonce_at(0).unwrap(), Some(u64::MAX - 2));
        assert!(addition_overflow.nonce_at(1).is_err());

        let multiplication_overflow = ProtocolNoncePartition::new(0, u64::MAX, u64::MAX).unwrap();
        assert_eq!(multiplication_overflow.nonce_at(1).unwrap(), None);
        assert!(multiplication_overflow.nonce_at(2).is_err());
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
