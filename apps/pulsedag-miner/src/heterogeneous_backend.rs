use crate::accelerator_scheduler::{
    AcceleratorBackendKind, AcceleratorDeviceKey, AcceleratorSchedule,
};
use crate::cuda_backend::CudaMiningBackend;
use anyhow::{anyhow, Result};
use pulsedag_core::pow::compare_pow_hash_to_target;
use pulsedag_core::types::BlockHeader;
use pulsedag_core::ProtocolActivationIdentity;
use pulsedag_miner::protocol_backend::ProtocolMiningBackend;
use pulsedag_miner::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use pulsedag_miner::{GpuMiningBackend, MiningBackend, NonceSearchResult};
use sha3::{Digest, Keccak256};
use std::sync::Arc;

trait MixedBatchLauncher: Send + Sync {
    fn batch_size(&self, owner: AcceleratorDeviceKey) -> Result<usize>;

    fn launch(
        &self,
        owner: AcceleratorDeviceKey,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
    ) -> Result<Vec<[u8; 32]>>;
}

struct ProductMixedBatchLauncher {
    cuda: CudaMiningBackend,
    opencl: GpuMiningBackend,
}

impl MixedBatchLauncher for ProductMixedBatchLauncher {
    fn batch_size(&self, owner: AcceleratorDeviceKey) -> Result<usize> {
        match owner.backend {
            AcceleratorBackendKind::Cuda => Ok(self.cuda.mixed_runtime_batch_size()),
            AcceleratorBackendKind::OpenCl => self.opencl.mixed_runtime_batch_size(),
        }
    }

    fn launch(
        &self,
        owner: AcceleratorDeviceKey,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
    ) -> Result<Vec<[u8; 32]>> {
        match owner.backend {
            AcceleratorBackendKind::Cuda => self.cuda.launch_mixed_runtime_batch(
                owner.device_index,
                pre_pow_hash,
                nonces,
            ),
            AcceleratorBackendKind::OpenCl => self.opencl.launch_mixed_runtime_batch(
                owner.device_index,
                pre_pow_hash,
                nonces,
            ),
        }
    }
}

pub(crate) struct HeterogeneousMiningBackend {
    devices: Vec<AcceleratorDeviceKey>,
    launcher: Arc<dyn MixedBatchLauncher>,
}

impl HeterogeneousMiningBackend {
    pub(crate) fn new(cuda: CudaMiningBackend, opencl: GpuMiningBackend) -> Result<Self> {
        let mut devices = cuda
            .selected_device_indices()
            .iter()
            .copied()
            .map(AcceleratorDeviceKey::cuda)
            .collect::<Vec<_>>();
        devices.extend(
            opencl
                .selected_devices()
                .iter()
                .map(|device| AcceleratorDeviceKey::opencl(device.device_index)),
        );
        Self::from_parts(
            devices,
            Arc::new(ProductMixedBatchLauncher { cuda, opencl }),
        )
    }

    fn from_parts(
        mut devices: Vec<AcceleratorDeviceKey>,
        launcher: Arc<dyn MixedBatchLauncher>,
    ) -> Result<Self> {
        devices.sort_unstable();
        if !devices
            .iter()
            .any(|device| device.backend == AcceleratorBackendKind::Cuda)
        {
            return Err(anyhow!("mixed runtime requires at least one CUDA device"));
        }
        if !devices
            .iter()
            .any(|device| device.backend == AcceleratorBackendKind::OpenCl)
        {
            return Err(anyhow!("mixed runtime requires at least one OpenCL device"));
        }
        AcceleratorSchedule::build(&devices, 1)?;
        Ok(Self { devices, launcher })
    }

