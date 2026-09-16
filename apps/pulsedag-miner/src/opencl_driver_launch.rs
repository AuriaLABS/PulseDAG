use anyhow::{anyhow, Context, Result};
use libloading::Library;
use std::ffi::{c_char, c_void, CStr, CString};
use std::os::raw::{c_int, c_uint};

const CL_SUCCESS: c_int = 0;
const CL_DEVICE_NOT_FOUND: c_int = -1;
const CL_TRUE: c_uint = 1;
const CL_DEVICE_TYPE_GPU: u64 = 1 << 2;
const CL_DEVICE_EXTENSIONS: c_uint = 0x1030;
const CL_PROGRAM_BUILD_LOG: c_uint = 0x1183;
const CL_MEM_READ_WRITE: u64 = 1 << 0;
const CL_MEM_WRITE_ONLY: u64 = 1 << 1;
const CL_MEM_READ_ONLY: u64 = 1 << 2;
const HASH_BYTES: usize = 32;
const MATRIX_BYTES: usize = 64 * 64 * std::mem::size_of::<u16>();
const MATRIX_KERNEL_NAME: &[u8] = b"pulsedag_generate_matrix_kernel\0";
const HASH_KERNEL_NAME: &[u8] = b"pulsedag_kheavyhash_kernel\0";

pub const OPENCL_LIBRARY_ENV: &str = "PULSEDAG_OPENCL_LIBRARY";

type ClInt = c_int;
type ClUint = c_uint;
type ClBool = c_uint;
type ClBitfield = u64;
type ClDeviceType = ClBitfield;
type ClMemFlags = ClBitfield;
type ClPlatformId = *mut c_void;
type ClDeviceId = *mut c_void;
type ClContext = *mut c_void;
type ClCommandQueue = *mut c_void;
type ClProgram = *mut c_void;
type ClKernel = *mut c_void;
type ClMem = *mut c_void;
type ClEvent = *mut c_void;
type ClContextProperties = isize;
type ClDeviceInfo = ClUint;
type ClProgramBuildInfo = ClUint;

type ClContextNotify =
    Option<unsafe extern "system" fn(*const c_char, *const c_void, usize, *mut c_void)>;
type ClProgramNotify = Option<unsafe extern "system" fn(ClProgram, *mut c_void)>;

type ClGetPlatformIDs = unsafe extern "system" fn(ClUint, *mut ClPlatformId, *mut ClUint) -> ClInt;
type ClGetDeviceIDs = unsafe extern "system" fn(
    ClPlatformId,
    ClDeviceType,
    ClUint,
    *mut ClDeviceId,
    *mut ClUint,
) -> ClInt;
type ClGetDeviceInfo =
    unsafe extern "system" fn(ClDeviceId, ClDeviceInfo, usize, *mut c_void, *mut usize) -> ClInt;
type ClCreateContext = unsafe extern "system" fn(
    *const ClContextProperties,
    ClUint,
    *const ClDeviceId,
    ClContextNotify,
    *mut c_void,
    *mut ClInt,
) -> ClContext;
type ClReleaseContext = unsafe extern "system" fn(ClContext) -> ClInt;
type ClCreateCommandQueue =
    unsafe extern "system" fn(ClContext, ClDeviceId, ClBitfield, *mut ClInt) -> ClCommandQueue;
type ClReleaseCommandQueue = unsafe extern "system" fn(ClCommandQueue) -> ClInt;
type ClCreateProgramWithSource = unsafe extern "system" fn(
    ClContext,
    ClUint,
    *const *const c_char,
    *const usize,
    *mut ClInt,
) -> ClProgram;
type ClBuildProgram = unsafe extern "system" fn(
    ClProgram,
    ClUint,
    *const ClDeviceId,
    *const c_char,
    ClProgramNotify,
    *mut c_void,
) -> ClInt;
type ClGetProgramBuildInfo = unsafe extern "system" fn(
    ClProgram,
    ClDeviceId,
    ClProgramBuildInfo,
    usize,
    *mut c_void,
    *mut usize,
) -> ClInt;
type ClReleaseProgram = unsafe extern "system" fn(ClProgram) -> ClInt;
type ClCreateKernel = unsafe extern "system" fn(ClProgram, *const c_char, *mut ClInt) -> ClKernel;
type ClReleaseKernel = unsafe extern "system" fn(ClKernel) -> ClInt;
type ClCreateBuffer =
    unsafe extern "system" fn(ClContext, ClMemFlags, usize, *mut c_void, *mut ClInt) -> ClMem;
