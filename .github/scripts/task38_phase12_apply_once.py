from pathlib import Path


LIB_PATH = Path("apps/pulsedag-miner/src/lib.rs")
MAIN_PATH = Path("apps/pulsedag-miner/src/main.rs")


def one_replace(source: str, old: str, new: str, label: str) -> str:
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return source.replace(old, new, 1)


def patch_lib() -> None:
    text = LIB_PATH.read_text()
    start_marker = '#[cfg(feature = "gpu")]\nmod opencl {'
    end_marker = '\npub fn miner_pow_preimage_bytes'
    start = text.index(start_marker)
    end = text.index(end_marker, start)
    module = text[start:end]

    module = one_replace(
        module,
        '    use super::OpenClDeviceSelection;',
        '    use super::{opencl_driver_launch, OpenClDeviceSelection};',
        'opencl sibling import',
    )
    module = one_replace(
        module,
        '    use std::os::raw::{c_int, c_uint, c_ulong, c_void};',
        '    use std::os::raw::{c_int, c_uint, c_void};',
        'OpenCL raw types import',
    )
    module = one_replace(
        module,
        '    type ClDeviceType = c_ulong;',
        '    type ClBitfield = u64;\n    type ClDeviceType = ClBitfield;',
        'OpenCL device type ABI',
    )
    module = one_replace(
        module,
        '    const CL_SUCCESS: ClInt = 0;',
        '    const CL_SUCCESS: ClInt = 0;\n    const CL_DEVICE_NOT_FOUND: ClInt = -1;',
        'OpenCL no-device status',
    )
    module = module.replace('unsafe extern "C" fn', 'unsafe extern "system" fn')
    if 'unsafe extern "C" fn' in module:
        raise SystemExit('OpenCL discovery still contains extern C ABI')

    api_marker = '    struct OpenClApi {'
    helper = '''    fn opencl_library_candidates(override_name: Option<String>) -> Vec<String> {
        if let Some(name) = override_name.filter(|value| !value.trim().is_empty()) {
            return vec![name];
        }

        if cfg!(target_os = "windows") {
            vec!["OpenCL.dll".to_string()]
        } else if cfg!(target_os = "macos") {
            vec!["/System/Library/Frameworks/OpenCL.framework/OpenCL".to_string()]
        } else {
            vec!["libOpenCL.so.1".to_string(), "libOpenCL.so".to_string()]
        }
    }

'''
    module = one_replace(module, api_marker, helper + api_marker, 'OpenCL candidate helper insertion')

    old_load = '''        fn load() -> Result<Self> {
            let names: &[&str] = if cfg!(target_os = "windows") {
                &["OpenCL.dll"]
            } else if cfg!(target_os = "macos") {
                &["/System/Library/Frameworks/OpenCL.framework/OpenCL"]
            } else {
                &["libOpenCL.so.1", "libOpenCL.so"]
            };

            let mut last_error = None;
            for name in names {
'''
    new_load = '''        fn load() -> Result<Self> {
            let names = opencl_library_candidates(
                std::env::var(opencl_driver_launch::OPENCL_LIBRARY_ENV).ok(),
            );

            let mut last_error = None;
            for name in &names {
'''
    module = one_replace(module, old_load, new_load, 'OpenCL loader candidates')

    old_status = '''            if status != CL_SUCCESS {
                return Ok(Vec::new());
            }
'''
    new_status = '''            if !device_count_status_is_available(status)? {
                return Ok(Vec::new());
            }
'''
    module = one_replace(module, old_status, new_status, 'OpenCL device count status')

    ensure_marker = '    fn ensure_opencl_success(status: ClInt, call: &str) -> Result<()> {'
    status_helper = '''    fn device_count_status_is_available(status: ClInt) -> Result<bool> {
        if status == CL_DEVICE_NOT_FOUND {
            return Ok(false);
        }
        ensure_opencl_success(status, "clGetDeviceIDs(count)")?;
        Ok(true)
    }

'''
    module = one_replace(module, ensure_marker, status_helper + ensure_marker, 'OpenCL status helper insertion')

    tests_block = '''    #[cfg(test)]
    mod tests {
        use super::{
            device_count_status_is_available, opencl_library_candidates, ClDeviceType,
            CL_DEVICE_NOT_FOUND, CL_SUCCESS,
        };

        #[test]
        fn explicit_library_override_is_exclusive() {
            let candidates =
                opencl_library_candidates(Some("/tmp/pulsedag-opencl-test.so".into()));
            assert_eq!(candidates, vec!["/tmp/pulsedag-opencl-test.so"]);
        }

        #[test]
        fn opencl_device_type_matches_64_bit_bitfield_abi() {
            assert_eq!(std::mem::size_of::<ClDeviceType>(), 8);
        }

        #[test]
        fn only_device_not_found_maps_to_empty_gpu_list() {
            assert!(!device_count_status_is_available(CL_DEVICE_NOT_FOUND).unwrap());
            assert!(device_count_status_is_available(CL_SUCCESS).unwrap());
            let err = device_count_status_is_available(-999).unwrap_err();
            assert!(err.to_string().contains("OpenCL status -999"));
        }
    }
'''
    final_close = module.rfind('\n}')
    if final_close == -1:
        raise SystemExit('OpenCL module closing brace not found')
    module = module[:final_close].rstrip() + '\n\n' + tests_block + module[final_close:]
    LIB_PATH.write_text(text[:start] + module + text[end:])


