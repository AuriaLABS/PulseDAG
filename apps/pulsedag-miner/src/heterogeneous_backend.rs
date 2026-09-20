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
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

const MIXED_RECOVERY_PROBE_PRE_POW_HASH: [u8; 32] = [0x5a; 32];
const MIXED_RECOVERY_PROBE_NONCE: u64 = 0;

trait MixedBatchLauncher: Send + Sync {
    fn batch_size(&self, owner: AcceleratorDeviceKey) -> Result<usize>;

    fn probe(&self, _owner: AcceleratorDeviceKey) -> Result<()> {
        Ok(())
    }

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

    fn probe(&self, owner: AcceleratorDeviceKey) -> Result<()> {
        match owner.backend {
            AcceleratorBackendKind::Cuda => {
                self.cuda.probe_mixed_runtime_device(owner.device_index)
            }
            AcceleratorBackendKind::OpenCl => {
                let hashes = self.opencl.launch_mixed_runtime_batch(
                    owner.device_index,
                    MIXED_RECOVERY_PROBE_PRE_POW_HASH,
                    &[MIXED_RECOVERY_PROBE_NONCE],
                )?;
                if hashes.len() != 1 {
                    return Err(anyhow!(
                        "mixed OpenCL recovery capability probe returned {} hash(es); expected exactly 1",
                        hashes.len()
                    ));
                }
                Ok(())
            }
        }
    }

    fn launch(
        &self,
        owner: AcceleratorDeviceKey,
        pre_pow_hash: [u8; 32],
        nonces: &[u64],
    ) -> Result<Vec<[u8; 32]>> {
        match owner.backend {
            AcceleratorBackendKind::Cuda => {
                self.cuda
                    .launch_mixed_runtime_batch(owner.device_index, pre_pow_hash, nonces)
            }
            AcceleratorBackendKind::OpenCl => {
                self.opencl
                    .launch_mixed_runtime_batch(owner.device_index, pre_pow_hash, nonces)
            }
        }
    }
}

#[derive(Debug, Default)]
struct MixedWorkerHealth {
    unavailable: Mutex<BTreeSet<AcceleratorDeviceKey>>,
}

impl MixedWorkerHealth {
    fn selected_unavailable(
        &self,
        selected: &[AcceleratorDeviceKey],
    ) -> Result<BTreeSet<AcceleratorDeviceKey>> {
        let unavailable = self
            .unavailable
            .lock()
            .map_err(|_| anyhow!("mixed worker health mutex poisoned"))?;
        Ok(unavailable
            .iter()
            .copied()
            .filter(|device| selected.contains(device))
            .collect())
    }

    fn mark_unavailable(&self, device: AcceleratorDeviceKey) -> Result<()> {
        self.unavailable
            .lock()
            .map_err(|_| anyhow!("mixed worker health mutex poisoned"))?
            .insert(device);
        Ok(())
    }

    fn mark_recovered(&self, device: AcceleratorDeviceKey) -> Result<()> {
        self.unavailable
            .lock()
            .map_err(|_| anyhow!("mixed worker health mutex poisoned"))?
            .remove(&device);
        Ok(())
    }
}

fn recover_unavailable_workers(
    devices: &[AcceleratorDeviceKey],
    launcher: &dyn MixedBatchLauncher,
    worker_health: &MixedWorkerHealth,
) -> Result<()> {
    for device in worker_health.selected_unavailable(devices)? {
        if launcher.probe(device).is_ok() {
            worker_health.mark_recovered(device)?;
        }
    }
    Ok(())
}