type ClReleaseMemObject = unsafe extern "system" fn(ClMem) -> ClInt;
type ClEnqueueWriteBuffer = unsafe extern "system" fn(
    ClCommandQueue,
    ClMem,
    ClBool,
    usize,
    usize,
    *const c_void,
    ClUint,
    *const ClEvent,
    *mut ClEvent,
) -> ClInt;
type ClEnqueueReadBuffer = unsafe extern "system" fn(
    ClCommandQueue,
    ClMem,
    ClBool,
    usize,
    usize,
    *mut c_void,
    ClUint,
    *const ClEvent,
    *mut ClEvent,
) -> ClInt;
type ClSetKernelArg = unsafe extern "system" fn(ClKernel, ClUint, usize, *const c_void) -> ClInt;
type ClEnqueueNDRangeKernel = unsafe extern "system" fn(
    ClCommandQueue,
    ClKernel,
    ClUint,
    *const usize,
    *const usize,
    *const usize,
    ClUint,
    *const ClEvent,
    *mut ClEvent,
) -> ClInt;
type ClFinish = unsafe extern "system" fn(ClCommandQueue) -> ClInt;

const SHARED_SOURCE: &str = include_str!("../opencl/kheavyhash_shared.h");
const MATRIX_SOURCE: &str = include_str!("../opencl/kheavyhash_matrix.cl");
const HASH_SOURCE: &str = include_str!("../opencl/kheavyhash.cl");

/// Return the exact OpenCL program source used by the runtime launcher.
/// Exposed so software-only CI can compile the same concatenated program without
/// requiring an OpenCL ICD or physical GPU.
pub fn canonical_opencl_program_source() -> String {
    fn without_shared_include(source: &str) -> String {
        source
            .lines()
            .filter(|line| line.trim() != "#include \"kheavyhash_shared.h\"")
            .collect::<Vec<_>>()
            .join("\n")
    }

    format!(
        "#define PULSEDAG_OPENCL_EMBEDDED_SHARED 1\n{SHARED_SOURCE}\n{}\n{}\n",
        without_shared_include(MATRIX_SOURCE),
        without_shared_include(HASH_SOURCE),
    )
}

/// Probe the configured OpenCL runtime and require the requested global GPU
/// device index plus cl_khr_fp64. No context/kernel execution is claimed here.
pub fn probe_opencl_gpu(device_index: usize) -> Result<()> {
    let api = OpenClApi::load()?;
    let device = select_gpu_device(&api, device_index)?;
    require_fp64(&api, device)
}

