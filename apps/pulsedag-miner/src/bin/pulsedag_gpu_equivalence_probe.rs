#[path = "../cuda_runtime.rs"]
mod cuda_runtime;

use anyhow::{anyhow, Context, Result};
use pulsedag_core::types::BlockHeader;
use pulsedag_core::{
    ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2,
    GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_miner::cuda_driver_launch::{self, CUDA_DRIVER_LIBRARY_ENV};
use pulsedag_miner::opencl_driver_launch::{self, OPENCL_LIBRARY_ENV};
use pulsedag_miner::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use sha3::{Digest, Keccak256};
use std::path::PathBuf;

const TARGET_BITS: u32 = 0x207f_ffff;
const NONCES: [u64; 7] = [0, 1, 7, 42, 255, 1_024, u64::MAX];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendMode {
    Cuda,
    OpenCl,
    Both,
}

impl BackendMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "cuda" => Ok(Self::Cuda),
            "opencl" => Ok(Self::OpenCl),
            "both" => Ok(Self::Both),
            _ => Err(anyhow!(
                "invalid --backend {value:?}; expected cuda, opencl, or both"
            )),
        }
    }

    fn uses_cuda(self) -> bool {
        matches!(self, Self::Cuda | Self::Both)
    }

    fn uses_opencl(self) -> bool {
        matches!(self, Self::OpenCl | Self::Both)
    }
}

#[derive(Debug)]
struct Config {
    backend: BackendMode,
    cuda_module: Option<PathBuf>,
    cuda_device: usize,
    opencl_device: usize,
    require_amd_opencl: bool,
}

fn main() -> Result<()> {
    let config = parse_args()?;

    if config.backend.uses_cuda() {
        reject_runtime_override(CUDA_DRIVER_LIBRARY_ENV)?;
    }
    if config.backend.uses_opencl() {
        reject_runtime_override(OPENCL_LIBRARY_ENV)?;
    }

    let cuda_module = if config.backend.uses_cuda() {
        let path = config
            .cuda_module
            .as_deref()
            .ok_or_else(|| anyhow!("CUDA evidence requires --cuda-module PATH"))?;
        let image = std::fs::read(path)
            .with_context(|| format!("failed to read CUDA module {}", path.display()))?;
        if image.is_empty() {
            return Err(anyhow!("CUDA module {} is empty", path.display()));
        }
        Some(image)
    } else {
        None
    };

    if config.backend.uses_cuda() {
        cuda_runtime::run_selection_self_test()
            .context("CUDA discovery selection self-test failed")?;
        cuda_driver_launch::probe_cuda_device(config.cuda_device)
            .context("physical CUDA device probe failed")?;
        let discovery = cuda_runtime::discover_cuda_devices()?;
        let selected =
            cuda_runtime::select_cuda_device(&discovery.devices, Some(config.cuda_device))?;
        println!(
            "physical_cuda_identity=PASS device_index={} device_name={:?} compute_capability={} driver_version={}",
            selected.index,
            selected.name,
            selected.compute_capability(),
            discovery.driver_version_display()
        );
    }

    if config.backend.uses_opencl() {
        let identity = opencl_driver_launch::describe_opencl_gpu(config.opencl_device)
            .context("physical OpenCL device identity probe failed")?;
        let amd = opencl_identity_is_amd(&identity.vendor, &identity.name);
        if config.require_amd_opencl && !amd {
            return Err(anyhow!(
                "selected OpenCL device is not identified as AMD/ATI: vendor={:?} name={:?}",
                identity.vendor,
                identity.name
            ));
        }
        println!(
            "physical_opencl_identity=PASS device_index={} vendor={:?} device_name={:?} amd_identity={}",
            identity.device_index,
            identity.vendor,
            identity.name,
            if amd { "PASS" } else { "NOT_CLAIMED" }
        );
    }

    let legacy_header = header(BLOCK_HEADER_VERSION_V1);
    let legacy = build_protocol_pow_work(&legacy_header, TARGET_BITS, None)?;
    run_case("legacy_v1", &legacy, &config, cuda_module.as_deref())?;

    let identity = activated_identity();
    let activated_header = header(BLOCK_HEADER_VERSION_V2);
    let activated = build_protocol_pow_work(&activated_header, TARGET_BITS, Some(&identity))?;
    run_case("activated_v2", &activated, &config, cuda_module.as_deref())?;

    println!(
        "physical_gpu_equivalence_probe=PASS canonical_vectors_per_protocol={} protocols=2 cpu_reference=PASS cuda_execution={} opencl_execution={} cross_backend_same_input={} GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED",
        NONCES.len(),
        if config.backend.uses_cuda() { "PASS" } else { "NOT_RUN" },
        if config.backend.uses_opencl() { "PASS" } else { "NOT_RUN" },
        if config.backend == BackendMode::Both { "PASS" } else { "NOT_RUN" },
    );
    Ok(())
}

