use anyhow::{anyhow, Context, Result};
use libloading::Library;
use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_uint};

const CUDA_SUCCESS: c_int = 0;
const CUDA_HASH_BYTES: usize = 32;
const CUDA_MATRIX_BYTES: usize = 64 * 64 * std::mem::size_of::<u16>();
const MATRIX_KERNEL_NAME: &[u8] = b"pulsedag_generate_matrix_kernel\0";
const HASH_KERNEL_NAME: &[u8] = b"pulsedag_kheavyhash_kernel\0";

pub const CUDA_DRIVER_LIBRARY_ENV: &str = "PULSEDAG_CUDA_DRIVER_LIBRARY";

type CuResult = c_int;
type CuDevice = c_int;
type CuContext = *mut c_void;
type CuModule = *mut c_void;
type CuFunction = *mut c_void;
type CuStream = *mut c_void;
type CuDevicePtr = u64;

type CuInit = unsafe extern "system" fn(c_uint) -> CuResult;
type CuDeviceGetCount = unsafe extern "system" fn(*mut c_int) -> CuResult;
type CuDeviceGet = unsafe extern "system" fn(*mut CuDevice, c_int) -> CuResult;
type CuCtxCreateV2 = unsafe extern "system" fn(*mut CuContext, c_uint, CuDevice) -> CuResult;
type CuCtxDestroyV2 = unsafe extern "system" fn(CuContext) -> CuResult;
type CuModuleLoadData = unsafe extern "system" fn(*mut CuModule, *const c_void) -> CuResult;
type CuModuleUnload = unsafe extern "system" fn(CuModule) -> CuResult;
type CuModuleGetFunction =
    unsafe extern "system" fn(*mut CuFunction, CuModule, *const c_char) -> CuResult;
type CuMemAllocV2 = unsafe extern "system" fn(*mut CuDevicePtr, usize) -> CuResult;
type CuMemFreeV2 = unsafe extern "system" fn(CuDevicePtr) -> CuResult;
type CuMemcpyHtoDV2 = unsafe extern "system" fn(CuDevicePtr, *const c_void, usize) -> CuResult;
type CuMemcpyDtoHV2 = unsafe extern "system" fn(*mut c_void, CuDevicePtr, usize) -> CuResult;
type CuLaunchKernel = unsafe extern "system" fn(
    CuFunction,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    CuStream,
    *mut *mut c_void,
    *mut *mut c_void,
) -> CuResult;
type CuCtxSynchronize = unsafe extern "system" fn() -> CuResult;

