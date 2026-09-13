use anyhow::{anyhow, Context, Result};
use libloading::Library;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uint, c_void};

use crate::cuda_runtime::cuda_driver_library_names;

type CuResult = c_int;
type CuDevice = c_int;
type CuContextHandle = *mut c_void;
type CuModuleHandle = *mut c_void;
type CuFunctionHandle = *mut c_void;
type CuStreamHandle = *mut c_void;
type CuDevicePtr = u64;

const CUDA_SUCCESS: CuResult = 0;
const CUDA_SMOKE_KERNEL_NAME: &str = "pulsedag_cuda_runtime_smoke_kernel";
const CUDA_SMOKE_MAGIC: u64 = 0x5055_4c53_4544_4147;
const CUDA_SMOKE_INPUT: u64 = 0x0123_4567_89ab_cdef;
const CUDA_DRIVER_LIBRARY_ENV: &str = "PULSEDAG_CUDA_DRIVER_LIBRARY";

type CuInit = unsafe extern "system" fn(c_uint) -> CuResult;
type CuDeviceGetCount = unsafe extern "system" fn(*mut c_int) -> CuResult;
type CuDeviceGet = unsafe extern "system" fn(*mut CuDevice, c_int) -> CuResult;
type CuCtxCreate = unsafe extern "system" fn(*mut CuContextHandle, c_uint, CuDevice) -> CuResult;
type CuCtxDestroy = unsafe extern "system" fn(CuContextHandle) -> CuResult;
type CuModuleLoadData = unsafe extern "system" fn(*mut CuModuleHandle, *const c_void) -> CuResult;
type CuModuleUnload = unsafe extern "system" fn(CuModuleHandle) -> CuResult;
type CuModuleGetFunction =
    unsafe extern "system" fn(*mut CuFunctionHandle, CuModuleHandle, *const c_char) -> CuResult;
type CuMemAlloc = unsafe extern "system" fn(*mut CuDevicePtr, usize) -> CuResult;
type CuMemFree = unsafe extern "system" fn(CuDevicePtr) -> CuResult;
type CuMemcpyHtoD = unsafe extern "system" fn(CuDevicePtr, *const c_void, usize) -> CuResult;
type CuMemcpyDtoH = unsafe extern "system" fn(*mut c_void, CuDevicePtr, usize) -> CuResult;
type CuLaunchKernel = unsafe extern "system" fn(
    CuFunctionHandle,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    CuStreamHandle,
    *mut *mut c_void,
    *mut *mut c_void,
) -> CuResult;
type CuCtxSynchronize = unsafe extern "system" fn() -> CuResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDriverLaunchSmoke {
    pub device_index: usize,
    pub input: u64,
    pub output: u64,
}

pub fn cuda_smoke_expected_output(input: u64) -> u64 {
    input ^ CUDA_SMOKE_MAGIC
}

pub fn validate_cuda_smoke_ptx(ptx: &[u8]) -> Result<()> {
    if ptx.is_empty() {
        return Err(anyhow!("CUDA smoke PTX is empty"));
    }

    let text = std::str::from_utf8(ptx).context("CUDA smoke PTX is not valid UTF-8")?;
    if !text.contains(CUDA_SMOKE_KERNEL_NAME) {
        return Err(anyhow!(
            "CUDA smoke PTX does not export {CUDA_SMOKE_KERNEL_NAME}"
        ));
    }

    CString::new(ptx).context("CUDA smoke PTX contains an interior NUL byte")?;
    Ok(())
}

