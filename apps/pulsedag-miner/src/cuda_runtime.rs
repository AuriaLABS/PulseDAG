use anyhow::{anyhow, Context, Result};
use libloading::Library;
use std::os::raw::{c_char, c_int, c_uint};

type CuResult = c_int;
type CuDevice = c_int;

const CUDA_SUCCESS: CuResult = 0;

type CuInit = unsafe extern "system" fn(c_uint) -> CuResult;
type CuDriverGetVersion = unsafe extern "system" fn(*mut c_int) -> CuResult;
type CuDeviceGetCount = unsafe extern "system" fn(*mut c_int) -> CuResult;
type CuDeviceGet = unsafe extern "system" fn(*mut CuDevice, c_int) -> CuResult;
type CuDeviceGetName = unsafe extern "system" fn(*mut c_char, c_int, CuDevice) -> CuResult;
type CuDeviceComputeCapability =
    unsafe extern "system" fn(*mut c_int, *mut c_int, CuDevice) -> CuResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDeviceInfo {
    pub index: usize,
    pub name: String,
    pub compute_major: i32,
    pub compute_minor: i32,
}

impl CudaDeviceInfo {
    pub fn compute_capability(&self) -> String {
        format!("{}.{}", self.compute_major, self.compute_minor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CudaDiscovery {
    pub driver_version: i32,
    pub devices: Vec<CudaDeviceInfo>,
}

impl CudaDiscovery {
    pub fn driver_version_display(&self) -> String {
        let major = self.driver_version / 1000;
        let minor = (self.driver_version % 1000) / 10;
        format!("{major}.{minor}")
    }
}

pub fn cuda_driver_library_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["nvcuda.dll"]
    } else if cfg!(target_os = "linux") {
        &["libcuda.so.1", "libcuda.so"]
    } else {
        &[]
    }
}

pub fn discover_cuda_devices() -> Result<CudaDiscovery> {
    let api = CudaDriverApi::load()?;
    ensure_cuda_success(unsafe { (api.cu_init)(0) }, "cuInit")?;

    let mut driver_version = 0;
    ensure_cuda_success(
        unsafe { (api.cu_driver_get_version)(&mut driver_version) },
        "cuDriverGetVersion",
    )?;

    let mut device_count = 0;
    ensure_cuda_success(
        unsafe { (api.cu_device_get_count)(&mut device_count) },
        "cuDeviceGetCount",
    )?;
    if device_count < 0 {
        return Err(anyhow!(
            "cuDeviceGetCount returned invalid negative count {device_count}"
        ));
    }

    let capacity = usize::try_from(device_count)
        .map_err(|_| anyhow!("CUDA device count does not fit in usize"))?;
    let mut devices = Vec::with_capacity(capacity);

    for ordinal in 0..device_count {
        let mut device = 0;
        ensure_cuda_success(
            unsafe { (api.cu_device_get)(&mut device, ordinal) },
            "cuDeviceGet",
        )?;

        let mut name_buf = [0 as c_char; 256];
        ensure_cuda_success(
            unsafe {
                (api.cu_device_get_name)(
                    name_buf.as_mut_ptr(),
                    c_int::try_from(name_buf.len()).expect("CUDA name buffer length fits c_int"),
                    device,
                )
            },
            "cuDeviceGetName",
        )?;
        let name_len = name_buf
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name_buf.len());
        let name_bytes = name_buf[..name_len]
            .iter()
            .map(|&byte| byte as u8)
            .collect::<Vec<_>>();
        let name = String::from_utf8_lossy(&name_bytes).into_owned();

        let mut compute_major = 0;
        let mut compute_minor = 0;
        ensure_cuda_success(
            unsafe {
                (api.cu_device_compute_capability)(&mut compute_major, &mut compute_minor, device)
            },
            "cuDeviceComputeCapability",
        )?;

        devices.push(CudaDeviceInfo {
            index: usize::try_from(ordinal)
                .map_err(|_| anyhow!("CUDA device ordinal does not fit in usize"))?,
            name,
            compute_major,
            compute_minor,
        });
    }

    Ok(CudaDiscovery {
        driver_version,
        devices,
    })
}

pub fn select_cuda_device(
    devices: &[CudaDeviceInfo],
    requested_index: Option<usize>,
) -> Result<&CudaDeviceInfo> {
    if devices.is_empty() {
        return Err(anyhow!("no NVIDIA CUDA devices discovered"));
    }

    let selected_index = requested_index.unwrap_or(0);
    devices
        .iter()
        .find(|device| device.index == selected_index)
        .ok_or_else(|| {
            anyhow!(
                "CUDA device index {selected_index} was not found; discovered {} device(s)",
                devices.len()
            )
        })
}