/// Execute the CUDA kHeavyHash matrix-generation and nonce-hash kernels through
/// the NVIDIA Driver API. This function only returns accelerator hashes; callers
/// must still re-verify every candidate through `ProtocolPowWork` before submit.
pub fn launch_kheavyhash_batch(
    module_image: &[u8],
    device_index: usize,
    pre_pow_hash: [u8; 32],
    nonces: &[u64],
    block_size: u32,
) -> Result<Vec<[u8; 32]>> {
    if nonces.is_empty() {
        return Ok(Vec::new());
    }
    if module_image.is_empty() {
        return Err(anyhow!("CUDA module image is empty"));
    }
    if block_size == 0 {
        return Err(anyhow!("CUDA block size must be non-zero"));
    }
    if pre_pow_hash == [0; 32] {
        return Err(anyhow!("CUDA pre_pow_hash must not be all-zero"));
    }

    let api = CudaDriverApi::load()?;
    ensure_cuda_success(unsafe { (api.cu_init)(0) }, "cuInit")?;

    let mut device_count = 0;
    ensure_cuda_success(
        unsafe { (api.cu_device_get_count)(&mut device_count) },
        "cuDeviceGetCount",
    )?;
    if device_count <= 0 {
        return Err(anyhow!("no NVIDIA CUDA devices discovered"));
    }
    let device_index_i32 = c_int::try_from(device_index)
        .map_err(|_| anyhow!("CUDA device index does not fit in c_int"))?;
    if device_index_i32 >= device_count {
        return Err(anyhow!(
            "CUDA device index {device_index} was not found; discovered {device_count} device(s)"
        ));
    }

    let mut device = 0;
    ensure_cuda_success(
        unsafe { (api.cu_device_get)(&mut device, device_index_i32) },
        "cuDeviceGet",
    )?;

    let mut context = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe { (api.cu_ctx_create_v2)(&mut context, 0, device) },
        "cuCtxCreate_v2",
    )?;

    let execution = launch_with_context(&api, module_image, pre_pow_hash, nonces, block_size);
    let destroy = ensure_cuda_success(
        unsafe { (api.cu_ctx_destroy_v2)(context) },
        "cuCtxDestroy_v2",
    );

    match (execution, destroy) {
        (Ok(hashes), Ok(())) => Ok(hashes),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn launch_with_context(
    api: &CudaDriverApi,
    module_image: &[u8],
    pre_pow_hash: [u8; 32],
    nonces: &[u64],
    block_size: u32,
) -> Result<Vec<[u8; 32]>> {
    let mut nul_terminated_image = Vec::with_capacity(module_image.len().saturating_add(1));
    nul_terminated_image.extend_from_slice(module_image);
    if nul_terminated_image.last().copied() != Some(0) {
        nul_terminated_image.push(0);
    }

    let mut module = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe {
            (api.cu_module_load_data)(&mut module, nul_terminated_image.as_ptr().cast::<c_void>())
        },
        "cuModuleLoadData",
    )?;

    let execution = launch_with_module(api, module, pre_pow_hash, nonces, block_size);
    let unload = ensure_cuda_success(unsafe { (api.cu_module_unload)(module) }, "cuModuleUnload");

    match (execution, unload) {
        (Ok(hashes), Ok(())) => Ok(hashes),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn launch_with_module(
    api: &CudaDriverApi,
    module: CuModule,
    pre_pow_hash: [u8; 32],
    nonces: &[u64],
    block_size: u32,
) -> Result<Vec<[u8; 32]>> {
    let mut matrix_function = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe {
            (api.cu_module_get_function)(
                &mut matrix_function,
                module,
                MATRIX_KERNEL_NAME.as_ptr().cast::<c_char>(),
            )
        },
        "cuModuleGetFunction(matrix)",
    )?;

    let mut hash_function = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe {
            (api.cu_module_get_function)(
                &mut hash_function,
                module,
                HASH_KERNEL_NAME.as_ptr().cast::<c_char>(),
            )
        },
        "cuModuleGetFunction(kheavyhash)",
    )?;

    let nonce_bytes = nonces
        .len()
        .checked_mul(std::mem::size_of::<u64>())
        .ok_or_else(|| anyhow!("CUDA nonce buffer size overflow"))?;
    let output_bytes = nonces
        .len()
        .checked_mul(CUDA_HASH_BYTES)
        .ok_or_else(|| anyhow!("CUDA output buffer size overflow"))?;

    let mut d_pre_pow_hash = 0;
    let mut d_matrix = 0;
    let mut d_nonces = 0;
    let mut d_outputs = 0;

    let execution = (|| -> Result<Vec<[u8; 32]>> {
        ensure_cuda_success(
            unsafe { (api.cu_mem_alloc_v2)(&mut d_pre_pow_hash, CUDA_HASH_BYTES) },
            "cuMemAlloc_v2(pre_pow_hash)",
        )?;
        ensure_cuda_success(
            unsafe { (api.cu_mem_alloc_v2)(&mut d_matrix, CUDA_MATRIX_BYTES) },
            "cuMemAlloc_v2(matrix)",
        )?;
        ensure_cuda_success(
            unsafe { (api.cu_mem_alloc_v2)(&mut d_nonces, nonce_bytes) },
            "cuMemAlloc_v2(nonces)",
        )?;
        ensure_cuda_success(
            unsafe { (api.cu_mem_alloc_v2)(&mut d_outputs, output_bytes) },
            "cuMemAlloc_v2(outputs)",
        )?;

        ensure_cuda_success(
            unsafe {
                (api.cu_memcpy_htod_v2)(
                    d_pre_pow_hash,
                    pre_pow_hash.as_ptr().cast::<c_void>(),
                    CUDA_HASH_BYTES,
                )
            },
            "cuMemcpyHtoD_v2(pre_pow_hash)",
        )?;
        ensure_cuda_success(
            unsafe {
                (api.cu_memcpy_htod_v2)(d_nonces, nonces.as_ptr().cast::<c_void>(), nonce_bytes)
            },
            "cuMemcpyHtoD_v2(nonces)",
        )?;

        let mut pre_arg = d_pre_pow_hash;
        let mut matrix_arg = d_matrix;
        let mut matrix_params = [
            (&mut pre_arg as *mut CuDevicePtr).cast::<c_void>(),
            (&mut matrix_arg as *mut CuDevicePtr).cast::<c_void>(),
        ];
        ensure_cuda_success(
            unsafe {
                (api.cu_launch_kernel)(
                    matrix_function,
                    1,
                    1,
                    1,
                    1,
                    1,
                    1,
                    0,
                    std::ptr::null_mut(),
                    matrix_params.as_mut_ptr(),
                    std::ptr::null_mut(),
                )
            },
            "cuLaunchKernel(matrix)",
        )?;

        let count = u64::try_from(nonces.len())
            .map_err(|_| anyhow!("CUDA nonce count does not fit in u64"))?;
        let grid_x_u64 = count.div_ceil(u64::from(block_size));
        let grid_x = c_uint::try_from(grid_x_u64)
            .map_err(|_| anyhow!("CUDA grid dimension does not fit in c_uint"))?;
        let mut nonce_arg = d_nonces;
        let mut output_arg = d_outputs;
        let mut count_arg = count;
        let mut hash_params = [
            (&mut pre_arg as *mut CuDevicePtr).cast::<c_void>(),
            (&mut matrix_arg as *mut CuDevicePtr).cast::<c_void>(),
            (&mut nonce_arg as *mut CuDevicePtr).cast::<c_void>(),
            (&mut output_arg as *mut CuDevicePtr).cast::<c_void>(),
            (&mut count_arg as *mut u64).cast::<c_void>(),
        ];
        ensure_cuda_success(
            unsafe {
                (api.cu_launch_kernel)(
                    hash_function,
                    grid_x,
                    1,
                    1,
                    block_size,
                    1,
                    1,
                    0,
                    std::ptr::null_mut(),
                    hash_params.as_mut_ptr(),
                    std::ptr::null_mut(),
                )
            },
            "cuLaunchKernel(kheavyhash)",
        )?;
        ensure_cuda_success(unsafe { (api.cu_ctx_synchronize)() }, "cuCtxSynchronize")?;

        let mut raw = vec![0u8; output_bytes];
        ensure_cuda_success(
            unsafe {
                (api.cu_memcpy_dtoh_v2)(raw.as_mut_ptr().cast::<c_void>(), d_outputs, output_bytes)
            },
            "cuMemcpyDtoH_v2(outputs)",
        )?;

        Ok(raw
            .chunks_exact(CUDA_HASH_BYTES)
            .map(|chunk| {
                let mut hash = [0u8; CUDA_HASH_BYTES];
                hash.copy_from_slice(chunk);
                hash
            })
            .collect())
    })();

    for device_ptr in [d_outputs, d_nonces, d_matrix, d_pre_pow_hash] {
        if device_ptr != 0 {
            let _ = unsafe { (api.cu_mem_free_v2)(device_ptr) };
        }
    }

    execution
}

