from pathlib import Path

LIB = Path("apps/pulsedag-miner/src/lib.rs")
BACKEND = Path("apps/pulsedag-miner/src/opencl_backend.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


lib = LIB.read_text()
lib = replace_once(
    lib,
    "use std::sync::{Arc, Mutex};\n",
    "use std::sync::{Arc, Mutex};\n\nextern crate self as pulsedag_miner;\n",
    "crate self alias",
)
lib = replace_once(
    lib,
    '#[cfg(feature = "cuda")]\npub mod cuda_driver_launch;\n',
    '#[cfg(feature = "cuda")]\npub mod cuda_driver_launch;\n#[cfg(feature = "gpu")]\n#[path = "accelerator_scheduler.rs"]\nmod accelerator_scheduler_runtime;\n',
    "runtime scheduler module",
)
LIB.write_text(lib)

backend = BACKEND.read_text()
backend = replace_once(
    backend,
    "use crate::opencl_driver_launch;\nuse crate::protocol_backend::{protocol_nonce_lane_count, protocol_nonce_partition};\n",
    "use crate::accelerator_scheduler_runtime::{AcceleratorDeviceKey, AcceleratorSchedule};\nuse crate::opencl_driver_launch;\n",
    "scheduler imports",
)
backend = replace_once(
    backend,
    "use sha3::{Digest, Keccak256};\n",
    "use sha3::{Digest, Keccak256};\nuse std::collections::BTreeSet;\nuse std::sync::{Mutex, OnceLock};\n",
    "worker health imports",
)

trait_start = backend.index("trait OpenClBatchLauncher: Send + Sync {")
trait_end = backend.index("\n}\n\n#[derive(Debug, Default)]\nstruct DriverOpenClBatchLauncher;", trait_start) + 3
backend = backend[:trait_start] + '''trait OpenClBatchLauncher: Send + Sync {
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
''' + backend[trait_end:]

impl_marker = "impl OpenClBatchLauncher for DriverOpenClBatchLauncher {\n"
backend = replace_once(
    backend,
    impl_marker,
    impl_marker + '''    fn probe(&self, device_index: usize) -> Result<()> {
        opencl_driver_launch::probe_opencl_gpu(device_index)
    }

''',
    "driver probe",
)

mine_start = backend.index("pub(crate) fn mine_canonical(")
search_start = backend.index("fn search_work(", mine_start)
new_mine = r'''#[derive(Debug, Default)]
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
        &device_indices,
        batch_size,
        backend.config().work_size,
        launcher,
        worker_health,
    )
}

'''
backend = backend[:mine_start] + new_mine + backend[search_start:]

search_start = backend.index("fn search_work(")
canonical_start = backend.index("fn canonical_pre_pow_hash", search_start)
new_search = r'''fn search_work(
    header: BlockHeader,
    work: ProtocolPowWork,
    max_tries: u64,
    device_indices: &[usize],
    batch_size: usize,
    work_size: usize,
    launcher: &dyn OpenClBatchLauncher,
    worker_health: &OpenClWorkerHealth,
) -> Result<NonceSearchResult> {
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

'''
backend = backend[:search_start] + new_search + backend[canonical_start:]

helper_marker = "\n    fn header(version: u32, target_bits: u32) -> BlockHeader {"
helper_pos = backend.index(helper_marker)
fail_once = r'''

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
                return Err(anyhow!("injected OpenCL worker failure on device {device_index}"));
            }
            drop(failed);
            nonces
                .iter()
                .map(|nonce| {
                    self.hashes.get(nonce).copied().ok_or_else(|| {
                        anyhow!("fail-once launcher has no hash for nonce {nonce}")
                    })
                })
                .collect()
        }
    }
'''
backend = backend[:helper_pos] + fail_once + backend[helper_pos:]

test_marker = "\n    #[test]\n    fn zero_opencl_launch_shape_fails_closed() {"
test_pos = backend.index(test_marker)
recovery_test = r'''

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
        assert!(first_calls[2..]
            .iter()
            .all(|call| call.device_index == 3));

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
'''
backend = backend[:test_pos] + recovery_test + backend[test_pos:]

BACKEND.write_text(backend)
