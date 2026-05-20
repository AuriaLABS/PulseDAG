use anyhow::{anyhow, Result};
use pulsedag_core::pow::{
    canonical_pow_adapter, pow_accepts, pow_hash_score_u64, pow_preimage_bytes,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendVerification {
    pub accepted: bool,
    pub final_hash_hex: String,
    pub target_hex: String,
}

/// Internal abstraction for mining backends.
///
/// The default implementation is [`CpuMiningBackend`], which intentionally
/// delegates to the existing strided CPU mining path so current miner behavior
/// stays unchanged while leaving room for future backends.
pub trait MiningBackend: Send + Sync {
    fn name(&self) -> &'static str;

    fn mine_header(
        &self,
        header: BlockHeader,
        max_tries: u64,
        threads: usize,
        target_bits: u32,
    ) -> Result<NonceSearchResult>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuMiningBackend;

impl MiningBackend for CpuMiningBackend {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn mine_header(
        &self,
        header: BlockHeader,
        max_tries: u64,
        threads: usize,
        target_bits: u32,
    ) -> Result<NonceSearchResult> {
        mine_header_strided(header, max_tries, threads, target_bits)
    }
}

#[cfg(feature = "gpu")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuBackendConfig {
    pub device_index: Option<usize>,
    pub batch_size: u64,
    pub work_size: usize,
}

#[cfg(feature = "gpu")]
impl Default for GpuBackendConfig {
    fn default() -> Self {
        Self {
            device_index: None,
            batch_size: gpu_env_u64("PULSEDAG_MINER_GPU_BATCH_SIZE", 65_536),
            work_size: gpu_env_usize("PULSEDAG_MINER_GPU_WORK_SIZE", 256),
        }
    }
}

#[cfg(feature = "gpu")]
impl GpuBackendConfig {
    pub fn with_device_index(mut self, device_index: Option<usize>) -> Self {
        self.device_index = device_index;
        self
    }
}

#[cfg(feature = "gpu")]
fn gpu_env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(feature = "gpu")]
fn gpu_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(feature = "gpu")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenClDeviceSelection {
    pub platform_index: usize,
    pub device_index: usize,
    pub platform_name: String,
    pub device_name: String,
}

#[cfg(feature = "gpu")]
#[derive(Debug, Clone)]
pub struct GpuMiningBackend {
    config: GpuBackendConfig,
    selected_device: OpenClDeviceSelection,
}

#[cfg(feature = "gpu")]
impl GpuMiningBackend {
    pub fn new(config: GpuBackendConfig) -> Result<Self> {
        let selected_device = opencl::select_device(config.device_index).map_err(|err| {
            anyhow!(
                "OpenCL GPU backend initialization failed: {err}. Use --backend cpu to fall back to CPU mining."
            )
        })?;
        println!(
            "gpu_backend_opencl selected platform[{}]={} device[{}]={} batch_size={} work_size={} env_batch=PULSEDAG_MINER_GPU_BATCH_SIZE env_work=PULSEDAG_MINER_GPU_WORK_SIZE",
            selected_device.platform_index,
            selected_device.platform_name,
            selected_device.device_index,
            selected_device.device_name,
            config.batch_size,
            config.work_size,
        );
        Ok(Self {
            config,
            selected_device,
        })
    }

    pub fn config(&self) -> &GpuBackendConfig {
        &self.config
    }

    pub fn selected_device(&self) -> &OpenClDeviceSelection {
        &self.selected_device
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        config: GpuBackendConfig,
        selected_device: OpenClDeviceSelection,
    ) -> Self {
        Self {
            config,
            selected_device,
        }
    }
}