pub(crate) struct HeterogeneousMiningBackend {
    devices: Vec<AcceleratorDeviceKey>,
    launcher: Arc<dyn MixedBatchLauncher>,
    worker_health: MixedWorkerHealth,
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
        Ok(Self {
            devices,
            launcher,
            worker_health: MixedWorkerHealth::default(),
        })
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
        recover_unavailable_workers(&self.devices, self.launcher.as_ref(), &self.worker_health)?;
        self.search_work(header, work, max_tries)
    }

    fn search_work(
        &self,
        header: BlockHeader,
        work: ProtocolPowWork,
        max_tries: u64,
    ) -> Result<NonceSearchResult> {
        let max_tries = max_tries.max(1);
        let mut schedule = AcceleratorSchedule::build(&self.devices, max_tries)?;
        for unavailable in self.worker_health.selected_unavailable(&self.devices)? {
            schedule = schedule.redistribute_failed_device(unavailable).map_err(|err| {
                anyhow!(
                    "mixed worker health excludes {:?}[{}] but deterministic redistribution failed: {err}",
                    unavailable.backend,
                    unavailable.device_index
                )
            })?;
        }
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

                let owner = schedule.lanes()[lane_index].owner;
                let partition = schedule.lanes()[lane_index].partition;
                let batch_size = self.launcher.batch_size(owner)?;
                if batch_size == 0 {
                    return Err(anyhow!(
                        "mixed runtime batch size must be non-zero for {:?}[{}]",
                        owner.backend,
                        owner.device_index
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

                let initial_owner = owner;
                let mut hashes = Vec::with_capacity(nonces.len());
                let mut replay_offset = 0usize;
                while replay_offset < nonces.len() {
                    let owner = schedule.lanes()[lane_index].owner;
                    let owner_batch_size = self.launcher.batch_size(owner)?;
                    if owner_batch_size == 0 {
                        return Err(anyhow!(
                            "mixed runtime batch size must be non-zero for {:?}[{}]",
                            owner.backend,
                            owner.device_index
                        ));
                    }
                    let replay_end = replay_offset
                        .saturating_add(owner_batch_size)
                        .min(nonces.len());
                    let replay_nonces = &nonces[replay_offset..replay_end];

                    match self.launcher.launch(owner, pre_pow_hash, replay_nonces) {
                        Ok(replay_hashes) => {
                            if replay_hashes.len() != replay_nonces.len() {
                                return Err(anyhow!(
                                    "mixed runtime {:?}[{}] returned {} hash(es) for {} replay nonce(s); refusing incomplete accelerator result",
                                    owner.backend,
                                    owner.device_index,
                                    replay_hashes.len(),
                                    replay_nonces.len()
                                ));
                            }
                            hashes.extend(replay_hashes);
                            replay_offset = replay_end;
                        }
                        Err(launch_error) => {
                            self.worker_health.mark_unavailable(owner)?;
                            schedule = schedule.redistribute_failed_device(owner).map_err(
                                |redistribution_error| {
                                    anyhow!(
                                        "mixed runtime {:?}[{}] launch failed (batch initial owner {:?}[{}]): {launch_error}; deterministic device redistribution failed: {redistribution_error}",
                                        owner.backend,
                                        owner.device_index,
                                        initial_owner.backend,
                                        initial_owner.device_index
                                    )
                                },
                            )?;
                        }
                    }
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
                    if !compare_pow_hash_to_target(&accelerator_hash, &work.material.target.target)
                    {
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

    struct FailOnceMixedLauncher {
        hashes: BTreeMap<u64, [u8; 32]>,
        calls: Mutex<Vec<LaunchCall>>,
        probes: Mutex<Vec<AcceleratorDeviceKey>>,
        batch_size: usize,
        failed_owner: AcceleratorDeviceKey,
        failed: Mutex<bool>,
        recovery_probe_succeeds: bool,
    }

    impl FailOnceMixedLauncher {
        fn calls(&self) -> Vec<LaunchCall> {
            self.calls.lock().unwrap().clone()
        }

        fn probes(&self) -> Vec<AcceleratorDeviceKey> {
            self.probes.lock().unwrap().clone()
        }
    }

    impl MixedBatchLauncher for FailOnceMixedLauncher {
        fn batch_size(&self, _owner: AcceleratorDeviceKey) -> Result<usize> {
            Ok(self.batch_size)
        }

        fn probe(&self, owner: AcceleratorDeviceKey) -> Result<()> {
            self.probes.lock().unwrap().push(owner);
            if self.recovery_probe_succeeds {
                Ok(())
            } else {
                Err(anyhow!("injected mixed recovery probe failure"))
            }
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
            let mut failed = self.failed.lock().unwrap();
            if owner == self.failed_owner && !*failed {
                *failed = true;
                return Err(anyhow!("injected mixed device launch failure"));
            }
            drop(failed);
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes.get(nonce).copied().ok_or_else(|| {
                        anyhow!("fail-once mixed launcher has no hash for nonce {nonce}")
                    })
                })
                .collect()
        }
    }

    #[test]
    fn mixed_runtime_replays_failed_device_batch_on_survivor_without_nonce_loss() {
        let target_bits = 0x0300_0001;
        let header = header(target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));

        let failed_owner = AcceleratorDeviceKey::cuda(1);
        let survivor = AcceleratorDeviceKey::opencl(4);
        let launcher = Arc::new(FailOnceMixedLauncher {
            hashes: (0..7).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            probes: Mutex::new(Vec::new()),
            batch_size: 2,
            failed_owner,
            failed: Mutex::new(false),
            recovery_probe_succeeds: true,
        });
        let backend = HeterogeneousMiningBackend::with_launcher(
            vec![failed_owner, survivor],
            launcher.clone(),
        )
        .unwrap();

        let result = backend.mine_header(header, 7, 1, target_bits).unwrap();
        assert!(!result.accepted);
        assert_eq!(result.tries, 7);

        let calls = launcher.calls();
        assert_eq!(calls[0].owner, failed_owner);
        assert_eq!(calls[0].nonces, vec![0, 2]);
        assert_eq!(calls[1].owner, survivor);
        assert_eq!(calls[1].nonces, vec![0, 2]);
        assert!(calls[1..].iter().all(|call| call.owner == survivor));

        let mut successful_nonces = calls[1..]
            .iter()
            .flat_map(|call| call.nonces.iter().copied())
            .collect::<Vec<_>>();
        successful_nonces.sort_unstable();
        assert_eq!(successful_nonces, (0..7).collect::<Vec<_>>());
        assert!(calls
            .iter()
            .all(|call| call.pre_pow_hash == canonical_pre_pow_hash(&work)));
    }

    struct RebatchingMixedLauncher {
        hashes: BTreeMap<u64, [u8; 32]>,
        calls: Mutex<Vec<LaunchCall>>,
        batch_sizes: BTreeMap<AcceleratorDeviceKey, usize>,
        failed_owner: AcceleratorDeviceKey,
        failed: Mutex<bool>,
    }

    impl MixedBatchLauncher for RebatchingMixedLauncher {
        fn batch_size(&self, owner: AcceleratorDeviceKey) -> Result<usize> {
            self.batch_sizes
                .get(&owner)
                .copied()
                .ok_or_else(|| anyhow!("missing test batch size for {:?}", owner))
        }

        fn launch(
            &self,
            owner: AcceleratorDeviceKey,
            pre_pow_hash: [u8; 32],
            nonces: &[u64],
        ) -> Result<Vec<[u8; 32]>> {
            let batch_size = self.batch_size(owner)?;
            if nonces.len() > batch_size {
                return Err(anyhow!(
                    "test launcher rejected oversized batch: {} > {} for {:?}[{}]",
                    nonces.len(),
                    batch_size,
                    owner.backend,
                    owner.device_index
                ));
            }
            self.calls.lock().unwrap().push(LaunchCall {
                owner,
                nonces: nonces.to_vec(),
                pre_pow_hash,
            });
            let mut failed = self.failed.lock().unwrap();
            if owner == self.failed_owner && !*failed {
                *failed = true;
                return Err(anyhow!("injected mixed device launch failure"));
            }
            drop(failed);
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes
                        .get(nonce)
                        .copied()
                        .ok_or_else(|| anyhow!("rebatching launcher has no hash for nonce {nonce}"))
                })
                .collect()
        }
    }

    #[test]
    fn mixed_runtime_rebatches_failed_range_for_smaller_replacement_device() {
        let target_bits = 0x0300_0001;
        let rejected_hash = [0xffu8; 32];
        let failed_owner = AcceleratorDeviceKey::opencl(4);
        let survivor = AcceleratorDeviceKey::cuda(1);
        let launcher = Arc::new(RebatchingMixedLauncher {
            hashes: (0..10).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            batch_sizes: BTreeMap::from([(failed_owner, 4), (survivor, 2)]),
            failed_owner,
            failed: Mutex::new(false),
        });
        let backend = HeterogeneousMiningBackend::with_launcher(
            vec![survivor, failed_owner],
            launcher.clone(),
        )
        .unwrap();

        let result = backend
            .mine_header(header(target_bits), 10, 1, target_bits)
            .unwrap();
        assert!(!result.accepted);
        assert_eq!(result.tries, 10);

        let calls = launcher.calls.lock().unwrap().clone();
        let failed_call = calls
            .iter()
            .find(|call| call.owner == failed_owner)
            .expect("failed OpenCL owner must receive its canonical batch");
        assert_eq!(failed_call.nonces, vec![1, 3, 5, 7]);
        assert!(calls
            .iter()
            .filter(|call| call.owner == survivor)
            .all(|call| call.nonces.len() <= 2));

        let mut successful_nonces = calls
            .iter()
            .filter(|call| call.owner == survivor)
            .flat_map(|call| call.nonces.iter().copied())
            .collect::<Vec<_>>();
        successful_nonces.sort_unstable();
        assert_eq!(successful_nonces, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn mixed_runtime_failed_device_requires_probe_before_next_job_recovery() {
        let target_bits = 0x0300_0001;
        let rejected_hash = [0xffu8; 32];
        let failed_owner = AcceleratorDeviceKey::cuda(1);
        let survivor = AcceleratorDeviceKey::opencl(4);
        let launcher = Arc::new(FailOnceMixedLauncher {
            hashes: (0..7).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            probes: Mutex::new(Vec::new()),
            batch_size: 2,
            failed_owner,
            failed: Mutex::new(false),
            recovery_probe_succeeds: true,
        });
        let backend = HeterogeneousMiningBackend::with_launcher(
            vec![failed_owner, survivor],
            launcher.clone(),
        )
        .unwrap();

        backend
            .mine_header(header(target_bits), 7, 1, target_bits)
            .unwrap();
        let first_job_calls = launcher.calls().len();
        assert!(launcher.probes().is_empty());

        backend
            .mine_header(header(target_bits), 7, 1, target_bits)
            .unwrap();

        assert_eq!(launcher.probes(), vec![failed_owner]);
        let second_job_calls = launcher.calls();
        assert_eq!(second_job_calls[first_job_calls].owner, failed_owner);
    }

    #[test]
    fn mixed_runtime_failed_device_stays_quarantined_when_probe_fails() {
        let target_bits = 0x0300_0001;
        let rejected_hash = [0xffu8; 32];
        let failed_owner = AcceleratorDeviceKey::cuda(1);
        let survivor = AcceleratorDeviceKey::opencl(4);
        let launcher = Arc::new(FailOnceMixedLauncher {
            hashes: (0..7).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            probes: Mutex::new(Vec::new()),
            batch_size: 2,
            failed_owner,
            failed: Mutex::new(false),
            recovery_probe_succeeds: false,
        });
        let backend = HeterogeneousMiningBackend::with_launcher(
            vec![failed_owner, survivor],
            launcher.clone(),
        )
        .unwrap();

        backend
            .mine_header(header(target_bits), 7, 1, target_bits)
            .unwrap();
        let first_job_calls = launcher.calls().len();

        backend
            .mine_header(header(target_bits), 7, 1, target_bits)
            .unwrap();

        assert_eq!(launcher.probes(), vec![failed_owner]);
        assert!(launcher.calls()[first_job_calls..]
            .iter()
            .all(|call| call.owner == survivor));
    }

    struct AlwaysFailMixedLauncher {
        calls: Mutex<Vec<AcceleratorDeviceKey>>,
    }

    impl MixedBatchLauncher for AlwaysFailMixedLauncher {
        fn batch_size(&self, _owner: AcceleratorDeviceKey) -> Result<usize> {
            Ok(1)
        }

        fn launch(
            &self,
            owner: AcceleratorDeviceKey,
            _pre_pow_hash: [u8; 32],
            _nonces: &[u64],
        ) -> Result<Vec<[u8; 32]>> {
            self.calls.lock().unwrap().push(owner);
            Err(anyhow!("injected persistent mixed launch failure"))
        }
    }

    #[test]
    fn mixed_runtime_fails_closed_when_no_healthy_device_remains() {
        let target_bits = 0x0300_0001;
        let cuda = AcceleratorDeviceKey::cuda(1);
        let opencl = AcceleratorDeviceKey::opencl(4);
        let launcher = Arc::new(AlwaysFailMixedLauncher {
            calls: Mutex::new(Vec::new()),
        });
        let backend =
            HeterogeneousMiningBackend::with_launcher(vec![cuda, opencl], launcher.clone())
                .unwrap();

        let error = backend
            .mine_header(header(target_bits), 4, 1, target_bits)
            .unwrap_err()
            .to_string();

        assert!(error.contains("no healthy device remains"));
        let calls = launcher.calls.lock().unwrap().clone();
        assert_eq!(calls, vec![cuda, opencl]);
    }

    #[test]
    fn mixed_runtime_rejects_prefilter_hash_that_fails_canonical_reverification() {
        let target_bits = 0x207f_ffff;
        let header = header(target_bits);
        let launcher = Arc::new(FakeMixedLauncher {
            hashes: [(0u64, [0u8; 32]), (1u64, [0xffu8; 32])]
                .into_iter()
                .collect(),
            calls: Mutex::new(Vec::new()),
            batch_size: 1,
        });
        let backend = HeterogeneousMiningBackend::with_launcher(
            vec![
                AcceleratorDeviceKey::cuda(0),
                AcceleratorDeviceKey::opencl(0),
            ],
            launcher,
        )
        .unwrap();

        let error = backend
            .mine_header(header, 2, 1, target_bits)
            .expect_err("non-canonical accelerator hash must fail closed");
        assert!(error.to_string().contains("accelerator hash mismatch"));
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