fn cuda_driver_library_candidates() -> Vec<String> {
    if let Ok(override_path) = std::env::var(CUDA_DRIVER_LIBRARY_ENV) {
        if !override_path.trim().is_empty() {
            return vec![override_path];
        }
    }

    if cfg!(target_os = "windows") {
        vec!["nvcuda.dll".to_string()]
    } else if cfg!(target_os = "linux") {
        vec!["libcuda.so.1".to_string(), "libcuda.so".to_string()]
    } else {
        Vec::new()
    }
}

struct CudaDriverApi {
    _library: Library,
    cu_init: CuInit,
    cu_device_get_count: CuDeviceGetCount,
    cu_device_get: CuDeviceGet,
    cu_ctx_create_v2: CuCtxCreateV2,
    cu_ctx_destroy_v2: CuCtxDestroyV2,
    cu_module_load_data: CuModuleLoadData,
    cu_module_unload: CuModuleUnload,
    cu_module_get_function: CuModuleGetFunction,
    cu_mem_alloc_v2: CuMemAllocV2,
    cu_mem_free_v2: CuMemFreeV2,
    cu_memcpy_htod_v2: CuMemcpyHtoDV2,
    cu_memcpy_dtoh_v2: CuMemcpyDtoHV2,
    cu_launch_kernel: CuLaunchKernel,
    cu_ctx_synchronize: CuCtxSynchronize,
}

