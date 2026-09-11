use anyhow::{anyhow, Result};
use pulsedag_core::types::BlockHeader;
use pulsedag_core::{
    ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_miner::protocol_backend::{
    protocol_nonce_lane_count, protocol_nonce_partition, ProtocolNoncePartition,
};
use pulsedag_miner::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CudaHashResult {
    nonce: u64,
    hash: [u8; 32],
}

fn activated_identity() -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        "pulsedag-testnet-v2",
        "55".repeat(32),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn header(target_bits: u32) -> BlockHeader {
    BlockHeader {
        version: BLOCK_HEADER_VERSION_V2,
        parents: vec!["11".repeat(32), "22".repeat(32)],
        timestamp: 1_700_000_000,
        difficulty: target_bits,
        nonce: 0,
        merkle_root: "33".repeat(32),
        state_root: "44".repeat(32),
        blue_score: 12,
        height: 13,
    }
}

fn cuda_nonce_batch(partition: ProtocolNoncePartition, max_results: usize) -> Result<Vec<u64>> {
    let max_results =
        u64::try_from(max_results).map_err(|_| anyhow!("CUDA batch size does not fit in u64"))?;
    let mut nonces = Vec::new();

    for iteration in 0..max_results {
        let Some(nonce) = partition.nonce_at(iteration)? else {
            break;
        };
        nonces.push(nonce);
    }

    Ok(nonces)
}

fn simulate_cuda_hashes(work: &ProtocolPowWork, nonces: &[u64]) -> Vec<CudaHashResult> {
    nonces
        .iter()
        .map(|&nonce| {
            let attempt = work.evaluate_nonce(nonce);
            CudaHashResult {
                nonce,
                hash: attempt.final_hash.hash,
            }
        })
        .collect()
}

fn cuda_target_candidate<'a>(
    results: &'a [CudaHashResult],
    target: &[u8; 32],
) -> Option<&'a CudaHashResult> {
    results.iter().find(|result| result.hash <= *target)
}

fn reverify_cuda_candidate(work: &ProtocolPowWork, result: &CudaHashResult) -> Result<bool> {
    let canonical = work.evaluate_nonce(result.nonce);
    if canonical.final_hash.hash != result.hash {
        return Err(anyhow!(
            "CUDA hash mismatch for nonce {}; refusing accelerator result before submit",
            result.nonce
        ));
    }

    Ok(canonical.comparison.accepted())
}

#[test]
fn cuda_batches_consume_the_canonical_nonce_partition_without_duplicates() {
    let max_tries = 23u64;
    let lanes = protocol_nonce_lane_count(max_tries, 4);
    assert_eq!(lanes, 4);

    let mut all = Vec::new();
    for lane in 0..lanes {
        let partition = protocol_nonce_partition(max_tries, lanes, lane).unwrap();
        let batch = cuda_nonce_batch(partition, usize::try_from(max_tries).unwrap()).unwrap();
        all.extend(batch);
    }

    let mut sorted = all.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..max_tries).collect::<Vec<_>>());

    sorted.dedup();
    assert_eq!(sorted.len(), usize::try_from(max_tries).unwrap());
}

#[test]
fn cuda_hashes_must_match_canonical_pow_before_any_acceptance() {
    let target_bits = 0x207f_ffff;
    let header = header(target_bits);
    let identity = activated_identity();
    let work = build_protocol_pow_work(&header, target_bits, Some(&identity)).unwrap();

    let nonce = 7u64;
    let mut result = simulate_cuda_hashes(&work, &[nonce]).remove(0);
    let canonical = work.evaluate_nonce(nonce);
    assert_eq!(
        reverify_cuda_candidate(&work, &result).unwrap(),
        canonical.comparison.accepted()
    );

    result.hash[0] ^= 0x01;
    let error = reverify_cuda_candidate(&work, &result).unwrap_err();
    assert!(error.to_string().contains("CUDA hash mismatch"));
}

#[test]
fn cuda_target_search_candidate_is_reverified_by_the_canonical_core_path() {
    let target_bits = 0x207f_ffff;
    let header = header(target_bits);
    let identity = activated_identity();
    let work = build_protocol_pow_work(&header, target_bits, Some(&identity)).unwrap();

    let partition = protocol_nonce_partition(1024, 1, 0).unwrap();
    let nonces = cuda_nonce_batch(partition, 1024).unwrap();
    let results = simulate_cuda_hashes(&work, &nonces);
    let candidate = cuda_target_candidate(&results, &work.material.target.target)
        .expect("easy test target should produce a CUDA candidate within 1024 nonces");

    assert!(reverify_cuda_candidate(&work, candidate).unwrap());
    let canonical = work.evaluate_nonce(candidate.nonce);
    assert_eq!(candidate.hash, canonical.final_hash.hash);
    assert!(canonical.comparison.accepted());
}