def patch_main() -> None:
    main = MAIN_PATH.read_text()
    main = one_replace(
        main,
        'The gpu backend is the existing optional OpenCL scaffold and requires the gpu feature.',
        'The gpu backend is the canonical OpenCL kHeavyHash backend and requires the gpu feature; explicit gpu selection fails closed on OpenCL discovery, runtime, build, launch, or canonical re-verification errors.',
        'CLI OpenCL backend description',
    )
    main = one_replace(
        main,
        '--gpu-device selects the single CUDA or OpenCL device for this slice.',
        '--gpu-device selects the global single CUDA or OpenCL GPU device index for this software slice.',
        'CLI GPU device description',
    )
    main = one_replace(
        main,
        'Auto preserves the existing OpenCL-then-CPU behavior when no CUDA module is supplied; when --cuda-module is supplied, auto tries CUDA first and falls through to the existing OpenCL attempt and then CPU if CUDA initialization or device selection fails.',
        'Auto tries OpenCL then CPU when no CUDA module is supplied; when --cuda-module is supplied, auto tries CUDA first, then OpenCL, then CPU if accelerator initialization or device selection fails.',
        'CLI auto backend description',
    )
    main = one_replace(
        main,
        ' The canonical kHeavyHash OpenCL kernel is not implemented yet.',
        '',
        'stale OpenCL not-implemented claim',
    )

    old_test = '''    #[test]
    fn usage_mentions_optional_gpu_and_explicit_cuda_backends() {
        let text = usage();

        assert!(text.contains("--backend cpu|gpu|cuda|auto"));
        assert!(text.contains("--cuda-module PATH"));
        assert!(text.contains("cuda feature"));
        assert!(text.contains("never falls back"));
        assert!(text.contains("falls through"));
    }
'''
    new_test = '''    #[test]
    fn usage_describes_canonical_opencl_and_explicit_cuda_backends() {
        let text = usage();

        assert!(text.contains("--backend cpu|gpu|cuda|auto"));
        assert!(text.contains("--cuda-module PATH"));
        assert!(text.contains("canonical OpenCL kHeavyHash backend"));
        assert!(text.contains("explicit gpu selection fails closed"));
        assert!(text.contains("cuda feature"));
        assert!(text.contains("never falls back"));
        assert!(text.contains("Physical NVIDIA/AMD validation is not claimed"));
        assert!(!text.contains("OpenCL scaffold"));
        assert!(!text.contains("not implemented yet"));
    }
'''
    main = one_replace(main, old_test, new_test, 'CLI usage regression test')
    MAIN_PATH.write_text(main)


patch_lib()
patch_main()
