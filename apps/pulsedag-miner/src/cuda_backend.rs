use crate::accelerator_scheduler::{AcceleratorDeviceKey, AcceleratorSchedule};
use anyhow::{anyhow, Result};
use pulsedag_core::pow::compare_pow_hash_to_target;
use pulsedag_core::types::BlockHeader;
use pulsedag_core::ProtocolActivationIdentity;
use pulsedag_miner::cuda_driver_launch;
use pulsedag_miner::protocol_backend::ProtocolMiningBackend;
use pulsedag_miner::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use pulsedag_miner::{MiningBackend, NonceSearchResult};
use sha3::{Digest, Keccak256};
use std::sync::Arc;

const DEFAULT_CUDA_BATCH_SIZE: usize = 4_096;
const DEFAULT_CUDA_BLOCK_SIZE: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CudaBackendConfig {
    module_image: Vec<u8>,
    device_indices: Vec<usize>,
    batch_size: usize,
    block_size: u32,
}

impl CudaBackendConfig {
    pub(crate) fn new(module_image: Vec<u8>, device_index: usize) -> Result<Self> {
        Self::with_launch_shape(
            module_image,
            device_index,
            DEFAULT_CUDA_BATCH_SIZE,
            DEFAULT_CUDA_BLOCK_SIZE,
        )
    }

    pub(crate) fn for_devices(module_image: Vec<u8>, device_indices: Vec<usize>) -> Result<Self> {
        Self::with_devices_and_launch_shape(
            module_image,
            device_indices,
            DEFAULT_CUDA_BATCH_SIZE,
            DEFAULT_CUDA_BLOCK_SIZE,
        )
    }

    fn with_launch_shape(
        module_image: Vec<u8>,
        device_index: usize,
        batch_size: usize,
        block_size: u32,
    ) -> Result<Self> {
        Self::with_devices_and_launch_shape(
            module_image,
            vec![device_index],
            batch_size,
            block_size,
        )
    }

    fn with_devices_and_launch_shape(
        module_image: Vec<u8>,
        mut device_indices: Vec<usize>,
        batch_size: usize,
        block_size: u32,
    ) -> Result<Self> {
        if module_image.is_empty() {
            return Err(anyhow!("CUDA module image is empty"));
        }
        if device_indices.is_empty() {
            return Err(anyhow!("CUDA device selection must not be empty"));
        }
        device_indices.sort_unstable();
        if device_indices.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(anyhow!("CUDA device selection contains duplicate indices"));
        }
        if batch_size == 0 {
            return Err(anyhow!("CUDA batch size must be non-zero"));
        }
        if block_size == 0 {
            return Err(anyhow!("CUDA block size must be non-zero"));
        }

        Ok(Self {
            module_image,
            device_indices,
            batch_size,
            block_size,
        })
    }
}

trait CudaBatchLauncher: Send + Sync {
    fn launch(
        &self,
        module_image: &[u8],
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
        block_size: u32,
    ) -> Result<Vec<[u8; 32]>>;
}

#[derive(Debug, Default)]
struct DriverCudaBatchLauncher;

impl CudaBatchLauncher for DriverCudaBatchLauncher {
    fn launch(
        &self,
        module_image: &[u8],
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
        block_size: u32,
    ) -> Result<Vec<[u8; 32]>> {
        cuda_driver_launch::launch_kheavyhash_batch(
            module_image,
            device_index,
            pre_pow_hash,
            nonces,
            block_size,
        )
    }
}

pub(crate) struct CudaMiningBackend {
    config: CudaBackendConfig,
    launcher: Arc<dyn CudaBatchLauncher>,
}

impl CudaMiningBackend {
    pub(crate) fn new(config: CudaBackendConfig) -> Self {
        Self {
            config,
            launcher: Arc::new(DriverCudaBatchLauncher),
        }
    }

    #[cfg(test)]
    fn with_launcher(config: CudaBackendConfig, launcher: Arc<dyn CudaBatchLauncher>) -> Self {
        Self { config, launcher }
    }

    pub(crate) fn selected_device_indices(&self) -> &[usize] {
        &self.config.device_indices
    }

    pub(crate) fn mixed_runtime_batch_size(&self) -> usize {
        self.config.batch_size
    }