pub fn run_cuda_driver_launch_smoke(
    ptx: &[u8],
    requested_device_index: Option<usize>,
) -> Result<CudaDriverLaunchSmoke> {
    validate_cuda_smoke_ptx(ptx)?;

    let api = CudaLaunchDriverApi::load()?;
    ensure_cuda_success(unsafe { (api.cu_init)(0) }, "cuInit")?;

    let mut device_count = 0;
    ensure_cuda_success(
        unsafe { (api.cu_device_get_count)(&mut device_count) },
        "cuDeviceGetCount",
    )?;
    if device_count <= 0 {
        return Err(anyhow!("no NVIDIA CUDA devices available for launch smoke"));
    }

    let selected_index = requested_device_index.unwrap_or(0);
    let available = usize::try_from(device_count)
        .map_err(|_| anyhow!("CUDA device count does not fit in usize"))?;
    if selected_index >= available {
        return Err(anyhow!(
            "CUDA device index {selected_index} was not found; discovered {available} device(s)"
        ));
    }

    let ordinal = c_int::try_from(selected_index)
        .map_err(|_| anyhow!("CUDA device index does not fit c_int"))?;
    let mut device = 0;
    ensure_cuda_success(
        unsafe { (api.cu_device_get)(&mut device, ordinal) },
        "cuDeviceGet",
    )?;

    let mut context_handle = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe { (api.cu_ctx_create)(&mut context_handle, 0, device) },
        "cuCtxCreate_v2",
    )?;
    let mut context = CudaContextGuard {
        api: &api,
        handle: context_handle,
    };

    let ptx_cstring = CString::new(ptx).context("CUDA smoke PTX contains an interior NUL byte")?;
    let mut module_handle = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe {
            (api.cu_module_load_data)(&mut module_handle, ptx_cstring.as_ptr().cast::<c_void>())
        },
        "cuModuleLoadData",
    )?;
    let mut module = CudaModuleGuard {
        api: &api,
        handle: module_handle,
    };

    let kernel_name =
        CString::new(CUDA_SMOKE_KERNEL_NAME).expect("static CUDA kernel name contains no NUL");
    let mut function = std::ptr::null_mut();
    ensure_cuda_success(
        unsafe { (api.cu_module_get_function)(&mut function, module_handle, kernel_name.as_ptr()) },
        "cuModuleGetFunction",
    )?;

    let input = CUDA_SMOKE_INPUT;
    let mut input_device_ptr = 0;
    ensure_cuda_success(
        unsafe { (api.cu_mem_alloc)(&mut input_device_ptr, std::mem::size_of::<u64>()) },
        "cuMemAlloc_v2(input)",
    )?;
    let mut input_allocation = CudaDeviceAllocation {
        api: &api,
        ptr: input_device_ptr,
    };

    let mut output_device_ptr = 0;
    ensure_cuda_success(
        unsafe { (api.cu_mem_alloc)(&mut output_device_ptr, std::mem::size_of::<u64>()) },
        "cuMemAlloc_v2(output)",
    )?;
    let mut output_allocation = CudaDeviceAllocation {
        api: &api,
        ptr: output_device_ptr,
    };

    ensure_cuda_success(
        unsafe {
            (api.cu_memcpy_htod)(
                input_device_ptr,
                (&input as *const u64).cast::<c_void>(),
                std::mem::size_of::<u64>(),
            )
        },
        "cuMemcpyHtoD_v2",
    )?;

    let mut input_kernel_arg = input_device_ptr;
    let mut output_kernel_arg = output_device_ptr;
    let mut kernel_params = [
        (&mut input_kernel_arg as *mut CuDevicePtr).cast::<c_void>(),
        (&mut output_kernel_arg as *mut CuDevicePtr).cast::<c_void>(),
    ];

    ensure_cuda_success(
        unsafe {
            (api.cu_launch_kernel)(
                function,
                1,
                1,
                1,
                1,
                1,
                1,
                0,
                std::ptr::null_mut(),
                kernel_params.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        },
        "cuLaunchKernel",
    )?;
    ensure_cuda_success(unsafe { (api.cu_ctx_synchronize)() }, "cuCtxSynchronize")?;

    let mut output = 0u64;
    ensure_cuda_success(
        unsafe {
            (api.cu_memcpy_dtoh)(
                (&mut output as *mut u64).cast::<c_void>(),
                output_device_ptr,
                std::mem::size_of::<u64>(),
            )
        },
        "cuMemcpyDtoH_v2",
    )?;

    let expected = cuda_smoke_expected_output(input);
    if output != expected {
        return Err(anyhow!(
            "CUDA driver launch smoke output mismatch: expected {expected:#018x}, got {output:#018x}"
        ));
    }

    output_allocation.free()?;
    input_allocation.free()?;
    module.unload()?;
    context.destroy()?;

    Ok(CudaDriverLaunchSmoke {
        device_index: selected_index,
        input,
        output,
    })
}

struct CudaContextGuard<'a> {
    api: &'a CudaLaunchDriverApi,
    handle: CuContextHandle,
}

impl CudaContextGuard<'_> {
    fn destroy(&mut self) -> Result<()> {
        if self.handle.is_null() {
            return Ok(());
        }

        ensure_cuda_success(
            unsafe { (self.api.cu_ctx_destroy)(self.handle) },
            "cuCtxDestroy_v2",
        )?;
        self.handle = std::ptr::null_mut();
        Ok(())
    }
}