    pub(crate) fn devices(&self) -> &[AcceleratorDeviceKey] {
        &self.devices
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
        let schedule = AcceleratorSchedule::build(&self.devices, max_tries)?;
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
                let batch_size = self.launcher.batch_size(lane.owner)?;
                if batch_size == 0 {
                    return Err(anyhow!(
                        "mixed runtime batch size must be non-zero for {:?}[{}]",
                        lane.owner.backend,
                        lane.owner.device_index
                    ));
                }

                let mut next_iteration = iterations[lane_index];
                let mut lane_exhausted = false;
                let mut nonces = Vec::with_capacity(batch_size);
                for _ in 0..batch_size {
                    let Some(nonce) = partition.nonce_at(next_iteration)? else {
                        lane_exhausted = true;
                        break;
                    };
                    nonces.push(nonce);
                    next_iteration = next_iteration.checked_add(1).ok_or_else(|| {
                        anyhow!(
                            "mixed runtime nonce iteration overflow for canonical lane {}",
                            partition.lane()
                        )
                    })?;
                }

                if nonces.is_empty() {
                    exhausted[lane_index] = true;
                    continue;
                }
                made_progress = true;

                let hashes = self
                    .launcher
                    .launch(lane.owner, pre_pow_hash, &nonces)
                    .map_err(|err| {
                        anyhow!(
                            "mixed runtime {:?}[{}] launch failed closed: {err}",
                            lane.owner.backend,
                            lane.owner.device_index
                        )
                    })?;
                if hashes.len() != nonces.len() {
                    return Err(anyhow!(
                        "mixed runtime {:?}[{}] returned {} hash(es) for {} nonce(s); refusing incomplete accelerator result",
                        lane.owner.backend,
                        lane.owner.device_index,
                        hashes.len(),
                        nonces.len()
                    ));
                }

                iterations[lane_index] = next_iteration;
                if lane_exhausted {
                    exhausted[lane_index] = true;
                }

                let batch_tries = u64::try_from(nonces.len())
                    .map_err(|_| anyhow!("mixed runtime batch nonce count does not fit in u64"))?;
                tries = tries
                    .checked_add(batch_tries)
                    .ok_or_else(|| anyhow!("mixed runtime nonce attempt counter overflow"))?;

                for (&nonce, &accelerator_hash) in nonces.iter().zip(&hashes) {
                    last_nonce = Some(nonce);
                    if !compare_pow_hash_to_target(&accelerator_hash, &work.material.target.target) {
                        continue;
                    }

                    let accepted = work.reverify_accelerator_hash(nonce, accelerator_hash)?;
                    if !accepted {
                        return Err(anyhow!(
                            "mixed runtime accelerator hash passed target prefilter for nonce {nonce} but canonical re-verification rejected it"
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
            anyhow!("mixed runtime canonical nonce partitions produced no work after normalization")
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

    #[cfg(test)]
    fn with_launcher(
        devices: Vec<AcceleratorDeviceKey>,
        launcher: Arc<dyn MixedBatchLauncher>,
    ) -> Result<Self> {
        Self::from_parts(devices, launcher)
    }
}

impl MiningBackend for HeterogeneousMiningBackend {
    fn name(&self) -> &'static str {
        "mixed"
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

impl ProtocolMiningBackend for HeterogeneousMiningBackend {
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
    use pulsedag_core::BLOCK_HEADER_VERSION_V1;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct LaunchCall {
        owner: AcceleratorDeviceKey,
        nonces: Vec<u64>,
        pre_pow_hash: [u8; 32],
    }

    struct FakeMixedLauncher {
        hashes: BTreeMap<u64, [u8; 32]>,
        calls: Mutex<Vec<LaunchCall>>,
        batch_size: usize,
    }

    impl FakeMixedLauncher {
        fn calls(&self) -> Vec<LaunchCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl MixedBatchLauncher for FakeMixedLauncher {
        fn batch_size(&self, _owner: AcceleratorDeviceKey) -> Result<usize> {
            Ok(self.batch_size)
        }

        fn launch(
            &self,
            owner: AcceleratorDeviceKey,
            pre_pow_hash: [u8; 32],
            nonces: &[u64],
        ) -> Result<Vec<[u8; 32]>> {
            self.calls.lock().unwrap().push(LaunchCall {
                owner,
                nonces: nonces.to_vec(),
                pre_pow_hash,
            });
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes
                        .get(nonce)
                        .copied()
                        .ok_or_else(|| anyhow!("fake mixed launcher has no hash for nonce {nonce}"))
                })
                .collect()
        }
    }

    fn header(target_bits: u32) -> BlockHeader {
        BlockHeader {
            version: BLOCK_HEADER_VERSION_V1,
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

    #[test]
    fn mixed_schedule_covers_nonce_domain_once_across_cuda_and_opencl() {
        let target_bits = 0x0300_0001;
        let header = header(target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));

        let launcher = Arc::new(FakeMixedLauncher {
            hashes: (0..7).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            batch_size: 2,
        });
        let backend = HeterogeneousMiningBackend::with_launcher(
            vec![
                AcceleratorDeviceKey::opencl(4),
                AcceleratorDeviceKey::cuda(1),
            ],
            launcher.clone(),
        )
        .unwrap();

        let result = backend.mine_header(header, 7, 1, target_bits).unwrap();
        assert!(!result.accepted);
        assert_eq!(result.tries, 7);

        let calls = launcher.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].owner, AcceleratorDeviceKey::cuda(1));
        assert_eq!(calls[0].nonces, vec![0, 2]);
        assert_eq!(calls[1].owner, AcceleratorDeviceKey::opencl(4));
        assert_eq!(calls[1].nonces, vec![1, 3]);
        assert_eq!(calls[2].owner, AcceleratorDeviceKey::cuda(1));
        assert_eq!(calls[2].nonces, vec![4, 6]);
        assert_eq!(calls[3].owner, AcceleratorDeviceKey::opencl(4));
        assert_eq!(calls[3].nonces, vec![5]);

        let mut seen = calls
            .iter()
            .flat_map(|call| call.nonces.iter().copied())
            .collect::<Vec<_>>();
        seen.sort_unstable();
        assert_eq!(seen, (0..7).collect::<Vec<_>>());
        assert!(calls
            .iter()
            .all(|call| call.pre_pow_hash == canonical_pre_pow_hash(&work)));
    }

    #[test]
    fn mixed_runtime_requires_both_vendor_kinds() {
        let launcher = Arc::new(FakeMixedLauncher {
            hashes: BTreeMap::new(),
            calls: Mutex::new(Vec::new()),
            batch_size: 1,
        });
        let error = HeterogeneousMiningBackend::with_launcher(
            vec![AcceleratorDeviceKey::cuda(0)],
            launcher,
        )
        .err()
        .expect("single-vendor inventory must fail");
        assert!(error.to_string().contains("OpenCL"));
    }
}