fn parse_args() -> Result<Config> {
    let mut backend = BackendMode::Both;
    let mut cuda_module = None;
    let mut cuda_device = 0usize;
    let mut opencl_device = 0usize;
    let mut require_amd_opencl = false;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--backend" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --backend"))?;
                backend = BackendMode::parse(&value)?;
            }
            "--cuda-module" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --cuda-module"))?;
                cuda_module = Some(PathBuf::from(value));
            }
            "--cuda-device" => {
                cuda_device = parse_index("--cuda-device", args.next())?;
            }
            "--opencl-device" => {
                opencl_device = parse_index("--opencl-device", args.next())?;
            }
            "--require-amd-opencl" => require_amd_opencl = true,
            "--help" | "-h" => {
                println!(
                    "usage: pulsedag-gpu-equivalence-probe [--backend cuda|opencl|both] [--cuda-module PATH] [--cuda-device INDEX] [--opencl-device INDEX] [--require-amd-opencl]\n\nRuns fixed legacy-v1 and activated-v2 canonical PoW vectors on real system GPU runtimes. CUDA mode requires --cuda-module. The probe refuses the repository-specific PULSEDAG_CUDA_DRIVER_LIBRARY / PULSEDAG_OPENCL_LIBRARY override hooks so CI mock runtimes cannot be mistaken for physical evidence. Physical evidence still requires a controlled host/runtime environment. Passing this probe is candidate hardware evidence only and never emits GPU_MINING_NVIDIA_PASS=true or GPU_MINING_AMD_PASS=true."
                );
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if require_amd_opencl && !backend.uses_opencl() {
        return Err(anyhow!(
            "--require-amd-opencl requires --backend opencl or --backend both"
        ));
    }

    Ok(Config {
        backend,
        cuda_module,
        cuda_device,
        opencl_device,
        require_amd_opencl,
    })
}

fn parse_index(flag: &str, value: Option<String>) -> Result<usize> {
    value
        .ok_or_else(|| anyhow!("missing value for {flag}"))?
        .parse::<usize>()
        .with_context(|| format!("invalid device index for {flag}"))
}

fn reject_runtime_override(name: &str) -> Result<()> {
    if std::env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(anyhow!(
            "{name} is set; physical-equivalence evidence refuses the repository runtime-library override hook so repository mocks/shims cannot be promoted as hardware evidence"
        ));
    }
    Ok(())
}

fn run_case(
    label: &str,
    work: &ProtocolPowWork,
    config: &Config,
    cuda_module: Option<&[u8]>,
) -> Result<()> {
    let expected = NONCES
        .iter()
        .map(|&nonce| work.evaluate_nonce(nonce).final_hash.hash)
        .collect::<Vec<_>>();
    let pre_pow_hash = canonical_pre_pow_hash(work);

    let cuda_hashes = if config.backend.uses_cuda() {
        let image = cuda_module.ok_or_else(|| anyhow!("missing CUDA module bytes"))?;
        let hashes = cuda_driver_launch::launch_kheavyhash_batch(
            image,
            config.cuda_device,
            pre_pow_hash,
            &NONCES,
            64,
        )
        .with_context(|| format!("{label} CUDA launch failed"))?;
        assert_hashes(label, "cuda", work, &hashes, &expected)?;
        Some(hashes)
    } else {
        None
    };

    let opencl_hashes = if config.backend.uses_opencl() {
        let hashes = opencl_driver_launch::launch_kheavyhash_batch(
            config.opencl_device,
            pre_pow_hash,
            &NONCES,
            64,
        )
        .with_context(|| format!("{label} OpenCL launch failed"))?;
        assert_hashes(label, "opencl", work, &hashes, &expected)?;
        Some(hashes)
    } else {
        None
    };

    if let (Some(cuda), Some(opencl)) = (&cuda_hashes, &opencl_hashes) {
        if cuda != opencl {
            return Err(anyhow!(
                "{label} CUDA/OpenCL same-input vectors diverged after each backend passed its CPU comparison"
            ));
        }
    }

    println!(
        "physical_vector_case={} nonces={} cpu_reference=PASS cuda_exact_match={} opencl_exact_match={} cuda_opencl_same_input={}",
        label,
        NONCES.len(),
        if cuda_hashes.is_some() { "PASS" } else { "NOT_RUN" },
        if opencl_hashes.is_some() { "PASS" } else { "NOT_RUN" },
        if cuda_hashes.is_some() && opencl_hashes.is_some() {
            "PASS"
        } else {
            "NOT_RUN"
        }
    );
    Ok(())
}