#[cfg(feature = "gpu")]
impl MiningBackend for GpuMiningBackend {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn mine_header(
        &self,
        header: BlockHeader,
        _max_tries: u64,
        _threads: usize,
        target_bits: u32,
    ) -> Result<NonceSearchResult> {
        // Build canonical nonce-independent material here so the GPU path cannot
        // accidentally invent a simplified header format. A future OpenCL kernel
        // must consume this exact adapter material and must still pass every
        // found nonce through `verify_backend_result_with_core` before submit.
        let mut canonical_header = header.clone();
        canonical_header.difficulty = target_bits;
        let material = canonical_pow_adapter()
            .pre_pow_material(&canonical_header)
            .map_err(|reason| anyhow!("invalid canonical PoW material: {}", reason.code()))?;
        Err(anyhow!(
            "OpenCL GPU backend selected platform[{}]={} device[{}]={}, but canonical kHeavyHash OpenCL mining is not implemented yet; refusing to mine with a non-canonical kernel. canonical_pre_pow_bytes={} target_hex={} batch_size={} work_size={}. Use --backend cpu to mine on the CPU.",
            self.selected_device.platform_index,
            self.selected_device.platform_name,
            self.selected_device.device_index,
            self.selected_device.device_name,
            material.pre_pow_bytes.len(),
            material.target.target_hex,
            self.config.batch_size,
            self.config.work_size,
        ))
    }
}

#[cfg(feature = "gpu")]
mod opencl {
    use super::OpenClDeviceSelection;
    use anyhow::{anyhow, Context, Result};
    use libloading::Library;
    use std::ffi::CStr;
    use std::os::raw::{c_int, c_uint, c_ulong, c_void};

    type ClInt = c_int;
    type ClUint = c_uint;
    type ClPlatformId = *mut c_void;
    type ClDeviceId = *mut c_void;
    type ClDeviceType = c_ulong;
    type ClDeviceInfo = c_uint;
    type ClPlatformInfo = c_uint;

    const CL_SUCCESS: ClInt = 0;
    const CL_DEVICE_TYPE_GPU: ClDeviceType = 1 << 2;
    const CL_PLATFORM_NAME: ClPlatformInfo = 0x0902;
    const CL_DEVICE_NAME: ClDeviceInfo = 0x102B;

    type ClGetPlatformIDs = unsafe extern "C" fn(ClUint, *mut ClPlatformId, *mut ClUint) -> ClInt;
    type ClGetPlatformInfo =
        unsafe extern "C" fn(ClPlatformId, ClPlatformInfo, usize, *mut c_void, *mut usize) -> ClInt;
    type ClGetDeviceIDs = unsafe extern "C" fn(
        ClPlatformId,
        ClDeviceType,
        ClUint,
        *mut ClDeviceId,
        *mut ClUint,
    ) -> ClInt;
    type ClGetDeviceInfo =
        unsafe extern "C" fn(ClDeviceId, ClDeviceInfo, usize, *mut c_void, *mut usize) -> ClInt;

    pub fn select_device(requested_device_index: Option<usize>) -> Result<OpenClDeviceSelection> {
        let api = OpenClApi::load()?;
        let platforms = api.platforms()?;
        if platforms.is_empty() {
            return Err(anyhow!("no OpenCL platforms found"));
        }

        let mut global_device_index = 0usize;
        for (platform_index, platform) in platforms.iter().copied().enumerate() {
            let platform_name = api
                .platform_info_string(platform, CL_PLATFORM_NAME)
                .unwrap_or_else(|_| "<unknown platform>".to_string());
            let devices = api.gpu_devices(platform)?;
            for (platform_device_index, device) in devices.iter().copied().enumerate() {
                if requested_device_index.is_none_or(|idx| idx == global_device_index) {
                    let device_name = api
                        .device_info_string(device, CL_DEVICE_NAME)
                        .unwrap_or_else(|_| "<unknown GPU device>".to_string());
                    return Ok(OpenClDeviceSelection {
                        platform_index,
                        device_index: global_device_index,
                        platform_name,
                        device_name: format!(
                            "{} (platform_device_index={})",
                            device_name, platform_device_index
                        ),
                    });
                }
                global_device_index = global_device_index.saturating_add(1);
            }
        }

        match requested_device_index {
            Some(index) => Err(anyhow!(
                "OpenCL GPU device index {index} was not found; discovered {global_device_index} GPU device(s)"
            )),
            None => Err(anyhow!("no OpenCL GPU devices found")),
        }
    }