    pub(crate) fn launch_mixed_runtime_batch(
        &self,
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
    ) -> Result<Vec<[u8; 32]>> {
        if !self.config.device_indices.contains(&device_index) {
            return Err(anyhow!(
                "CUDA mixed runtime requested unselected device index {device_index}"
            ));
        }
        self.launcher.launch(
            &self.config.module_image,
            device_index,
            pre_pow_hash,
            nonces,
            self.config.block_size,
        )
    }

    fn mine_canonical(
        &self,
        header: BlockHeader,
        max_tries: u64,
        target_bits: u32,
        identity: Option<&ProtocolActivationIdentity>,
    ) -> Result<NonceSearchResult> {
        let work = build_protocol_pow_work(&header, target_bits, identity)?;
        self.search_work(header, work, max_tries)
    }

    fn search_work(
        &self,
        header: BlockHeader,
        work: ProtocolPowWork,
        max_tries: u64,
    ) -> Result<NonceSearchResult> {
        let max_tries = max_tries.max(1);
        let devices = self
            .config
            .device_indices
            .iter()
            .copied()
            .map(AcceleratorDeviceKey::cuda)
            .collect::<Vec<_>>();
        let schedule = AcceleratorSchedule::build(&devices, max_tries)?;
        let active_lanes = schedule.lanes().len();
        let mut iterations = vec![0u64; active_lanes];
        let mut exhausted = vec![false; active_lanes];
        let pre_pow_hash = canonical_pre_pow_hash(&work);
        let mut tries = 0u64;
        let mut last_nonce = None;

        loop {
            let mut made_progress = false;
            for lane_index in 0..active_lanes {
                if exhausted[lane_index] {
                    continue;
                }

                let lane = &schedule.lanes()[lane_index];
                let partition = lane.partition;
                let mut next_iteration = iterations[lane_index];
                let mut lane_exhausted = false;
                let mut nonces = Vec::with_capacity(self.config.batch_size);
                for _ in 0..self.config.batch_size {
                    let Some(nonce) = partition.nonce_at(next_iteration)? else {
                        lane_exhausted = true;
                        break;
                    };
                    nonces.push(nonce);
                    next_iteration = next_iteration.checked_add(1).ok_or_else(|| {
                        anyhow!(
                            "CUDA nonce iteration overflow for canonical lane {}",
                            partition.lane()
                        )
                    })?;
                }

                if nonces.is_empty() {
                    exhausted[lane_index] = true;
                    continue;
                }
                made_progress = true;

                let hashes = self.launcher.launch(
                    &self.config.module_image,
                    lane.owner.device_index,
                    pre_pow_hash,
                    &nonces,
                    self.config.block_size,
                )?;
                if hashes.len() != nonces.len() {
                    return Err(anyhow!(
                        "CUDA launcher returned {} hash(es) for {} nonce(s); refusing incomplete accelerator result",
                        hashes.len(),
                        nonces.len()
                    ));
                }

                iterations[lane_index] = next_iteration;
                if lane_exhausted {
                    exhausted[lane_index] = true;
                }

                let batch_tries = u64::try_from(nonces.len())
                    .map_err(|_| anyhow!("CUDA batch nonce count does not fit in u64"))?;
                tries = tries
                    .checked_add(batch_tries)
                    .ok_or_else(|| anyhow!("CUDA nonce attempt counter overflow"))?;

                for (&nonce, &accelerator_hash) in nonces.iter().zip(&hashes) {
                    last_nonce = Some(nonce);
                    if !compare_pow_hash_to_target(&accelerator_hash, &work.material.target.target)
                    {
                        continue;
                    }

                    let accepted = work.reverify_accelerator_hash(nonce, accelerator_hash)?;
                    if !accepted {
                        return Err(anyhow!(
                            "CUDA accelerator hash passed target prefilter for nonce {nonce} but canonical re-verification rejected it"
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

            if !made_progress {
                break;
            }
        }

        let fallback_nonce = last_nonce.ok_or_else(|| {
            anyhow!("CUDA canonical nonce partitions produced no work after normalization")
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
}

impl MiningBackend for CudaMiningBackend {
    fn name(&self) -> &'static str {
        "cuda"
    }

    fn mine_header(
        &self,
        header: BlockHeader,
        max_tries: u64,
        _threads: usize,
        target_bits: u32,
    ) -> Result<NonceSearchResult> {
        self.mine_canonical(header, max_tries, target_bits, None)
    }
}

impl ProtocolMiningBackend for CudaMiningBackend {
    fn mine_header_for_protocol(
        &self,
        header: BlockHeader,
        max_tries: u64,
        _threads: usize,
        target_bits: u32,
        identity: &ProtocolActivationIdentity,
    ) -> Result<NonceSearchResult> {
        self.mine_canonical(header, max_tries, target_bits, Some(identity))
    }
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
    use pulsedag_core::{
        BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION,
    };
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct LaunchCall {
        module_image: Vec<u8>,
        device_index: usize,
        pre_pow_hash: [u8; 32],
        nonces: Vec<u64>,
        block_size: u32,
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

    impl CudaBatchLauncher for FakeLauncher {
        fn launch(
            &self,
            module_image: &[u8],
            device_index: usize,
            pre_pow_hash: [u8; 32],
            nonces: &[u64],
            block_size: u32,
        ) -> Result<Vec<[u8; 32]>> {
            self.calls.lock().unwrap().push(LaunchCall {
                module_image: module_image.to_vec(),
                device_index,
                pre_pow_hash,
                nonces: nonces.to_vec(),
                block_size,
            });
            if let Some(error) = &self.error {
                return Err(anyhow!(error.clone()));
            }
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes
                        .get(nonce)
                        .copied()
                        .ok_or_else(|| anyhow!("fake CUDA launcher has no hash for nonce {nonce}"))
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

    fn test_config(device_index: usize, batch_size: usize) -> CudaBackendConfig {
        CudaBackendConfig::with_launch_shape(
            b"test-cuda-module".to_vec(),
            device_index,
            batch_size,
            64,
        )
        .unwrap()
    }

    fn test_config_devices(device_indices: &[usize], batch_size: usize) -> CudaBackendConfig {
        CudaBackendConfig::with_devices_and_launch_shape(
            b"test-cuda-module".to_vec(),
            device_indices.to_vec(),
            batch_size,
            64,
        )
        .unwrap()
    }

    #[test]
    fn empty_cuda_module_fails_closed() {
        let error = CudaBackendConfig::new(Vec::new(), 0).unwrap_err();
        assert!(error.to_string().contains("CUDA module image is empty"));
    }

    #[test]
    fn zero_cuda_launch_shape_fails_closed() {
        assert!(CudaBackendConfig::with_launch_shape(vec![1], 0, 0, 64).is_err());
        assert!(CudaBackendConfig::with_launch_shape(vec![1], 0, 1, 0).is_err());
    }

    #[test]
    fn cuda_device_inventory_is_canonicalized_and_duplicates_fail_closed() {
        let config = CudaBackendConfig::for_devices(vec![1], vec![3, 1, 2]).unwrap();
        assert_eq!(config.device_indices, vec![1, 2, 3]);
        assert!(CudaBackendConfig::for_devices(vec![1], Vec::new()).is_err());
        assert!(CudaBackendConfig::for_devices(vec![1], vec![1, 1]).is_err());
    }

    #[test]
    fn deterministic_batches_select_requested_device_and_module() {
        let target_bits = 0x0300_0001;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));
        let hashes = (0..7).map(|nonce| (nonce, rejected_hash)).collect();
        let launcher = Arc::new(FakeLauncher {
            hashes,
            calls: Mutex::new(Vec::new()),
            error: None,
        });
        let backend = CudaMiningBackend::with_launcher(test_config(3, 3), launcher.clone());

        let result = backend.mine_header(header, 7, 99, target_bits).unwrap();
        assert!(!result.accepted);
        assert_eq!(result.tries, 7);
        assert_eq!(result.header.nonce, 6);

        let calls = launcher.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].nonces, vec![0, 1, 2]);
        assert_eq!(calls[1].nonces, vec![3, 4, 5]);
        assert_eq!(calls[2].nonces, vec![6]);
        assert!(calls.iter().all(|call| call.device_index == 3));
        assert!(calls
            .iter()
            .all(|call| call.module_image == b"test-cuda-module"));
        assert!(calls.iter().all(|call| call.block_size == 64));
        assert!(calls
            .iter()
            .all(|call| call.pre_pow_hash == canonical_pre_pow_hash(&work)));
    }

    #[test]
    fn homogeneous_multidevice_schedule_covers_nonce_domain_once() {
        let target_bits = 0x0300_0001;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));
        let hashes = (0..7).map(|nonce| (nonce, rejected_hash)).collect();
        let launcher = Arc::new(FakeLauncher {
            hashes,
            calls: Mutex::new(Vec::new()),
            error: None,
        });
        let backend =
            CudaMiningBackend::with_launcher(test_config_devices(&[3, 1], 2), launcher.clone());

        let result = backend.mine_header(header, 7, 1, target_bits).unwrap();
        assert!(!result.accepted);
        assert_eq!(result.tries, 7);

        let calls = launcher.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].device_index, 1);
        assert_eq!(calls[0].nonces, vec![0, 2]);
        assert_eq!(calls[1].device_index, 3);
        assert_eq!(calls[1].nonces, vec![1, 3]);
        assert_eq!(calls[2].device_index, 1);
        assert_eq!(calls[2].nonces, vec![4, 6]);
        assert_eq!(calls[3].device_index, 3);
        assert_eq!(calls[3].nonces, vec![5]);

        let mut seen = calls
            .iter()
            .flat_map(|call| call.nonces.iter().copied())
            .collect::<Vec<_>>();
        seen.sort_unstable();
        assert_eq!(seen, (0..7).collect::<Vec<_>>());
    }

    #[test]
    fn legacy_v1_uses_canonical_protocol_pow_work() {
        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let launcher = Arc::new(FakeLauncher::canonical(&work, 0..1));
        let backend = CudaMiningBackend::with_launcher(test_config(0, 1), launcher.clone());

        let result = backend
            .mine_header(header, 1, 1, target_bits)
            .expect("legacy CUDA search should use canonical work");
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
        let launcher = Arc::new(FakeLauncher::canonical(&work, 0..1));
        let backend = CudaMiningBackend::with_launcher(test_config(0, 1), launcher.clone());

        let result = backend
            .mine_header_for_protocol(header, 1, 1, target_bits, &identity)
            .expect("v2 CUDA search should use canonical protocol work");
        assert_eq!(result.header.nonce, 0);
        assert_eq!(
            launcher.calls()[0].pre_pow_hash,
            canonical_pre_pow_hash(&work)
        );
    }

    #[test]
    fn launcher_error_fails_closed_without_fallback() {
        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let launcher = Arc::new(FakeLauncher::failing("mock CUDA launch failure"));
        let backend = CudaMiningBackend::with_launcher(test_config(0, 2), launcher);

        let error = backend.mine_header(header, 2, 1, target_bits).unwrap_err();
        assert!(error.to_string().contains("mock CUDA launch failure"));
    }

    #[test]
    fn wrong_target_passing_accelerator_hash_is_rejected_by_canonical_reverification() {
        let target_bits = 0x207f_ffff;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let mut hashes = BTreeMap::new();
        hashes.insert(0, [0u8; 32]);
        assert_ne!(work.evaluate_nonce(0).final_hash.hash, [0u8; 32]);
        let launcher = Arc::new(FakeLauncher {
            hashes,
            calls: Mutex::new(Vec::new()),
            error: None,
        });
        let backend = CudaMiningBackend::with_launcher(test_config(0, 1), launcher);

        let error = backend.mine_header(header, 1, 1, target_bits).unwrap_err();
        assert!(error.to_string().contains("accelerator hash mismatch"));
    }

    #[test]
    fn incomplete_accelerator_batch_fails_closed() {
        struct ShortLauncher;
        impl CudaBatchLauncher for ShortLauncher {
            fn launch(
                &self,
                _module_image: &[u8],
                _device_index: usize,
                _pre_pow_hash: [u8; 32],
                _nonces: &[u64],
                _block_size: u32,
            ) -> Result<Vec<[u8; 32]>> {
                Ok(Vec::new())
            }
        }

        let target_bits = 1;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let backend = CudaMiningBackend::with_launcher(test_config(0, 2), Arc::new(ShortLauncher));
        let error = backend.mine_header(header, 2, 1, target_bits).unwrap_err();
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
        let launcher = Arc::new(FakeLauncher::canonical(&work, 0..64));
        let backend = CudaMiningBackend::with_launcher(test_config(0, 64), launcher);

        let result = backend.mine_header(header, 64, 1, target_bits).unwrap();
        assert!(result.accepted);
        assert_eq!(result.header.nonce, first_accepted);
        assert_eq!(
            result.final_hash_hex,
            work.evaluate_nonce(first_accepted).final_hash.hash_hex
        );
    }
}