impl Drop for CudaContextGuard<'_> {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                (self.api.cu_ctx_destroy)(self.handle);
            }
        }
    }
}

struct CudaModuleGuard<'a> {
    api: &'a CudaLaunchDriverApi,
    handle: CuModuleHandle,
}

impl CudaModuleGuard<'_> {
    fn unload(&mut self) -> Result<()> {
        if self.handle.is_null() {
            return Ok(());
        }

        ensure_cuda_success(
            unsafe { (self.api.cu_module_unload)(self.handle) },
            "cuModuleUnload",
        )?;
        self.handle = std::ptr::null_mut();
        Ok(())
    }
}

impl Drop for CudaModuleGuard<'_> {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                (self.api.cu_module_unload)(self.handle);
            }
        }
    }
}

struct CudaDeviceAllocation<'a> {
    api: &'a CudaLaunchDriverApi,
    ptr: CuDevicePtr,
}

impl CudaDeviceAllocation<'_> {
    fn free(&mut self) -> Result<()> {
        if self.ptr == 0 {
            return Ok(());
        }

        ensure_cuda_success(unsafe { (self.api.cu_mem_free)(self.ptr) }, "cuMemFree_v2")?;
        self.ptr = 0;
        Ok(())
    }
}

impl Drop for CudaDeviceAllocation<'_> {
    fn drop(&mut self) {
        if self.ptr != 0 {
            unsafe {
                (self.api.cu_mem_free)(self.ptr);
            }
        }
    }
}

