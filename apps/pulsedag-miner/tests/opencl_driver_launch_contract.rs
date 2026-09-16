#![cfg(feature = "gpu")]

#[allow(dead_code)]
#[path = "../src/opencl_driver_launch.rs"]
mod opencl_driver_launch;

use opencl_driver_launch::{canonical_opencl_program_source, launch_kheavyhash_batch};

#[test]
fn canonical_program_contains_matrix_and_hash_kernels() {
    let source = canonical_opencl_program_source();
    assert!(source.contains("pulsedag_generate_matrix_kernel"));
    assert!(source.contains("pulsedag_kheavyhash_kernel"));
    assert!(source.contains("pd_kheavyhash_from_pre_pow_hash"));
    assert!(source.contains("pd_matrix_xoshiro_next"));
    assert!(source.contains("1.0e-9"));
    assert!(!source.contains("#include \"kheavyhash_shared.h\""));
}

#[test]
fn canonical_program_requires_fp64_rank_semantics() {
    let source = canonical_opencl_program_source();
    assert!(source.contains("#ifndef cl_khr_fp64"));
    assert!(source.contains("#pragma OPENCL EXTENSION cl_khr_fp64 : enable"));
    assert!(source.contains("double values[64][64]"));
}

#[test]
fn empty_batch_is_noop_without_loading_opencl() {
    let hashes = launch_kheavyhash_batch(usize::MAX, [0; 32], &[], 0).unwrap();
    assert!(hashes.is_empty());
}

#[test]
fn invalid_work_size_fails_before_loading_opencl() {
    let error = launch_kheavyhash_batch(0, [1; 32], &[0], 0).unwrap_err();
    assert_eq!(error.to_string(), "OpenCL work size must be non-zero");
}

#[test]
fn all_zero_pre_pow_hash_is_not_rejected_by_input_validation() {
    let error = launch_kheavyhash_batch(usize::MAX, [0; 32], &[0], 64).unwrap_err();
    assert_ne!(
        error.to_string(),
        "OpenCL pre_pow_hash must not be all-zero"
    );
}
