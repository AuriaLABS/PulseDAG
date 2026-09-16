use crate::accelerator_scheduler_runtime::{AcceleratorDeviceKey, AcceleratorSchedule};
use crate::opencl_driver_launch;
use crate::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use crate::{GpuMiningBackend, NonceSearchResult};
use anyhow::{anyhow, Result};
use pulsedag_core::pow::compare_pow_hash_to_target;
use pulsedag_core::types::BlockHeader;
use pulsedag_core::ProtocolActivationIdentity;
use sha3::{Digest, Keccak256};
use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

trait OpenClBatchLauncher: Send + Sync {
    fn probe(&self, _device_index: usize) -> Result<()> {
        Ok(())
    }

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
    fn probe(&self, device_index: usize) -> Result<()> {
        opencl_driver_launch::probe_opencl_gpu(device_index)
    }

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

#[derive(Debug, Default)]
struct OpenClWorkerHealth {
    unavailable: Mutex<BTreeSet<usize>>,
}

impl OpenClWorkerHealth {
    fn selected_unavailable(&self, selected: &[usize]) -> Result<BTreeSet<usize>> {
        let unavailable = self
            .unavailable
            .lock()
            .map_err(|_| anyhow!("OpenCL worker health mutex poisoned"))?;
        Ok(unavailable
            .iter()
            .copied()
            .filter(|device_index| selected.contains(device_index))
            .collect())
    }

    fn mark_unavailable(&self, device_index: usize) -> Result<()> {
        self.unavailable
            .lock()
            .map_err(|_| anyhow!("OpenCL worker health mutex poisoned"))?
            .insert(device_index);
        Ok(())
    }