fn cuda_driver_library_candidates() -> Vec<String> {
    if let Ok(override_path) = std::env::var(CUDA_DRIVER_LIBRARY_ENV) {
        if !override_path.trim().is_empty() {
            return vec![override_path];
        }
    }

    cuda_driver_library_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

struct CudaLaunchDriverApi {
    _library: Library,
    cu_init: CuInit,
    cu_device_get_count: CuDeviceGetCount,
    cu_device_get: CuDeviceGet,
    cu_ctx_create: CuCtxCreate,
    cu_ctx_destroy: CuCtxDestroy,
    cu_module_load_data: CuModuleLoadData,
    cu_module_unload: CuModuleUnload,
    cu_module_get_function: CuModuleGetFunction,
    cu_mem_alloc: CuMemAlloc,
    cu_mem_free: CuMemFree,
    cu_memcpy_htod: CuMemcpyHtoD,
    cu_memcpy_dtoh: CuMemcpyDtoH,
    cu_launch_kernel: CuLaunchKernel,
    cu_ctx_synchronize: CuCtxSynchronize,
}

impl CudaLaunchDriverApi {
    fn load() -> Result<Self> {
        let names = cuda_driver_library_candidates();
        if names.is_empty() {
            return Err(anyhow!(
                "CUDA Driver API launch path is unsupported on this operating system"
            ));
        }

        let mut last_error = None;
        for name in names {
            let library = match unsafe { Library::new(&name) } {
                Ok(library) => library,
                Err(err) => {
                    last_error = Some(format!("{name}: {err}"));
                    continue;
                }
            };

            let loaded = unsafe { Self::load_from_library(library) };
            match loaded {
                Ok(api) => return Ok(api),
                Err(err) => {
                    last_error = Some(format!("{name}: {err:#}"));
                }
            }
        }

        Err(anyhow!(
            "CUDA driver runtime library with launch symbols not found ({})",
            last_error.unwrap_or_else(|| "no candidate library loaded".to_string())
        ))
    }

    unsafe fn load_from_library(library: Library) -> Result<Self> {
        let cu_init = *library
            .get::<CuInit>(b"cuInit\0")
            .context("CUDA Driver API missing cuInit")?;
        let cu_device_get_count = *library
            .get::<CuDeviceGetCount>(b"cuDeviceGetCount\0")
            .context("CUDA Driver API missing cuDeviceGetCount")?;
        let cu_device_get = *library
            .get::<CuDeviceGet>(b"cuDeviceGet\0")
            .context("CUDA Driver API missing cuDeviceGet")?;
        let cu_ctx_create = *library
            .get::<CuCtxCreate>(b"cuCtxCreate_v2\0")
            .context("CUDA Driver API missing cuCtxCreate_v2")?;
        let cu_ctx_destroy = *library
            .get::<CuCtxDestroy>(b"cuCtxDestroy_v2\0")
            .context("CUDA Driver API missing cuCtxDestroy_v2")?;
        let cu_module_load_data = *library
            .get::<CuModuleLoadData>(b"cuModuleLoadData\0")
            .context("CUDA Driver API missing cuModuleLoadData")?;
        let cu_module_unload = *library
            .get::<CuModuleUnload>(b"cuModuleUnload\0")
            .context("CUDA Driver API missing cuModuleUnload")?;
        let cu_module_get_function = *library
            .get::<CuModuleGetFunction>(b"cuModuleGetFunction\0")
            .context("CUDA Driver API missing cuModuleGetFunction")?;
        let cu_mem_alloc = *library
            .get::<CuMemAlloc>(b"cuMemAlloc_v2\0")
            .context("CUDA Driver API missing cuMemAlloc_v2")?;
        let cu_mem_free = *library
            .get::<CuMemFree>(b"cuMemFree_v2\0")
            .context("CUDA Driver API missing cuMemFree_v2")?;
        let cu_memcpy_htod = *library
            .get::<CuMemcpyHtoD>(b"cuMemcpyHtoD_v2\0")
            .context("CUDA Driver API missing cuMemcpyHtoD_v2")?;
        let cu_memcpy_dtoh = *library
            .get::<CuMemcpyDtoH>(b"cuMemcpyDtoH_v2\0")
            .context("CUDA Driver API missing cuMemcpyDtoH_v2")?;
        let cu_launch_kernel = *library
            .get::<CuLaunchKernel>(b"cuLaunchKernel\0")
            .context("CUDA Driver API missing cuLaunchKernel")?;
        let cu_ctx_synchronize = *library
            .get::<CuCtxSynchronize>(b"cuCtxSynchronize\0")
            .context("CUDA Driver API missing cuCtxSynchronize")?;

        Ok(Self {
            _library: library,
            cu_init,
            cu_device_get_count,
            cu_device_get,
            cu_ctx_create,
            cu_ctx_destroy,
            cu_module_load_data,
            cu_module_unload,
            cu_module_get_function,
            cu_mem_alloc,
            cu_mem_free,
            cu_memcpy_htod,
            cu_memcpy_dtoh,
            cu_launch_kernel,
            cu_ctx_synchronize,
        })
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

    const MOCK_SMOKE_PTX: &[u8] = b".visible .entry pulsedag_cuda_runtime_smoke_kernel() {}\n";

    #[test]
    fn smoke_expected_output_is_stable() {
        assert_eq!(
            cuda_smoke_expected_output(CUDA_SMOKE_INPUT),
            0x5176_0934_ccef_8ca8
        );
    }

    #[test]
    fn smoke_ptx_requires_exported_entry() {
        assert!(validate_cuda_smoke_ptx(b"").is_err());
        assert!(validate_cuda_smoke_ptx(b".version 8.0\n").is_err());
        assert!(validate_cuda_smoke_ptx(MOCK_SMOKE_PTX).is_ok());
    }

    #[test]
    fn smoke_ptx_rejects_interior_nul() {
        assert!(validate_cuda_smoke_ptx(
            b".visible .entry pulsedag_cuda_runtime_smoke_kernel() {}\0"
        )
        .is_err());
    }

    #[test]
    fn smoke_mem_free_failure_is_reported() {
        if std::env::var_os("PULSEDAG_TEST_CUDA_FAIL_MEM_FREE").is_none() {
            return;
        }
        let error = run_cuda_driver_launch_smoke(MOCK_SMOKE_PTX, Some(0)).unwrap_err();
        assert!(error.to_string().contains("cuMemFree_v2"));
    }

    #[test]
    fn smoke_module_unload_failure_is_reported() {
        if std::env::var_os("PULSEDAG_TEST_CUDA_FAIL_MODULE_UNLOAD").is_none() {
            return;
        }
        let error = run_cuda_driver_launch_smoke(MOCK_SMOKE_PTX, Some(0)).unwrap_err();
        assert!(error.to_string().contains("cuModuleUnload"));
    }

    #[test]
    fn smoke_context_destroy_failure_is_reported() {
        if std::env::var_os("PULSEDAG_TEST_CUDA_FAIL_CTX_DESTROY").is_none() {
            return;
        }
        let error = run_cuda_driver_launch_smoke(MOCK_SMOKE_PTX, Some(0)).unwrap_err();
        assert!(error.to_string().contains("cuCtxDestroy_v2"));
    }
}