    struct OpenClApi {
        _library: Library,
        cl_get_platform_ids: ClGetPlatformIDs,
        cl_get_platform_info: ClGetPlatformInfo,
        cl_get_device_ids: ClGetDeviceIDs,
        cl_get_device_info: ClGetDeviceInfo,
    }

    impl OpenClApi {
        fn load() -> Result<Self> {
            let names: &[&str] = if cfg!(target_os = "windows") {
                &["OpenCL.dll"]
            } else if cfg!(target_os = "macos") {
                &["/System/Library/Frameworks/OpenCL.framework/OpenCL"]
            } else {
                &["libOpenCL.so.1", "libOpenCL.so"]
            };

            let mut last_error = None;
            for name in names {
                let library = match unsafe { Library::new(name) } {
                    Ok(library) => library,
                    Err(err) => {
                        last_error = Some(err.to_string());
                        continue;
                    }
                };

                let cl_get_platform_ids =
                    unsafe { *library.get::<ClGetPlatformIDs>(b"clGetPlatformIDs\0")? };
                let cl_get_platform_info =
                    unsafe { *library.get::<ClGetPlatformInfo>(b"clGetPlatformInfo\0")? };
                let cl_get_device_ids =
                    unsafe { *library.get::<ClGetDeviceIDs>(b"clGetDeviceIDs\0")? };
                let cl_get_device_info =
                    unsafe { *library.get::<ClGetDeviceInfo>(b"clGetDeviceInfo\0")? };

                return Ok(Self {
                    _library: library,
                    cl_get_platform_ids,
                    cl_get_platform_info,
                    cl_get_device_ids,
                    cl_get_device_info,
                });
            }

            Err(anyhow!(
                "OpenCL runtime library not found ({})",
                last_error.unwrap_or_else(|| "no candidate library names were tried".to_string())
            ))
        }

        fn platforms(&self) -> Result<Vec<ClPlatformId>> {
            let mut count = 0;
            let status = unsafe { (self.cl_get_platform_ids)(0, std::ptr::null_mut(), &mut count) };
            ensure_opencl_success(status, "clGetPlatformIDs(count)")?;
            let mut platforms = vec![std::ptr::null_mut(); count as usize];
            if count > 0 {
                let status = unsafe {
                    (self.cl_get_platform_ids)(count, platforms.as_mut_ptr(), std::ptr::null_mut())
                };
                ensure_opencl_success(status, "clGetPlatformIDs(list)")?;
            }
            Ok(platforms)
        }

        fn gpu_devices(&self, platform: ClPlatformId) -> Result<Vec<ClDeviceId>> {
            let mut count = 0;
            let status = unsafe {
                (self.cl_get_device_ids)(
                    platform,
                    CL_DEVICE_TYPE_GPU,
                    0,
                    std::ptr::null_mut(),
                    &mut count,
                )
            };
            if status != CL_SUCCESS {
                return Ok(Vec::new());
            }
            let mut devices = vec![std::ptr::null_mut(); count as usize];
            if count > 0 {
                let status = unsafe {
                    (self.cl_get_device_ids)(
                        platform,
                        CL_DEVICE_TYPE_GPU,
                        count,
                        devices.as_mut_ptr(),
                        std::ptr::null_mut(),
                    )
                };
                ensure_opencl_success(status, "clGetDeviceIDs(list)")?;
            }
            Ok(devices)
        }

        fn platform_info_string(
            &self,
            platform: ClPlatformId,
            info: ClPlatformInfo,
        ) -> Result<String> {
            let mut size = 0usize;
            let status = unsafe {
                (self.cl_get_platform_info)(platform, info, 0, std::ptr::null_mut(), &mut size)
            };
            ensure_opencl_success(status, "clGetPlatformInfo(size)")?;
            let mut buf = vec![0u8; size.max(1)];
            let status = unsafe {
                (self.cl_get_platform_info)(
                    platform,
                    info,
                    buf.len(),
                    buf.as_mut_ptr().cast::<c_void>(),
                    std::ptr::null_mut(),
                )
            };
            ensure_opencl_success(status, "clGetPlatformInfo(value)")?;
            c_string_from_buf(&buf).context("OpenCL platform name was not valid UTF-8")
        }