/// Execute canonical matrix generation followed by the kHeavyHash nonce batch.
/// This function returns accelerator hashes only. Callers must still target-
/// filter and re-verify every accepted candidate through ProtocolPowWork.
pub fn launch_kheavyhash_batch(
    device_index: usize,
    pre_pow_hash: [u8; HASH_BYTES],
    nonces: &[u64],
    work_size: usize,
) -> Result<Vec<[u8; HASH_BYTES]>> {
    if nonces.is_empty() {
        return Ok(Vec::new());
    }
    if work_size == 0 {
        return Err(anyhow!("OpenCL work size must be non-zero"));
    }

    let nonce_bytes = nonces
        .len()
        .checked_mul(std::mem::size_of::<u64>())
        .ok_or_else(|| anyhow!("OpenCL nonce buffer size overflow"))?;
    let output_bytes = nonces
        .len()
        .checked_mul(HASH_BYTES)
        .ok_or_else(|| anyhow!("OpenCL output buffer size overflow"))?;
    let global_size = nonces
        .len()
        .div_ceil(work_size)
        .checked_mul(work_size)
        .ok_or_else(|| anyhow!("OpenCL global work size overflow"))?;

    let api = OpenClApi::load()?;
    let device = select_gpu_device(&api, device_index)?;
    require_fp64(&api, device)?;

    let mut status = CL_SUCCESS;
    let context = unsafe {
        (api.cl_create_context)(
            std::ptr::null(),
            1,
            &device,
            None,
            std::ptr::null_mut(),
            &mut status,
        )
    };
    ensure_handle(context, status, "clCreateContext")?;

    let mut resources = Resources::new(&api, context);
    let execution = (|| -> Result<Vec<[u8; HASH_BYTES]>> {
        let queue = unsafe { (api.cl_create_command_queue)(context, device, 0, &mut status) };
        ensure_handle(queue, status, "clCreateCommandQueue")?;
        resources.queue = queue;

        let source = CString::new(canonical_opencl_program_source())
            .context("canonical OpenCL program source contained an interior NUL")?;
        let source_ptr = source.as_ptr();
        let source_len = source.as_bytes().len();
        let program = unsafe {
            (api.cl_create_program_with_source)(context, 1, &source_ptr, &source_len, &mut status)
        };
        ensure_handle(program, status, "clCreateProgramWithSource")?;
        resources.program = program;

        let build_options = CString::new("-Dcl_khr_fp64=1").expect("static build option");
        let build_status = unsafe {
            (api.cl_build_program)(
                program,
                1,
                &device,
                build_options.as_ptr(),
                None,
                std::ptr::null_mut(),
            )
        };
        if build_status != CL_SUCCESS {
            let log = program_build_log(&api, program, device)
                .unwrap_or_else(|error| format!("<build log unavailable: {error}>"));
            return Err(anyhow!(
                "clBuildProgram failed with OpenCL status {build_status}: {log}"
            ));
        }

        let matrix_kernel = create_kernel(&api, program, MATRIX_KERNEL_NAME, "matrix")?;
        resources.matrix_kernel = matrix_kernel;
        let hash_kernel = create_kernel(&api, program, HASH_KERNEL_NAME, "kheavyhash")?;
        resources.hash_kernel = hash_kernel;

        let pre_buffer =
            create_buffer(&api, context, CL_MEM_READ_ONLY, HASH_BYTES, "pre_pow_hash")?;
        resources.buffers.push(pre_buffer);
        let matrix_buffer =
            create_buffer(&api, context, CL_MEM_READ_WRITE, MATRIX_BYTES, "matrix")?;
        resources.buffers.push(matrix_buffer);
        let nonce_buffer = create_buffer(&api, context, CL_MEM_READ_ONLY, nonce_bytes, "nonces")?;
        resources.buffers.push(nonce_buffer);
        let output_buffer =
            create_buffer(&api, context, CL_MEM_WRITE_ONLY, output_bytes, "outputs")?;
        resources.buffers.push(output_buffer);

        enqueue_write(
            &api,
            queue,
            pre_buffer,
            pre_pow_hash.as_ptr().cast(),
            HASH_BYTES,
            "pre_pow_hash",
        )?;
        enqueue_write(
            &api,
            queue,
            nonce_buffer,
            nonces.as_ptr().cast(),
            nonce_bytes,
            "nonces",
        )?;

        set_mem_arg(&api, matrix_kernel, 0, &pre_buffer, "matrix.pre_pow_hash")?;
        set_mem_arg(&api, matrix_kernel, 1, &matrix_buffer, "matrix.output")?;
        let one = 1usize;
        ensure_opencl_success(
            unsafe {
                (api.cl_enqueue_nd_range_kernel)(
                    queue,
                    matrix_kernel,
                    1,
                    std::ptr::null(),
                    &one,
                    &one,
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            "clEnqueueNDRangeKernel(matrix)",
        )?;

        set_mem_arg(&api, hash_kernel, 0, &pre_buffer, "hash.pre_pow_hash")?;
        set_mem_arg(&api, hash_kernel, 1, &matrix_buffer, "hash.matrix")?;
        set_mem_arg(&api, hash_kernel, 2, &nonce_buffer, "hash.nonces")?;
        set_mem_arg(&api, hash_kernel, 3, &output_buffer, "hash.outputs")?;
        let count = u64::try_from(nonces.len())
            .map_err(|_| anyhow!("OpenCL nonce count does not fit in u64"))?;
        ensure_opencl_success(
            unsafe {
                (api.cl_set_kernel_arg)(
                    hash_kernel,
                    4,
                    std::mem::size_of::<u64>(),
                    (&count as *const u64).cast::<c_void>(),
                )
            },
            "clSetKernelArg(hash.count)",
        )?;
        ensure_opencl_success(
            unsafe {
                (api.cl_enqueue_nd_range_kernel)(
                    queue,
                    hash_kernel,
                    1,
                    std::ptr::null(),
                    &global_size,
                    &work_size,
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            "clEnqueueNDRangeKernel(kheavyhash)",
        )?;
        ensure_opencl_success(unsafe { (api.cl_finish)(queue) }, "clFinish")?;

        let mut raw = vec![0u8; output_bytes];
        ensure_opencl_success(
            unsafe {
                (api.cl_enqueue_read_buffer)(
                    queue,
                    output_buffer,
                    CL_TRUE,
                    0,
                    output_bytes,
                    raw.as_mut_ptr().cast::<c_void>(),
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            "clEnqueueReadBuffer(outputs)",
        )?;

        Ok(raw
            .chunks_exact(HASH_BYTES)
            .map(|chunk| {
                let mut hash = [0u8; HASH_BYTES];
                hash.copy_from_slice(chunk);
                hash
            })
            .collect())
    })();

    let cleanup = resources.cleanup();
    match (execution, cleanup) {
        (Ok(hashes), Ok(())) => Ok(hashes),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn select_gpu_device(api: &OpenClApi, requested_index: usize) -> Result<ClDeviceId> {
    let mut platform_count = 0;
    ensure_opencl_success(
        unsafe { (api.cl_get_platform_ids)(0, std::ptr::null_mut(), &mut platform_count) },
        "clGetPlatformIDs(count)",
    )?;
    if platform_count == 0 {
        return Err(anyhow!("no OpenCL platforms found"));
    }
    let mut platforms = vec![std::ptr::null_mut(); platform_count as usize];
    ensure_opencl_success(
        unsafe {
            (api.cl_get_platform_ids)(platform_count, platforms.as_mut_ptr(), std::ptr::null_mut())
        },
        "clGetPlatformIDs(list)",
    )?;

    let mut global_index = 0usize;
    for platform in platforms {
        let mut count = 0;
        let status = unsafe {
            (api.cl_get_device_ids)(
                platform,
                CL_DEVICE_TYPE_GPU,
                0,
                std::ptr::null_mut(),
                &mut count,
            )
        };
        if status == CL_DEVICE_NOT_FOUND {
            continue;
        }
        ensure_opencl_success(status, "clGetDeviceIDs(count)")?;
        let mut devices = vec![std::ptr::null_mut(); count as usize];
        if count > 0 {
            ensure_opencl_success(
                unsafe {
                    (api.cl_get_device_ids)(
                        platform,
                        CL_DEVICE_TYPE_GPU,
                        count,
                        devices.as_mut_ptr(),
                        std::ptr::null_mut(),
                    )
                },
                "clGetDeviceIDs(list)",
            )?;
        }
        for device in devices {
            if global_index == requested_index {
                return Ok(device);
            }
            global_index = global_index
                .checked_add(1)
                .ok_or_else(|| anyhow!("OpenCL global GPU index overflow"))?;
        }
    }

    Err(anyhow!(
        "OpenCL GPU device index {requested_index} was not found; discovered {global_index} GPU device(s)"
    ))
}

fn require_fp64(api: &OpenClApi, device: ClDeviceId) -> Result<()> {
    let extensions = device_info_string(api, device, CL_DEVICE_EXTENSIONS)?;
    if extensions
        .split_ascii_whitespace()
        .any(|item| item == "cl_khr_fp64")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "selected OpenCL GPU does not advertise cl_khr_fp64; canonical Matrix::generate rank semantics require f64 and this backend refuses a reduced-precision substitute"
        ))
    }
}

fn device_info_string(api: &OpenClApi, device: ClDeviceId, info: ClDeviceInfo) -> Result<String> {
    let mut size = 0usize;
    ensure_opencl_success(
        unsafe { (api.cl_get_device_info)(device, info, 0, std::ptr::null_mut(), &mut size) },
        "clGetDeviceInfo(size)",
    )?;
    let mut buffer = vec![0u8; size.max(1)];
    ensure_opencl_success(
        unsafe {
            (api.cl_get_device_info)(
                device,
                info,
                buffer.len(),
                buffer.as_mut_ptr().cast::<c_void>(),
                std::ptr::null_mut(),
            )
        },
        "clGetDeviceInfo(value)",
    )?;
    let value = CStr::from_bytes_until_nul(&buffer).unwrap_or(c"");
    Ok(value.to_string_lossy().into_owned())
}

fn program_build_log(api: &OpenClApi, program: ClProgram, device: ClDeviceId) -> Result<String> {
    let mut size = 0usize;
    ensure_opencl_success(
        unsafe {
            (api.cl_get_program_build_info)(
                program,
                device,
                CL_PROGRAM_BUILD_LOG,
                0,
                std::ptr::null_mut(),
                &mut size,
            )
        },
        "clGetProgramBuildInfo(size)",
    )?;
    let mut buffer = vec![0u8; size.max(1)];
    ensure_opencl_success(
        unsafe {
            (api.cl_get_program_build_info)(
                program,
                device,
                CL_PROGRAM_BUILD_LOG,
                buffer.len(),
                buffer.as_mut_ptr().cast::<c_void>(),
                std::ptr::null_mut(),
                &mut size,
            )
        },
        "clGetProgramBuildInfo(value)",
    )?;
    let value = CStr::from_bytes_until_nul(&buffer).unwrap_or(c"");
    Ok(value.to_string_lossy().into_owned())
}

fn create_kernel(
    api: &OpenClApi,
    program: ClProgram,
    name: &[u8],
    label: &str,
) -> Result<ClKernel> {
    let mut status = CL_SUCCESS;
    let kernel =
        unsafe { (api.cl_create_kernel)(program, name.as_ptr().cast::<c_char>(), &mut status) };
    ensure_handle(kernel, status, &format!("clCreateKernel({label})"))?;
    Ok(kernel)
}

fn create_buffer(
    api: &OpenClApi,
    context: ClContext,
    flags: ClMemFlags,
    size: usize,
    label: &str,
) -> Result<ClMem> {
    let mut status = CL_SUCCESS;
    let buffer =
        unsafe { (api.cl_create_buffer)(context, flags, size, std::ptr::null_mut(), &mut status) };
    ensure_handle(buffer, status, &format!("clCreateBuffer({label})"))?;
    Ok(buffer)
}

fn enqueue_write(
    api: &OpenClApi,
    queue: ClCommandQueue,
    buffer: ClMem,
    source: *const c_void,
    size: usize,
    label: &str,
) -> Result<()> {
    ensure_opencl_success(
        unsafe {
            (api.cl_enqueue_write_buffer)(
                queue,
                buffer,
                CL_TRUE,
                0,
                size,
                source,
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        },
        &format!("clEnqueueWriteBuffer({label})"),
    )
}

fn set_mem_arg(
    api: &OpenClApi,
    kernel: ClKernel,
    index: ClUint,
    memory: &ClMem,
    label: &str,
) -> Result<()> {
    ensure_opencl_success(
        unsafe {
            (api.cl_set_kernel_arg)(
                kernel,
                index,
                std::mem::size_of::<ClMem>(),
                (memory as *const ClMem).cast::<c_void>(),
            )
        },
        &format!("clSetKernelArg({label})"),
    )
}

fn ensure_opencl_success(status: ClInt, call: &str) -> Result<()> {
    if status == CL_SUCCESS {
        Ok(())
    } else {
        Err(anyhow!("{call} failed with OpenCL status {status}"))
    }
}

fn ensure_handle(handle: *mut c_void, status: ClInt, call: &str) -> Result<()> {
    ensure_opencl_success(status, call)?;
    if handle.is_null() {
        Err(anyhow!("{call} returned a null handle"))
    } else {
        Ok(())
    }
}

fn opencl_library_candidates() -> Vec<String> {
    if let Ok(override_path) = std::env::var(OPENCL_LIBRARY_ENV) {
        if !override_path.trim().is_empty() {
            return vec![override_path];
        }
    }
    if cfg!(target_os = "windows") {
        vec!["OpenCL.dll".to_string()]
    } else if cfg!(target_os = "macos") {
        vec!["/System/Library/Frameworks/OpenCL.framework/OpenCL".to_string()]
    } else if cfg!(target_os = "linux") {
        vec!["libOpenCL.so.1".to_string(), "libOpenCL.so".to_string()]
    } else {
        Vec::new()
    }
}

struct Resources<'a> {
    api: &'a OpenClApi,
    context: ClContext,
    queue: ClCommandQueue,
    program: ClProgram,
    matrix_kernel: ClKernel,
    hash_kernel: ClKernel,
    buffers: Vec<ClMem>,
    cleaned: bool,
}

impl<'a> Resources<'a> {
    fn new(api: &'a OpenClApi, context: ClContext) -> Self {
        Self {
            api,
            context,
            queue: std::ptr::null_mut(),
            program: std::ptr::null_mut(),
            matrix_kernel: std::ptr::null_mut(),
            hash_kernel: std::ptr::null_mut(),
            buffers: Vec::new(),
            cleaned: false,
        }
    }

    fn cleanup(&mut self) -> Result<()> {
        let mut first_error = None;
        for buffer in self.buffers.drain(..).rev() {
            record_cleanup(
                &mut first_error,
                unsafe { (self.api.cl_release_mem_object)(buffer) },
                "clReleaseMemObject",
            );
        }
        for (kernel, label) in [
            (self.hash_kernel, "clReleaseKernel(kheavyhash)"),
            (self.matrix_kernel, "clReleaseKernel(matrix)"),
        ] {
            if !kernel.is_null() {
                record_cleanup(
                    &mut first_error,
                    unsafe { (self.api.cl_release_kernel)(kernel) },
                    label,
                );
            }
        }
        self.hash_kernel = std::ptr::null_mut();
        self.matrix_kernel = std::ptr::null_mut();
        if !self.program.is_null() {
            record_cleanup(
                &mut first_error,
                unsafe { (self.api.cl_release_program)(self.program) },
                "clReleaseProgram",
            );
            self.program = std::ptr::null_mut();
        }
        if !self.queue.is_null() {
            record_cleanup(
                &mut first_error,
                unsafe { (self.api.cl_release_command_queue)(self.queue) },
                "clReleaseCommandQueue",
            );
            self.queue = std::ptr::null_mut();
        }
        if !self.context.is_null() {
            record_cleanup(
                &mut first_error,
                unsafe { (self.api.cl_release_context)(self.context) },
                "clReleaseContext",
            );
            self.context = std::ptr::null_mut();
        }
        self.cleaned = true;
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for Resources<'_> {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.cleanup();
        }
    }
}

fn record_cleanup(first_error: &mut Option<anyhow::Error>, status: ClInt, call: &str) {
    if status != CL_SUCCESS && first_error.is_none() {
        *first_error = Some(anyhow!("{call} failed with OpenCL status {status}"));
    }
}

struct OpenClApi {
    _library: Library,
    cl_get_platform_ids: ClGetPlatformIDs,
    cl_get_device_ids: ClGetDeviceIDs,
    cl_get_device_info: ClGetDeviceInfo,
    cl_create_context: ClCreateContext,
    cl_release_context: ClReleaseContext,
    cl_create_command_queue: ClCreateCommandQueue,
    cl_release_command_queue: ClReleaseCommandQueue,
    cl_create_program_with_source: ClCreateProgramWithSource,
    cl_build_program: ClBuildProgram,
    cl_get_program_build_info: ClGetProgramBuildInfo,
    cl_release_program: ClReleaseProgram,
    cl_create_kernel: ClCreateKernel,
    cl_release_kernel: ClReleaseKernel,
    cl_create_buffer: ClCreateBuffer,
    cl_release_mem_object: ClReleaseMemObject,
    cl_enqueue_write_buffer: ClEnqueueWriteBuffer,
    cl_enqueue_read_buffer: ClEnqueueReadBuffer,
    cl_set_kernel_arg: ClSetKernelArg,
    cl_enqueue_nd_range_kernel: ClEnqueueNDRangeKernel,
    cl_finish: ClFinish,
}

impl OpenClApi {
    fn load() -> Result<Self> {
        let candidates = opencl_library_candidates();
        if candidates.is_empty() {
            return Err(anyhow!("OpenCL is unsupported on this operating system"));
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
                    macro_rules! symbol {
                        ($ty:ty, $name:literal) => {
                            *library
                                .get::<$ty>(concat!($name, "\0").as_bytes())
                                .with_context(|| format!("OpenCL runtime missing {}", $name))?
                        };
                    }
                    Ok(Self {
                        cl_get_platform_ids: symbol!(ClGetPlatformIDs, "clGetPlatformIDs"),
                        cl_get_device_ids: symbol!(ClGetDeviceIDs, "clGetDeviceIDs"),
                        cl_get_device_info: symbol!(ClGetDeviceInfo, "clGetDeviceInfo"),
                        cl_create_context: symbol!(ClCreateContext, "clCreateContext"),
                        cl_release_context: symbol!(ClReleaseContext, "clReleaseContext"),
                        cl_create_command_queue: symbol!(
                            ClCreateCommandQueue,
                            "clCreateCommandQueue"
                        ),
                        cl_release_command_queue: symbol!(
                            ClReleaseCommandQueue,
                            "clReleaseCommandQueue"
                        ),
                        cl_create_program_with_source: symbol!(
                            ClCreateProgramWithSource,
                            "clCreateProgramWithSource"
                        ),
                        cl_build_program: symbol!(ClBuildProgram, "clBuildProgram"),
                        cl_get_program_build_info: symbol!(
                            ClGetProgramBuildInfo,
                            "clGetProgramBuildInfo"
                        ),
                        cl_release_program: symbol!(ClReleaseProgram, "clReleaseProgram"),
                        cl_create_kernel: symbol!(ClCreateKernel, "clCreateKernel"),
                        cl_release_kernel: symbol!(ClReleaseKernel, "clReleaseKernel"),
                        cl_create_buffer: symbol!(ClCreateBuffer, "clCreateBuffer"),
                        cl_release_mem_object: symbol!(ClReleaseMemObject, "clReleaseMemObject"),
                        cl_enqueue_write_buffer: symbol!(
                            ClEnqueueWriteBuffer,
                            "clEnqueueWriteBuffer"
                        ),
                        cl_enqueue_read_buffer: symbol!(ClEnqueueReadBuffer, "clEnqueueReadBuffer"),
                        cl_set_kernel_arg: symbol!(ClSetKernelArg, "clSetKernelArg"),
                        cl_enqueue_nd_range_kernel: symbol!(
                            ClEnqueueNDRangeKernel,
                            "clEnqueueNDRangeKernel"
                        ),
                        cl_finish: symbol!(ClFinish, "clFinish"),
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
            "OpenCL runtime library not found or incomplete ({})",
            last_error.unwrap_or_else(|| "no candidate library loaded".to_string())
        ))
    }
}