    fn mark_recovered(&self, device_index: usize) -> Result<()> {
        self.unavailable
            .lock()
            .map_err(|_| anyhow!("OpenCL worker health mutex poisoned"))?
            .remove(&device_index);
        Ok(())
    }
}

static PROCESS_OPENCL_WORKER_HEALTH: OnceLock<OpenClWorkerHealth> = OnceLock::new();

fn recover_unavailable_workers(
    selected_device_indices: &[usize],
    launcher: &dyn OpenClBatchLauncher,
    worker_health: &OpenClWorkerHealth,
) -> Result<()> {
    for device_index in worker_health.selected_unavailable(selected_device_indices)? {
        if launcher.probe(device_index).is_ok() {
            worker_health.mark_recovered(device_index)?;
        }
    }
    Ok(())
}

pub(crate) fn mine_canonical(
    backend: &GpuMiningBackend,
    header: BlockHeader,
    max_tries: u64,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<NonceSearchResult> {
    let worker_health = PROCESS_OPENCL_WORKER_HEALTH.get_or_init(OpenClWorkerHealth::default);
    mine_canonical_with_runtime(
        backend,
        header,
        max_tries,
        target_bits,
        identity,
        &DriverOpenClBatchLauncher,
        worker_health,
    )
}

#[cfg(test)]
fn mine_canonical_with_launcher(
    backend: &GpuMiningBackend,
    header: BlockHeader,
    max_tries: u64,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
    launcher: &dyn OpenClBatchLauncher,
) -> Result<NonceSearchResult> {
    let worker_health = OpenClWorkerHealth::default();
    mine_canonical_with_runtime(
        backend,
        header,
        max_tries,
        target_bits,
        identity,
        launcher,
        &worker_health,
    )
}

fn mine_canonical_with_runtime(
    backend: &GpuMiningBackend,
    header: BlockHeader,
    max_tries: u64,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
    launcher: &dyn OpenClBatchLauncher,
    worker_health: &OpenClWorkerHealth,
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

    let device_indices = backend
        .selected_devices()
        .iter()
        .map(|device| device.device_index)
        .collect::<Vec<_>>();
    recover_unavailable_workers(&device_indices, launcher, worker_health)?;
    search_work(
        header,
        work,
        max_tries,
        OpenClSearchRuntime {
            device_indices: &device_indices,
            batch_size,
            work_size: backend.config().work_size,
            launcher,
            worker_health,
        },
    )
}

struct OpenClSearchRuntime<'a> {
    device_indices: &'a [usize],
    batch_size: usize,
    work_size: usize,
    launcher: &'a dyn OpenClBatchLauncher,
    worker_health: &'a OpenClWorkerHealth,
}

fn search_work(
    header: BlockHeader,
    work: ProtocolPowWork,
    max_tries: u64,
    runtime: OpenClSearchRuntime<'_>,
) -> Result<NonceSearchResult> {
    let OpenClSearchRuntime {
        device_indices,
        batch_size,
        work_size,
        launcher,
        worker_health,
    } = runtime;
    if device_indices.is_empty() {
        return Err(anyhow!(
            "OpenCL multi-device search requires at least one device"
        ));
    }
    for (position, device_index) in device_indices.iter().enumerate() {
        if device_indices[..position].contains(device_index) {
            return Err(anyhow!(
                "OpenCL multi-device search contains duplicate device index {device_index}"
            ));
        }
    }

    let max_tries = max_tries.max(1);
    let devices = device_indices
        .iter()
        .copied()
        .map(AcceleratorDeviceKey::opencl)
        .collect::<Vec<_>>();
    let mut schedule = AcceleratorSchedule::build(&devices, max_tries)?;
    for unavailable in worker_health.selected_unavailable(device_indices)? {
        schedule = schedule
            .redistribute_failed_device(AcceleratorDeviceKey::opencl(unavailable))
            .map_err(|err| {
                anyhow!(
                    "OpenCL worker health excludes device {unavailable} but deterministic redistribution failed: {err}"
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

            let partition = schedule.lanes()[lane_index].partition;
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
                        "OpenCL nonce iteration overflow for canonical lane {}",
                        partition.lane()
                    )
                })?;
            }

            if nonces.is_empty() {
                exhausted[lane_index] = true;
                continue;
            }
            made_progress = true;

            let hashes = loop {
                let owner = schedule.lanes()[lane_index].owner;
                match launcher.launch(owner.device_index, pre_pow_hash, &nonces, work_size) {
                    Ok(hashes) => break hashes,
                    Err(launch_error) => {
                        worker_health.mark_unavailable(owner.device_index)?;
                        schedule = schedule.redistribute_failed_device(owner).map_err(
                            |redistribution_error| {
                                anyhow!(
                                    "OpenCL worker {} launch failed: {launch_error}; deterministic worker redistribution failed: {redistribution_error}",
                                    owner.device_index
                                )
                            },
                        )?;
                    }
                }
            };

            if hashes.len() != nonces.len() {
                return Err(anyhow!(
                    "OpenCL launcher returned {} hash(es) for {} nonce(s); refusing incomplete accelerator result",
                    hashes.len(),
                    nonces.len()
                ));
            }

            iterations[lane_index] = next_iteration;
            if lane_exhausted {
                exhausted[lane_index] = true;
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

        if !made_progress {
            break;
        }
    }

    let fallback_nonce = last_nonce.ok_or_else(|| {
        anyhow!("OpenCL canonical nonce partitions produced no work after normalization")
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

    struct FailOnceLauncher {
        hashes: BTreeMap<u64, [u8; 32]>,
        calls: Mutex<Vec<LaunchCall>>,
        fail_device: usize,
        failed: Mutex<bool>,
    }

    impl FailOnceLauncher {
        fn calls(&self) -> Vec<LaunchCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl OpenClBatchLauncher for FailOnceLauncher {
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
            let mut failed = self.failed.lock().unwrap();
            if device_index == self.fail_device && !*failed {
                *failed = true;
                return Err(anyhow!(
                    "injected OpenCL worker failure on device {device_index}"
                ));
            }
            drop(failed);
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes
                        .get(nonce)
                        .copied()
                        .ok_or_else(|| anyhow!("fail-once launcher has no hash for nonce {nonce}"))
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

    fn test_selection(device_index: usize) -> OpenClDeviceSelection {
        OpenClDeviceSelection {
            platform_index: 0,
            device_index,
            platform_name: "test-platform".to_string(),
            device_name: format!("test-device-{device_index}"),
        }
    }

    fn test_backend(device_index: usize, batch_size: u64, work_size: usize) -> GpuMiningBackend {
        GpuMiningBackend::for_test(
            GpuBackendConfig {
                device_index: Some(device_index),
                batch_size,
                work_size,
            },
            test_selection(device_index),
        )
    }

    fn test_backend_devices(
        device_indices: &[usize],
        batch_size: u64,
        work_size: usize,
    ) -> GpuMiningBackend {
        GpuMiningBackend::for_test_devices(
            GpuBackendConfig {
                device_index: None,
                batch_size,
                work_size,
            },
            device_indices.iter().copied().map(test_selection).collect(),
        )
    }

    #[test]
    fn multi_device_batches_cover_nonce_domain_exactly_once() {
        let target_bits = 0x0300_0001;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));
        let launcher = FakeLauncher {
            hashes: (0..7).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            error: None,
        };
        let backend = test_backend_devices(&[1, 3], 2, 64);

        let result =
            mine_canonical_with_launcher(&backend, header, 7, target_bits, None, &launcher)
                .unwrap();
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

        let mut observed = calls
            .iter()
            .flat_map(|call| call.nonces.iter().copied())
            .collect::<Vec<_>>();
        observed.sort_unstable();
        let expected: Vec<u64> = (0..7).collect();
        assert_eq!(observed, expected);
    }

    #[test]
    fn failed_worker_is_isolated_replays_batch_and_recovers_next_job() {
        let target_bits = 0x0300_0001;
        let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
        let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
        let rejected_hash = [0xffu8; 32];
        assert!(!compare_pow_hash_to_target(
            &rejected_hash,
            &work.material.target.target
        ));

        let launcher = FailOnceLauncher {
            hashes: (0..7).map(|nonce| (nonce, rejected_hash)).collect(),
            calls: Mutex::new(Vec::new()),
            fail_device: 1,
            failed: Mutex::new(false),
        };
        let backend = test_backend_devices(&[1, 3], 2, 64);
        let worker_health = OpenClWorkerHealth::default();

        let first = mine_canonical_with_runtime(
            &backend,
            header.clone(),
            7,
            target_bits,
            None,
            &launcher,
            &worker_health,
        )
        .unwrap();
        assert!(!first.accepted);
        assert_eq!(first.tries, 7);
        assert_eq!(
            worker_health.selected_unavailable(&[1, 3]).unwrap(),
            BTreeSet::from([1usize])
        );

        let first_calls = launcher.calls();
        assert_eq!(first_calls.len(), 5);
        assert_eq!(first_calls[0].device_index, 1);
        assert_eq!(first_calls[0].nonces, vec![0, 2]);
        assert_eq!(first_calls[1].device_index, 3);
        assert_eq!(first_calls[1].nonces, vec![0, 2]);
        assert!(first_calls[2..].iter().all(|call| call.device_index == 3));

        let first_call_count = first_calls.len();
        let second = mine_canonical_with_runtime(
            &backend,
            header,
            7,
            target_bits,
            None,
            &launcher,
            &worker_health,
        )
        .unwrap();
        assert!(!second.accepted);
        assert_eq!(second.tries, 7);
        assert!(worker_health
            .selected_unavailable(&[1, 3])
            .unwrap()
            .is_empty());

        let calls = launcher.calls();
        let second_calls = &calls[first_call_count..];
        assert_eq!(second_calls.len(), 4);
        assert_eq!(second_calls[0].device_index, 1);
        assert_eq!(second_calls[0].nonces, vec![0, 2]);
        assert_eq!(second_calls[1].device_index, 3);
        assert_eq!(second_calls[1].nonces, vec![1, 3]);
        assert_eq!(second_calls[2].device_index, 1);
        assert_eq!(second_calls[2].nonces, vec![4, 6]);
        assert_eq!(second_calls[3].device_index, 3);
        assert_eq!(second_calls[3].nonces, vec![5]);
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
