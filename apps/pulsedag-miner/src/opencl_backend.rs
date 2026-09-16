use crate::opencl_driver_launch;
use crate::protocol_backend::protocol_nonce_partition;
use crate::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use crate::{GpuMiningBackend, NonceSearchResult};
use anyhow::{anyhow, Result};
use pulsedag_core::pow::compare_pow_hash_to_target;
use pulsedag_core::types::BlockHeader;
use pulsedag_core::ProtocolActivationIdentity;
use sha3::{Digest, Keccak256};

trait OpenClBatchLauncher: Send + Sync {
    fn launch(
        &self,
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
        work_size: usize,
    ) -> Result<Vec<[u8; 32]>>;
}

#[derive(Debug, Default)]
struct DriverOpenClBatchLauncher;

impl OpenClBatchLauncher for DriverOpenClBatchLauncher {
    fn launch(
        &self,
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
        work_size: usize,
    ) -> Result<Vec<[u8; 32]>> {
        opencl_driver_launch::launch_kheavyhash_batch(device_index, pre_pow_hash, nonces, work_size)
    }
}

pub(crate) fn mine_canonical(
    backend: &GpuMiningBackend,
    header: BlockHeader,
    max_tries: u64,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<NonceSearchResult> {
    mine_canonical_with_launcher(
        backend,
        header,
        max_tries,
        target_bits,
        identity,
        &DriverOpenClBatchLauncher,
    )
}

fn mine_canonical_with_launcher(
    backend: &GpuMiningBackend,
    header: BlockHeader,
    max_tries: u64,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
    launcher: &dyn OpenClBatchLauncher,
) -> Result<NonceSearchResult> {
    let work = build_protocol_pow_work(&header, target_bits, identity)?;
    let batch_size = usize::try_from(backend.config().batch_size)
        .map_err(|_| anyhow!("OpenCL batch size does not fit in usize"))?;
    if batch_size == 0 {
        return Err(anyhow!("OpenCL batch size must be non-zero"));
    }
    if backend.config().work_size == 0 {
        return Err(anyhow!("OpenCL work size must be non-zero"));
    }

    search_work(
        header,
        work,
        max_tries,
        backend.selected_device().device_index,
        batch_size,
        backend.config().work_size,
        launcher,
    )
}

fn search_work(
    header: BlockHeader,
    work: ProtocolPowWork,
    max_tries: u64,
    device_index: usize,
    batch_size: usize,
    work_size: usize,
    launcher: &dyn OpenClBatchLauncher,
) -> Result<NonceSearchResult> {
    let max_tries = max_tries.max(1);
    let partition = protocol_nonce_partition(max_tries, 1, 0)?;
    let pre_pow_hash = canonical_pre_pow_hash(&work);
    let mut iteration = 0u64;
    let mut tries = 0u64;
    let mut last_nonce = None;

    loop {
        let mut nonces = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            let Some(nonce) = partition.nonce_at(iteration)? else {
                break;
            };
            nonces.push(nonce);
            iteration = iteration.checked_add(1).ok_or_else(|| {
                anyhow!(
                    "OpenCL nonce iteration overflow for canonical lane {}",
                    partition.lane()
                )
            })?;
        }

        if nonces.is_empty() {
            break;
        }

        let hashes = launcher.launch(device_index, pre_pow_hash, &nonces, work_size)?;
        if hashes.len() != nonces.len() {
            return Err(anyhow!(
                "OpenCL launcher returned {} hash(es) for {} nonce(s); refusing incomplete accelerator result",
                hashes.len(),
                nonces.len()
            ));
        }

        let batch_tries = u64::try_from(nonces.len())
            .map_err(|_| anyhow!("OpenCL batch nonce count does not fit in u64"))?;
        tries = tries
            .checked_add(batch_tries)
            .ok_or_else(|| anyhow!("OpenCL nonce attempt counter overflow"))?;

        for (&nonce, &accelerator_hash) in nonces.iter().zip(&hashes) {
            last_nonce = Some(nonce);
            if !compare_pow_hash_to_target(&accelerator_hash, &work.material.target.target) {
                continue;
            }

            let accepted = work.reverify_accelerator_hash(nonce, accelerator_hash)?;
            if !accepted {
                return Err(anyhow!(
                    "OpenCL accelerator hash passed target prefilter for nonce {nonce} but canonical re-verification rejected it"
                ));
            }

            let mut winner = header.clone();
            winner.nonce = nonce;
            let canonical = work.evaluate_nonce(nonce);
            return Ok(NonceSearchResult {
                header: winner,
                accepted: true,
                tries,
                final_hash_hex: canonical.final_hash.hash_hex,
            });
        }
    }

    let fallback_nonce = last_nonce.ok_or_else(|| {
        anyhow!("OpenCL canonical nonce partition produced no work after normalization")
    })?;
    let canonical = work.evaluate_nonce(fallback_nonce);
    let mut fallback = header;
    fallback.nonce = fallback_nonce;
    Ok(NonceSearchResult {
        header: fallback,
        accepted: false,
        tries: tries.max(1),
        final_hash_hex: canonical.final_hash.hash_hex,
    })
}