        fn device_info_string(&self, device: ClDeviceId, info: ClDeviceInfo) -> Result<String> {
            let mut size = 0usize;
            let status = unsafe {
                (self.cl_get_device_info)(device, info, 0, std::ptr::null_mut(), &mut size)
            };
            ensure_opencl_success(status, "clGetDeviceInfo(size)")?;
            let mut buf = vec![0u8; size.max(1)];
            let status = unsafe {
                (self.cl_get_device_info)(
                    device,
                    info,
                    buf.len(),
                    buf.as_mut_ptr().cast::<c_void>(),
                    std::ptr::null_mut(),
                )
            };
            ensure_opencl_success(status, "clGetDeviceInfo(value)")?;
            c_string_from_buf(&buf).context("OpenCL device name was not valid UTF-8")
        }
    }

    fn ensure_opencl_success(status: ClInt, call: &str) -> Result<()> {
        if status == CL_SUCCESS {
            Ok(())
        } else {
            Err(anyhow!("{call} failed with OpenCL status {status}"))
        }
    }

    fn c_string_from_buf(buf: &[u8]) -> Result<String> {
        let cstr = CStr::from_bytes_until_nul(buf).unwrap_or(c"");
        Ok(cstr.to_string_lossy().into_owned())
    }
}

pub fn miner_pow_preimage_bytes(header: &BlockHeader) -> Vec<u8> {
    pow_preimage_bytes(header)
}

pub fn miner_pow_hash_hex(header: &BlockHeader) -> String {
    canonical_pow_adapter()
        .evaluate_header(header)
        .map(|attempt| attempt.final_hash.hash_hex)
        .unwrap_or_default()
}

pub fn miner_pow_score_u64(header: &BlockHeader) -> u64 {
    pow_hash_score_u64(header)
}

pub fn miner_pow_accepts(header: &BlockHeader) -> bool {
    pow_accepts(header)
}

fn miner_pow_eval_at_target_bits(header: &BlockHeader, target_bits: u32) -> Result<(bool, String)> {
    let verification = verify_backend_result_with_core(header, target_bits)?;
    Ok((verification.accepted, verification.final_hash_hex))
}

pub fn miner_pow_accepts_target_bits(header: &BlockHeader, target_bits: u32) -> Result<bool> {
    Ok(verify_backend_result_with_core(header, target_bits)?.accepted)
}

/// Verifies a backend-proposed final header through the canonical CPU/core PoW adapter.
///
/// All mining backends, including future GPU implementations, must pass through
/// this gate before the miner submits a block to a node. The supplied
/// `target_bits` are applied to a verification clone so compact targets from
/// mining templates are checked consistently with the CPU/core adapter without
/// mutating the caller's header.
pub fn verify_backend_result_with_core(
    header: &BlockHeader,
    target_bits: u32,
) -> Result<BackendVerification> {
    if target_bits == 0 {
        return Err(anyhow!("invalid target bits: 0"));
    }

    let mut h = header.clone();
    h.difficulty = target_bits;
    let attempt = canonical_pow_adapter()
        .evaluate_header(&h)
        .map_err(|reason| anyhow!("invalid PoW header material: {}", reason.code()))?;

    Ok(BackendVerification {
        accepted: attempt.comparison.accepted(),
        final_hash_hex: attempt.final_hash.hash_hex,
        target_hex: pulsedag_core::pow::target_hex(&pulsedag_core::pow::target_from_bits(
            target_bits,
        )),
    })
}

/// Verifies that a backend search result is canonical before the caller submits it.
///
/// This helper is intentionally backend-agnostic so GPU result handling can be
/// tested without requiring OpenCL hardware: the backend's `accepted` flag is
/// treated only as a proposal, and the canonical CPU/core adapter is the source
/// of truth for the returned verification.
pub fn verify_backend_search_result(
    result: &NonceSearchResult,
    target_bits: u32,
) -> Result<BackendVerification> {
    verify_backend_result_with_core(&result.header, target_bits)
}