pub fn run_selection_self_test() -> Result<()> {
    let devices = vec![
        CudaDeviceInfo {
            index: 0,
            name: "mock-nvidia-0".to_string(),
            compute_major: 8,
            compute_minor: 6,
        },
        CudaDeviceInfo {
            index: 1,
            name: "mock-nvidia-1".to_string(),
            compute_major: 9,
            compute_minor: 0,
        },
    ];

    let default = select_cuda_device(&devices, None)?;
    if default.index != 0 {
        return Err(anyhow!("default CUDA selection was not device 0"));
    }

    let explicit = select_cuda_device(&devices, Some(1))?;
    if explicit.index != 1 || explicit.compute_capability() != "9.0" {
        return Err(anyhow!("explicit CUDA device selection mismatch"));
    }

    if select_cuda_device(&devices, Some(2)).is_ok() {
        return Err(anyhow!("missing CUDA device index unexpectedly selected"));
    }
    if select_cuda_device(&[], None).is_ok() {
        return Err(anyhow!("empty CUDA device list unexpectedly selected"));
    }

    Ok(())
}

struct CudaDriverApi {
    _library: Library,
    cu_init: CuInit,
    cu_driver_get_version: CuDriverGetVersion,
    cu_device_get_count: CuDeviceGetCount,
    cu_device_get: CuDeviceGet,
    cu_device_get_name: CuDeviceGetName,
    cu_device_compute_capability: CuDeviceComputeCapability,
}

impl CudaDriverApi {
    fn load() -> Result<Self> {
        let names = cuda_driver_library_names();
        if names.is_empty() {
            return Err(anyhow!(
                "CUDA Driver API is unsupported on this operating system"
            ));
        }

        let mut last_error = None;
        for name in names {
            let library = match unsafe { Library::new(name) } {
                Ok(library) => library,
                Err(err) => {
                    last_error = Some(format!("{name}: {err}"));
                    continue;
                }
            };

            let cu_init = unsafe {
                *library
                    .get::<CuInit>(b"cuInit\0")
                    .context("CUDA Driver API missing cuInit")?
            };
            let cu_driver_get_version = unsafe {
                *library
                    .get::<CuDriverGetVersion>(b"cuDriverGetVersion\0")
                    .context("CUDA Driver API missing cuDriverGetVersion")?
            };
            let cu_device_get_count = unsafe {
                *library
                    .get::<CuDeviceGetCount>(b"cuDeviceGetCount\0")
                    .context("CUDA Driver API missing cuDeviceGetCount")?
            };
            let cu_device_get = unsafe {
                *library
                    .get::<CuDeviceGet>(b"cuDeviceGet\0")
                    .context("CUDA Driver API missing cuDeviceGet")?
            };
            let cu_device_get_name = unsafe {
                *library
                    .get::<CuDeviceGetName>(b"cuDeviceGetName\0")
                    .context("CUDA Driver API missing cuDeviceGetName")?
            };
            let cu_device_compute_capability = unsafe {
                *library
                    .get::<CuDeviceComputeCapability>(b"cuDeviceComputeCapability\0")
                    .context("CUDA Driver API missing cuDeviceComputeCapability")?
            };

            return Ok(Self {
                _library: library,
                cu_init,
                cu_driver_get_version,
                cu_device_get_count,
                cu_device_get,
                cu_device_get_name,
                cu_device_compute_capability,
            });
        }

        Err(anyhow!(
            "CUDA driver runtime library not found ({})",
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

    fn mock_devices() -> Vec<CudaDeviceInfo> {
        vec![
            CudaDeviceInfo {
                index: 0,
                name: "first".to_string(),
                compute_major: 7,
                compute_minor: 5,
            },
            CudaDeviceInfo {
                index: 1,
                name: "second".to_string(),
                compute_major: 8,
                compute_minor: 9,
            },
        ]
    }

    #[test]
    fn default_selection_is_first_device() {
        let devices = mock_devices();
        assert_eq!(select_cuda_device(&devices, None).unwrap().index, 0);
    }

    #[test]
    fn explicit_selection_is_deterministic() {
        let devices = mock_devices();
        let selected = select_cuda_device(&devices, Some(1)).unwrap();
        assert_eq!(selected.name, "second");
        assert_eq!(selected.compute_capability(), "8.9");
    }

    #[test]
    fn invalid_selection_fails_closed() {
        let devices = mock_devices();
        assert!(select_cuda_device(&devices, Some(2)).is_err());
        assert!(select_cuda_device(&[], None).is_err());
    }

    #[test]
    fn supported_platform_has_driver_library_candidate() {
        if cfg!(target_os = "windows") || cfg!(target_os = "linux") {
            assert!(!cuda_driver_library_names().is_empty());
        }
    }
}