fn assert_hashes(
    label: &str,
    backend: &str,
    work: &ProtocolPowWork,
    actual: &[[u8; 32]],
    expected: &[[u8; 32]],
) -> Result<()> {
    if actual.len() != expected.len() {
        return Err(anyhow!(
            "{label} {backend} returned {} hashes for {} canonical vectors",
            actual.len(),
            expected.len()
        ));
    }
    for (position, ((&nonce, &actual_hash), &expected_hash)) in NONCES
        .iter()
        .zip(actual.iter())
        .zip(expected.iter())
        .enumerate()
    {
        if actual_hash != expected_hash {
            return Err(anyhow!(
                "{label} {backend} hash mismatch at vector {position} nonce {nonce}: actual={} expected={}",
                hash_hex(actual_hash),
                hash_hex(expected_hash)
            ));
        }
        work.reverify_accelerator_hash(nonce, actual_hash)
            .with_context(|| {
                format!("{label} {backend} canonical re-verification failed for nonce {nonce}")
            })?;
    }
    Ok(())
}

fn canonical_pre_pow_hash(work: &ProtocolPowWork) -> [u8; 32] {
    let digest = Keccak256::digest(&work.material.pre_pow_bytes);
    let mut pre_pow_hash = [0u8; 32];
    pre_pow_hash.copy_from_slice(&digest);
    pre_pow_hash
}

fn hash_hex(hash: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

fn opencl_identity_is_amd(vendor: &str, name: &str) -> bool {
    let vendor = vendor.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    vendor.contains("advanced micro devices")
        || vendor
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token == "amd" || token == "ati")
        || name.contains("radeon")
        || name
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token == "amd" || token == "ati")
}

fn header(version: u32) -> BlockHeader {
    BlockHeader {
        version,
        parents: vec!["11".repeat(32), "22".repeat(32)],
        timestamp: 1_700_000_000,
        difficulty: TARGET_BITS,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amd_identity_classifier_is_conservative() {
        assert!(opencl_identity_is_amd(
            "Advanced Micro Devices, Inc.",
            "gfx1100"
        ));
        assert!(opencl_identity_is_amd("AMD", "Radeon RX 7900 XTX"));
        assert!(opencl_identity_is_amd("Mesa", "AMD Radeon RX 6800"));
        assert!(!opencl_identity_is_amd("NVIDIA Corporation", "NVIDIA RTX"));
        assert!(!opencl_identity_is_amd("Intel(R) Corporation", "Intel Arc"));
    }

    #[test]
    fn runtime_override_guard_allows_absence() {
        const NAME: &str = "PULSEDAG_TEST_GPU_EQUIVALENCE_UNUSED_ENV";
        std::env::remove_var(NAME);
        assert!(reject_runtime_override(NAME).is_ok());
    }

    #[test]
    fn runtime_override_guard_rejects_nonempty_override() {
        const NAME: &str = "PULSEDAG_TEST_GPU_EQUIVALENCE_OVERRIDE_ENV";
        std::env::set_var(NAME, "mock-driver-path");
        let error = reject_runtime_override(NAME).unwrap_err().to_string();
        std::env::remove_var(NAME);
        assert!(error.contains("refuses the repository runtime-library override hook"));
    }
}