fn nonce_for_attempt(thread_id: usize, stride: usize, iteration: u64) -> u64 {
    thread_id as u64 + (stride as u64 * iteration)
}

pub fn mine_header_strided(
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
    target_bits: u32,
) -> Result<NonceSearchResult> {
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
        let thread_header = header.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut local_tries = 0u64;
            let mut iteration = 0u64;

            loop {
                let nonce = nonce_for_attempt(thread_id, effective_threads, iteration);
                if nonce >= max_tries {
                    break;
                }

                if found.load(Ordering::Relaxed) {
                    break;
                }

                let mut candidate = thread_header.clone();
                candidate.nonce = nonce;
                local_tries = local_tries.saturating_add(1);

                let (accepted, hash_hex) = miner_pow_eval_at_target_bits(&candidate, target_bits)?;
                if accepted {
                    let already_found = found.swap(true, Ordering::SeqCst);
                    if !already_found {
                        let mut guard = winner.lock().map_err(|_| {
                            anyhow!("winner mutex poisoned during candidate selection")
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
    let (_, fallback_hash) = miner_pow_eval_at_target_bits(&fallback_header, target_bits)?;
    Ok(NonceSearchResult {
        header: fallback_header,
        accepted: false,
        tries: total_tries.max(1),
        final_hash_hex: fallback_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        miner_pow_accepts, miner_pow_hash_hex, miner_pow_preimage_bytes, miner_pow_score_u64,
        nonce_for_attempt, verify_backend_result_with_core, verify_backend_search_result,
        CpuMiningBackend, MiningBackend, NonceSearchResult,
    };
    use pulsedag_core::pow::{
        canonical_pow_adapter, canonical_pow_engine, target_from_bits, target_hex, PowEngine,
    };
    use pulsedag_core::types::BlockHeader;

    #[test]
    fn worker_partitioning_is_non_overlapping_for_prefix_space() {
        let threads = 6usize;
        let samples_per_thread = 30u64;
        let mut seen = std::collections::BTreeSet::new();

        for tid in 0..threads {
            for i in 0..samples_per_thread {
                let n = nonce_for_attempt(tid, threads, i);
                assert!(seen.insert(n), "duplicate nonce generated in schedule: {n}");
            }
        }

        assert_eq!(seen.len(), (threads as u64 * samples_per_thread) as usize);
    }

    #[test]
    fn strided_schedule_is_deterministic_per_worker() {
        let threads = 4usize;
        let worker_two: Vec<u64> = (0..8).map(|i| nonce_for_attempt(2, threads, i)).collect();
        assert_eq!(worker_two, vec![2, 6, 10, 14, 18, 22, 26, 30]);
    }

    #[test]
    fn miner_and_core_compute_same_hash() {
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 7,
            difficulty: 0x1f00ffff,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        assert_eq!(
            miner_pow_hash_hex(&header),
            canonical_pow_engine().evaluate_header(&header).hash_hex
        );
        assert!(!miner_pow_preimage_bytes(&header).is_empty());
    }

    #[test]
    fn cpu_miner_and_canonical_adapter_evaluate_same_nonce() {
        let header = BlockHeader {
            version: 1,
            parents: vec!["b".into(), "a".into()],
            timestamp: 2,
            nonce: 11,
            difficulty: 0x207fffff,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 2,
            height: 2,
        };
        let adapter = canonical_pow_adapter();
        let attempt = adapter.evaluate_header(&header).expect("adapter attempt");

        assert_eq!(miner_pow_hash_hex(&header), attempt.final_hash.hash_hex);
        assert_eq!(miner_pow_score_u64(&header), attempt.final_hash.score_u64);
        assert_eq!(miner_pow_accepts(&header), attempt.comparison.accepted());
        assert_eq!(
            miner_pow_preimage_bytes(&header),
            attempt.material.pre_pow_bytes
        );
    }

    #[test]
    fn easy_target_finds_solution() {
        let target_bits = 0x207fffff;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let mined = super::mine_header_strided(header, 10_000, 4, target_bits)
            .expect("mining should succeed");
        assert!(mined.accepted);
    }

    #[test]
    fn cpu_backend_uses_strided_cpu_path() {
        let target_bits = 0x01000001;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let backend = CpuMiningBackend;

        let via_backend = backend
            .mine_header(header.clone(), 16, 1, target_bits)
            .expect("backend mining should run");
        let direct = super::mine_header_strided(header, 16, 1, target_bits)
            .expect("direct CPU mining should run");

        assert_eq!(backend.name(), "cpu");
        assert_eq!(via_backend.accepted, direct.accepted);
        assert_eq!(via_backend.tries, direct.tries);
        assert_eq!(via_backend.header.nonce, direct.header.nonce);
        assert_eq!(via_backend.final_hash_hex, direct.final_hash_hex);
    }

    #[test]
    fn cpu_backend_preserves_max_tries_floor() {
        let target_bits = 1;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };

        let mined = CpuMiningBackend
            .mine_header(header, 0, 4, target_bits)
            .expect("backend should normalize max_tries like CPU path");

        assert_eq!(mined.tries, 1);
        assert_eq!(mined.header.nonce, 0);
    }

    #[test]
    fn backend_verification_cpu_found_nonce_passes() {
        let target_bits = 0x207fffff;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let mined = CpuMiningBackend
            .mine_header(header, 10_000, 4, target_bits)
            .expect("CPU backend should find an easy-target nonce");

        let verification = verify_backend_result_with_core(&mined.header, target_bits)
            .expect("canonical verification should run");

        assert!(mined.accepted);
        assert!(verification.accepted);
        assert_eq!(verification.final_hash_hex, mined.final_hash_hex);
    }

    #[test]
    fn backend_verification_fake_backend_nonce_is_rejected() {
        let target_bits = 0x01000001;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };

        let verification = verify_backend_result_with_core(&header, target_bits)
            .expect("canonical verification should run");

        assert!(!verification.accepted);
    }

    #[test]
    fn backend_search_result_ignores_backend_accepted_flag_and_rechecks_cpu_core() {
        let target_bits = 0x01000001;
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let fake_gpu_result = NonceSearchResult {
            header,
            accepted: true,
            tries: 1,
            final_hash_hex: "fake".to_string(),
        };

        let verification = verify_backend_search_result(&fake_gpu_result, target_bits)
            .expect("canonical CPU/core verification should run");

        assert!(!verification.accepted);
        assert_ne!(verification.final_hash_hex, fake_gpu_result.final_hash_hex);
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_backend_scaffold_uses_canonical_material_and_refuses_fake_kernel() {
        use super::{GpuBackendConfig, GpuMiningBackend, OpenClDeviceSelection};

        let backend = GpuMiningBackend::for_test(
            GpuBackendConfig {
                device_index: Some(0),
                batch_size: 1024,
                work_size: 64,
            },
            OpenClDeviceSelection {
                platform_index: 0,
                device_index: 0,
                platform_name: "test-platform".to_string(),
                device_name: "test-device".to_string(),
            },
        );
        let target_bits = 0x207fffff;
        let header = BlockHeader {
            version: 1,
            parents: vec!["b".into(), "a".into()],
            timestamp: 2,
            nonce: 0,
            difficulty: target_bits,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 2,
            height: 2,
        };

        let err = backend
            .mine_header(header, 1, 1, target_bits)
            .expect_err("scaffold must not mine with a non-canonical kernel");
        let message = err.to_string();

        assert!(message.contains("canonical kHeavyHash OpenCL mining is not implemented"));
        assert!(message.contains("canonical_pre_pow_bytes="));
        assert!(message.contains("Use --backend cpu"));
    }

    #[test]
    fn invalid_target_fails_cleanly() {
        let header = BlockHeader {
            version: 1,
            parents: vec!["a".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: 1,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let err = super::miner_pow_accepts_target_bits(&header, 0).expect_err("must fail");
        assert!(err.to_string().contains("invalid target bits"));
        let _ = target_hex(&target_from_bits(0x1d00ffff));
    }
}