impl CudaDriverApi {
    fn load() -> Result<Self> {
        let candidates = cuda_driver_library_candidates();
        if candidates.is_empty() {
            return Err(anyhow!(
                "CUDA Driver API is unsupported on this operating system"
            ));
        }

        let mut last_error = None;
        for candidate in candidates {
            let library = match unsafe { Library::new(&candidate) } {
                Ok(library) => library,
                Err(error) => {
                    last_error = Some(format!("{candidate}: {error}"));
                    continue;
                }
            };

            let loaded = unsafe {
                (|| -> Result<Self> {
                    Ok(Self {
                        cu_init: *library
                            .get::<CuInit>(b"cuInit\0")
                            .context("CUDA Driver API missing cuInit")?,
                        cu_device_get_count: *library
                            .get::<CuDeviceGetCount>(b"cuDeviceGetCount\0")
                            .context("CUDA Driver API missing cuDeviceGetCount")?,
                        cu_device_get: *library
                            .get::<CuDeviceGet>(b"cuDeviceGet\0")
                            .context("CUDA Driver API missing cuDeviceGet")?,
                        cu_ctx_create_v2: *library
                            .get::<CuCtxCreateV2>(b"cuCtxCreate_v2\0")
                            .context("CUDA Driver API missing cuCtxCreate_v2")?,
                        cu_ctx_destroy_v2: *library
                            .get::<CuCtxDestroyV2>(b"cuCtxDestroy_v2\0")
                            .context("CUDA Driver API missing cuCtxDestroy_v2")?,
                        cu_module_load_data: *library
                            .get::<CuModuleLoadData>(b"cuModuleLoadData\0")
                            .context("CUDA Driver API missing cuModuleLoadData")?,
                        cu_module_unload: *library
                            .get::<CuModuleUnload>(b"cuModuleUnload\0")
                            .context("CUDA Driver API missing cuModuleUnload")?,
                        cu_module_get_function: *library
                            .get::<CuModuleGetFunction>(b"cuModuleGetFunction\0")
                            .context("CUDA Driver API missing cuModuleGetFunction")?,
                        cu_mem_alloc_v2: *library
                            .get::<CuMemAllocV2>(b"cuMemAlloc_v2\0")
                            .context("CUDA Driver API missing cuMemAlloc_v2")?,
                        cu_mem_free_v2: *library
                            .get::<CuMemFreeV2>(b"cuMemFree_v2\0")
                            .context("CUDA Driver API missing cuMemFree_v2")?,
                        cu_memcpy_htod_v2: *library
                            .get::<CuMemcpyHtoDV2>(b"cuMemcpyHtoD_v2\0")
                            .context("CUDA Driver API missing cuMemcpyHtoD_v2")?,
                        cu_memcpy_dtoh_v2: *library
                            .get::<CuMemcpyDtoHV2>(b"cuMemcpyDtoH_v2\0")
                            .context("CUDA Driver API missing cuMemcpyDtoH_v2")?,
                        cu_launch_kernel: *library
                            .get::<CuLaunchKernel>(b"cuLaunchKernel\0")
                            .context("CUDA Driver API missing cuLaunchKernel")?,
                        cu_ctx_synchronize: *library
                            .get::<CuCtxSynchronize>(b"cuCtxSynchronize\0")
                            .context("CUDA Driver API missing cuCtxSynchronize")?,
                        _library: library,
                    })
                })()
            };

            match loaded {
                Ok(api) => return Ok(api),
                Err(error) => last_error = Some(format!("{candidate}: {error}")),
            }
        }

        Err(anyhow!(
            "CUDA driver runtime library not found or incomplete ({})",
            last_error.unwrap_or_else(|| "no candidate library loaded".to_string())
        ))
    }
}

fn ensure_cuda_success(status: CuResult, call: &str) -> Result<()> {
    if status == CUDA_SUCCESS {
        Ok(())
    } else {
        Err(anyhow!("{call} failed with CUDA status {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_batch_is_a_noop_without_loading_cuda() {
        assert_eq!(
            launch_kheavyhash_batch(b"ignored", 0, [0; 32], &[], 256).unwrap(),
            Vec::<[u8; 32]>::new()
        );
    }

    #[test]
    fn invalid_launch_shape_fails_before_loading_cuda() {
        assert!(launch_kheavyhash_batch(b"", 0, [0; 32], &[0], 256).is_err());
        assert!(launch_kheavyhash_batch(b"ptx", 0, [0; 32], &[0], 0).is_err());
    }

    #[test]
    fn all_zero_pre_pow_hash_fails_before_loading_cuda() {
        let error = launch_kheavyhash_batch(b"ptx", 0, [0; 32], &[0], 256).unwrap_err();
        assert_eq!(error.to_string(), "CUDA pre_pow_hash must not be all-zero");
    }
}