fn canonical_pre_pow_hash(work: &ProtocolPowWork) -> [u8; 32] {
    let digest = Keccak256::digest(&work.material.pre_pow_bytes);
    let mut pre_pow_hash = [0u8; 32];
    pre_pow_hash.copy_from_slice(&digest);
    pre_pow_hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GpuBackendConfig, OpenClDeviceSelection};
    use pulsedag_core::{
        BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION,
    };
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct LaunchCall {
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: Vec<u64>,
        work_size: usize,
    }

    struct FakeLauncher {
        hashes: BTreeMap<u64, [u8; 32]>,
        calls: Mutex<Vec<LaunchCall>>,
        error: Option<String>,
    }

    impl FakeLauncher {
        fn canonical(work: &ProtocolPowWork, nonces: std::ops::Range<u64>) -> Self {
            let hashes = nonces
                .map(|nonce| (nonce, work.evaluate_nonce(nonce).final_hash.hash))
                .collect();
            Self {
                hashes,
                calls: Mutex::new(Vec::new()),
                error: None,
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                hashes: BTreeMap::new(),
                calls: Mutex::new(Vec::new()),
                error: Some(message.to_string()),
            }
        }

        fn calls(&self) -> Vec<LaunchCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl OpenClBatchLauncher for FakeLauncher {
        fn launch(
            &self,
            device_index: usize,
            pre_pow_hash: [u8; 32],
            nonces: &[u64],
            work_size: usize,
        ) -> Result<Vec<[u8; 32]>> {
            self.calls.lock().unwrap().push(LaunchCall {
                device_index,
                pre_pow_hash,
                nonces: nonces.to_vec(),
                work_size,
            });
            if let Some(error) = &self.error {
                return Err(anyhow!(error.clone()));
            }
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes.get(nonce).copied().ok_or_else(|| {
                        anyhow!("fake OpenCL launcher has no hash for nonce {nonce}")
                    })
                })
                .collect()
        }
    }

    fn header(version: u32, target_bits: u32) -> BlockHeader {
        BlockHeader {
            version,
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

    fn activated_identity() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "55".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn test_backend(device_index: usize, batch_size: u64, work_size: usize) -> GpuMiningBackend {
        GpuMiningBackend::for_test(
            GpuBackendConfig {
                device_index: Some(device_index),
                batch_size,
                work_size,
            },
            OpenClDeviceSelection {
                platform_index: 0,
                device_index,
                platform_name: "test-platform".to_string(),
                device_name: "test-device".to_string(),
            },
        )
    }

    #[test]
    fn zero_opencl_launch_shape_fails_closed() {
        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let launcher = FakeLauncher::canonical(
            &build_protocol_pow_work(&header, target_bits, None).unwrap(),
            0..1,
        );

        let error = mine_canonical_with_launcher(
            &test_backend(0, 0, 64),
            header.clone(),
            1,
            target_bits,
            None,
            &launcher,
        )
        .unwrap_err();
        assert!(error.to_string().contains("batch size must be non-zero"));

        let error = mine_canonical_with_launcher(
            &test_backend(0, 1, 0),
            header,
            1,
            target_bits,
            None,
            &launcher,
        )
        .unwrap_err();
        assert!(error.to_string().contains("work size must be non-zero"));
    }

    #[test]
    fn deterministic_batches_select_requested_device_and_work_size() {
        let target_bits = 0x0300_0001;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));
        let hashes = (0..7).map(|nonce| (nonce, rejected_hash)).collect();
        let launcher = FakeLauncher {
            hashes,
            calls: Mutex::new(Vec::new()),
            error: None,
        };
        let backend = test_backend(3, 3, 64);

        let result =
            mine_canonical_with_launcher(&backend, header, 7, target_bits, None, &launcher)
                .unwrap();
        assert!(!result.accepted);
        assert_eq!(result.tries, 7);
        assert_eq!(result.header.nonce, 6);

        let calls = launcher.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].nonces, vec![0, 1, 2]);
        assert_eq!(calls[1].nonces, vec![3, 4, 5]);
        assert_eq!(calls[2].nonces, vec![6]);
        assert!(calls.iter().all(|call| call.device_index == 3));
        assert!(calls.iter().all(|call| call.work_size == 64));
        assert!(calls
            .iter()
            .all(|call| call.pre_pow_hash == canonical_pre_pow_hash(&work)));
    }

    #[test]
    fn legacy_v1_uses_canonical_protocol_pow_work() {
        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let launcher = FakeLauncher::canonical(&work, 0..1);
        let backend = test_backend(0, 1, 64);

        let result =
            mine_canonical_with_launcher(&backend, header, 1, target_bits, None, &launcher)
                .expect("legacy OpenCL search should use canonical work");
        assert_eq!(result.header.nonce, 0);
        assert_eq!(
            launcher.calls()[0].pre_pow_hash,
            canonical_pre_pow_hash(&work)
        );
    }

    #[test]
    fn activated_v2_uses_canonical_protocol_pow_work() {
        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
        let identity = activated_identity();
        let work = build_protocol_pow_work(&header, target_bits, Some(&identity)).unwrap();
        let launcher = FakeLauncher::canonical(&work, 0..1);
        let backend = test_backend(0, 1, 64);

        let result = mine_canonical_with_launcher(
            &backend,
            header,
            1,
            target_bits,
            Some(&identity),
            &launcher,
        )
        .expect("v2 OpenCL search should use canonical protocol work");
        assert_eq!(result.header.nonce, 0);
        assert_eq!(
            launcher.calls()[0].pre_pow_hash,
            canonical_pre_pow_hash(&work)
        );
    }

    #[test]
    fn launcher_error_fails_closed_without_cpu_fallback() {
        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let launcher = FakeLauncher::failing("mock OpenCL launch failure");
        let backend = test_backend(0, 2, 64);

        let error = mine_canonical_with_launcher(&backend, header, 2, target_bits, None, &launcher)
            .unwrap_err();
        assert!(error.to_string().contains("mock OpenCL launch failure"));
    }

    #[test]
    fn wrong_target_passing_accelerator_hash_is_rejected_by_canonical_reverification() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let mut hashes = BTreeMap::new();
        hashes.insert(0, [0u8; 32]);
        assert_ne!(work.evaluate_nonce(0).final_hash.hash, [0u8; 32]);
        let launcher = FakeLauncher {
            hashes,
            calls: Mutex::new(Vec::new()),
            error: None,
        };
        let backend = test_backend(0, 1, 64);

        let error = mine_canonical_with_launcher(&backend, header, 1, target_bits, None, &launcher)
            .unwrap_err();
        assert!(error.to_string().contains("accelerator hash mismatch"));
    }

    #[test]
    fn incomplete_accelerator_batch_fails_closed() {
        struct ShortLauncher;
        impl OpenClBatchLauncher for ShortLauncher {
            fn launch(
                &self,
                _device_index: usize,
                _pre_pow_hash: [u8; 32],
                _nonces: &[u64],
                _work_size: usize,
            ) -> Result<Vec<[u8; 32]>> {
                Ok(Vec::new())
            }
        }

        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let backend = test_backend(0, 2, 64);
        let error =
            mine_canonical_with_launcher(&backend, header, 2, target_bits, None, &ShortLauncher)
                .unwrap_err();
        assert!(error
            .to_string()
            .contains("refusing incomplete accelerator result"));
    }

    #[test]
    fn lowest_target_passing_nonce_is_selected_deterministically() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let first_accepted = (0..64)
            .find(|nonce| work.evaluate_nonce(*nonce).comparison.accepted())
            .expect("easy target should yield a deterministic accepted nonce");
        let launcher = FakeLauncher::canonical(&work, 0..64);
        let backend = test_backend(0, 64, 64);

        let result =
            mine_canonical_with_launcher(&backend, header, 64, target_bits, None, &launcher)
                .unwrap();
        assert!(result.accepted);
        assert_eq!(result.header.nonce, first_accepted);
        assert_eq!(
            result.final_hash_hex,
            work.evaluate_nonce(first_accepted).final_hash.hash_hex
        );
    }
}
