#[allow(dead_code)]
#[path = "accelerator_reconnect.rs"]
mod accelerator_reconnect;
#[allow(dead_code)]
#[path = "accelerator_scheduler.rs"]
mod accelerator_scheduler;
#[cfg(feature = "cuda")]
mod cuda_backend;
#[cfg(all(feature = "gpu", feature = "cuda"))]
mod heterogeneous_backend;
mod submit_finality;
mod template_protocol;

use accelerator_reconnect::{AcceleratorControlTransition, AcceleratorReconnectController};
use accelerator_scheduler::AcceleratorDeviceKey;
use anyhow::{anyhow, Context, Result};
#[cfg(feature = "cuda")]
use cuda_backend::{CudaBackendConfig, CudaMiningBackend};
#[cfg(all(feature = "gpu", feature = "cuda"))]
use heterogeneous_backend::HeterogeneousMiningBackend;
use pulsedag_api::ApiResponse;
use pulsedag_core::types::{Block, BlockHeader};
use pulsedag_core::ProtocolActivationIdentity;
#[cfg(feature = "cuda")]
use pulsedag_miner::cuda_driver_launch;
use pulsedag_miner::protocol_backend::{verify_backend_result_for_protocol, ProtocolMiningBackend};
use pulsedag_miner::protocol_pow::compute_mined_block_hash;
use pulsedag_miner::{verify_backend_result_with_core, CpuMiningBackend, MiningBackend};
#[cfg(feature = "gpu")]
use pulsedag_miner::{GpuBackendConfig, GpuMiningBackend};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use submit_finality::{
    reconcile_submit_finality, ReconciliationOutcome, RECONCILIATION_ATTEMPTS,
    RECONCILIATION_BACKOFF_MS, SUBMIT_FINALITY_UNKNOWN_CODE,
};
use template_protocol::validated_template_protocol_identity;
use tokio::time::{sleep, Duration};

trait RuntimeMiningBackend: MiningBackend + ProtocolMiningBackend {}
impl<T> RuntimeMiningBackend for T where T: MiningBackend + ProtocolMiningBackend {}

struct RuntimeBackendSelection {
    backend: Arc<dyn RuntimeMiningBackend>,
    accelerator_devices: Vec<AcceleratorDeviceKey>,
}

impl RuntimeBackendSelection {
    fn cpu() -> Self {
        Self {
            backend: Arc::new(CpuMiningBackend),
            accelerator_devices: Vec::new(),
        }
    }
}

impl std::ops::Deref for RuntimeBackendSelection {
    type Target = dyn RuntimeMiningBackend;

    fn deref(&self) -> &Self::Target {
        self.backend.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AcceleratorRuntimeJob {
    transition: Option<AcceleratorControlTransition>,
    generation: u64,
    reconnect_epoch: u64,
}

#[derive(Debug, Clone)]
struct LoopControlState {
    accelerator: Option<AcceleratorReconnectController>,
    has_successful_work: bool,
    reconnect_pending: bool,
}

impl LoopControlState {
    fn new(devices: &[AcceleratorDeviceKey], max_tries: u64) -> Result<Self> {
        let accelerator = if devices.is_empty() {
            None
        } else {
            Some(AcceleratorReconnectController::build(
                devices,
                max_tries.max(1),
            )?)
        };
        Ok(Self {
            accelerator,
            has_successful_work: false,
            reconnect_pending: false,
        })
    }

    fn note_template_transport_failure(&mut self) {
        if self.accelerator.is_some() && self.has_successful_work {
            self.reconnect_pending = true;
        }
    }

    fn on_work_acquired(&mut self, max_tries: u64) -> Result<Option<AcceleratorRuntimeJob>> {
        let Some(current) = self.accelerator.as_ref() else {
            self.reconnect_pending = false;
            return Ok(None);
        };

        let transition = if !self.has_successful_work {
            None
        } else if self.reconnect_pending {
            Some(AcceleratorControlTransition::Reconnect {
                max_tries: max_tries.max(1),
            })
        } else {
            Some(AcceleratorControlTransition::JobRefresh {
                max_tries: max_tries.max(1),
            })
        };

        if let Some(transition) = transition {
            self.accelerator = Some(current.apply(transition)?);
        }
        self.has_successful_work = true;
        self.reconnect_pending = false;

        let controller = self
            .accelerator
            .as_ref()
            .expect("accelerator runtime job requires controller state");
        Ok(Some(AcceleratorRuntimeJob {
            transition,
            generation: controller.schedule().generation(),
            reconnect_epoch: controller.reconnect_epoch(),
        }))
    }

    fn validate_job_identity(&self, job: Option<AcceleratorRuntimeJob>) -> Result<()> {
        match (self.accelerator.as_ref(), job) {
            (None, None) => Ok(()),
            (Some(controller), Some(job)) => {
                if controller.schedule().generation() != job.generation
                    || controller.reconnect_epoch() != job.reconnect_epoch
                {
                    return Err(anyhow!(
              "stale accelerator runtime job identity: job generation={} reconnect_epoch={} current generation={} reconnect_epoch={}",
              job.generation,
              job.reconnect_epoch,
              controller.schedule().generation(),
              controller.reconnect_epoch()
          ));
                }
                Ok(())
            }
            (None, Some(_)) => Err(anyhow!(
                "accelerator runtime job identity exists without accelerator controller"
            )),
            (Some(_), None) => Err(anyhow!(
                "accelerator controller requires a runtime job identity before submit"
            )),
        }
    }

    #[cfg(test)]
    fn accelerator_mut(&mut self) -> Option<&mut AcceleratorReconnectController> {
        self.accelerator.as_mut()
    }
}

fn is_template_transport_error(error: &reqwest::Error) -> bool {
    !error.is_status()
        && !error.is_decode()
        && !error.is_builder()
        && (error.is_connect() || error.is_timeout() || error.is_request() || error.is_body())
}

#[derive(Debug, Serialize)]
struct TemplateRequest {
    miner_address: String,
}

#[derive(Debug, Deserialize)]
struct TemplateData {
    protocol_version: u32,
    algorithm: String,
    template_id: String,
    created_at_unix: u64,
    expires_at_unix: u64,
    freshness_ttl_secs: u64,
    freshness_grace_secs: u64,
    #[serde(default)]
    protocol_identity: Option<ProtocolActivationIdentity>,
    #[serde(default)]
    protocol_identity_fingerprint: Option<String>,
    block: Block,
    target_hex: String,
    compact_target: u32,
}

#[derive(Debug, Serialize)]
struct SubmitRequest {
    template_id: String,
    block: Block,
}

#[derive(Debug, Deserialize)]
struct SubmitData {
    accepted: bool,
    reason: Option<String>,
    block_hash: Option<String>,
    height: Option<u64>,
    pow_accepted_dev: bool,
    stale_template: bool,
    reason_code: String,
}

#[derive(Debug)]
struct Config {
    node: String,
    miner_address: String,
    backend: BackendKind,
    max_tries: u64,
    threads: usize,
    loop_mode: bool,
    sleep_ms: u64,
    refresh_before_expiry_ms: u64,
    heartbeat: bool,
    worker_id: String,
    gpu_device: Option<usize>,
    cuda_module: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendKind {
    Cpu,
    Gpu,
    Cuda,
    Mixed,
    Auto,
}

impl std::str::FromStr for BackendKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "gpu" => Ok(Self::Gpu),
            "cuda" => Ok(Self::Cuda),
            "mixed" => Ok(Self::Mixed),
            "auto" => Ok(Self::Auto),
            _ => Err(anyhow!(
                "invalid --backend: {value}; expected 'cpu', 'gpu', 'cuda', 'mixed', or 'auto'"
            )),
        }
    }
}

#[derive(Debug, Serialize)]
struct WorkerHeartbeatRequest {
    worker_id: String,
    miner_address: String,
    templates_requested: u64,
    blocks_submitted: u64,
    accepted_blocks: u64,
    stale_rejections: u64,
    invalid_pow_rejections: u64,
    accepted_shares: u64,
}

#[derive(Debug, Clone)]
struct MinerTelemetry {
    backend: &'static str,
    workers: usize,
    attempts: u64,
    hashes_per_sec: f64,
    templates_received: u64,
    templates_skipped_stale: u64,
    submits_total: u64,
    submits_accepted: u64,
    submits_rejected: u64,
    submits_finality_unknown: u64,
    submits_reconciled_accepted: u64,
    submits_reconciled_rejected: u64,
    submits_still_unknown: u64,
    last_reject_code: Option<String>,
    last_template_height: Option<u64>,
    last_accepted_height: Option<u64>,
    node_stale_rejections: u64,
    invalid_pow_rejections: u64,
    backend_verification_failures: u64,
    reject_breakdown: BTreeMap<String, u64>,
}

impl MinerTelemetry {
    fn new(backend: &'static str, workers: usize) -> Self {
        Self {
            backend,
            workers,
            attempts: 0,
            hashes_per_sec: 0.0,
            templates_received: 0,
            templates_skipped_stale: 0,
            submits_total: 0,
            submits_accepted: 0,
            submits_rejected: 0,
            submits_finality_unknown: 0,
            submits_reconciled_accepted: 0,
            submits_reconciled_rejected: 0,
            submits_still_unknown: 0,
            last_reject_code: None,
            last_template_height: None,
            last_accepted_height: None,
            node_stale_rejections: 0,
            invalid_pow_rejections: 0,
            backend_verification_failures: 0,
            reject_breakdown: BTreeMap::new(),
        }
    }

    fn record_template_received(&mut self, height: u64) {
        self.templates_received = self.templates_received.saturating_add(1);
        self.last_template_height = Some(height);
    }

    fn record_mining_result(&mut self, attempts: u64, hashes_per_sec: f64) {
        self.attempts = self.attempts.saturating_add(attempts);
        self.hashes_per_sec = hashes_per_sec;
    }

    fn record_stale_skip(&mut self) {
        self.templates_skipped_stale = self.templates_skipped_stale.saturating_add(1);
    }

    fn record_submit_accepted(&mut self, height: Option<u64>) {
        self.submits_total = self.submits_total.saturating_add(1);
        self.submits_accepted = self.submits_accepted.saturating_add(1);
        self.last_reject_code = None;
        self.last_accepted_height = height;
    }

    fn record_submit_rejected(&mut self, reason_code: impl Into<String>, stale_template: bool) {
        let reason_code = reason_code.into();
        self.submits_total = self.submits_total.saturating_add(1);
        self.submits_rejected = self.submits_rejected.saturating_add(1);
        if reason_code == "invalid_pow" {
            self.invalid_pow_rejections = self.invalid_pow_rejections.saturating_add(1);
        }
        if reason_code == "stale_template" || stale_template {
            self.node_stale_rejections = self.node_stale_rejections.saturating_add(1);
        }
        *self
            .reject_breakdown
            .entry(reason_code.clone())
            .or_insert(0) += 1;
        self.last_reject_code = Some(reason_code);
    }

    fn record_submit_finality_unknown(&mut self) {
        self.submits_total = self.submits_total.saturating_add(1);
        self.submits_finality_unknown = self.submits_finality_unknown.saturating_add(1);
        self.last_reject_code = Some(SUBMIT_FINALITY_UNKNOWN_CODE.to_string());
    }

    fn record_reconciled_accepted(&mut self, height: Option<u64>) {
        self.submits_accepted = self.submits_accepted.saturating_add(1);
        self.submits_reconciled_accepted = self.submits_reconciled_accepted.saturating_add(1);
        self.last_reject_code = None;
        self.last_accepted_height = height;
    }

    fn record_reconciled_rejected(&mut self, reason_code: impl Into<String>) {
        let reason_code = reason_code.into();
        self.submits_rejected = self.submits_rejected.saturating_add(1);
        self.submits_reconciled_rejected = self.submits_reconciled_rejected.saturating_add(1);
        *self
            .reject_breakdown
            .entry(reason_code.clone())
            .or_insert(0) += 1;
        self.last_reject_code = Some(reason_code);
    }

    fn record_still_unknown(&mut self) {
        self.submits_still_unknown = self.submits_still_unknown.saturating_add(1);
        self.last_reject_code = Some("submit_finality_still_unknown".to_string());
    }

    fn record_backend_verification_failed(&mut self) {
        self.backend_verification_failures = self.backend_verification_failures.saturating_add(1);
        self.invalid_pow_rejections = self.invalid_pow_rejections.saturating_add(1);
        *self
            .reject_breakdown
            .entry("backend_verification_failed".to_string())
            .or_insert(0) += 1;
        self.last_reject_code = Some("backend_verification_failed".to_string());
    }

    fn heartbeat_payload(&self, cfg: &Config) -> WorkerHeartbeatRequest {
        WorkerHeartbeatRequest {
            worker_id: cfg.worker_id.clone(),
            miner_address: cfg.miner_address.clone(),
            templates_requested: self.templates_received,
            blocks_submitted: self.submits_total,
            accepted_blocks: self.submits_accepted,
            stale_rejections: self
                .templates_skipped_stale
                .saturating_add(self.node_stale_rejections),
            invalid_pow_rejections: self.invalid_pow_rejections,
            accepted_shares: 0,
        }
    }

    fn log(&self, event: &str) {
        println!(
            "miner_telemetry event={} backend={} workers={} attempts={} hashes_per_sec={:.2} templates_received={} templates_skipped_stale={} submits_total={} submits_accepted={} submits_rejected={} submits_finality_unknown={} submits_reconciled_accepted={} submits_reconciled_rejected={} submits_still_unknown={} backend_verification_failures={} last_reject_code={} reject_breakdown={:?} last_template_height={} last_accepted_height={}",
            event,
            self.backend,
            self.workers,
            self.attempts,
            self.hashes_per_sec,
            self.templates_received,
            self.templates_skipped_stale,
            self.submits_total,
            self.submits_accepted,
            self.submits_rejected,
            self.submits_finality_unknown,
            self.submits_reconciled_accepted,
            self.submits_reconciled_rejected,
            self.submits_still_unknown,
            self.backend_verification_failures,
            self.last_reject_code.as_deref().unwrap_or("-"),
            self.reject_breakdown,
            self.last_template_height
                .map(|height| height.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.last_accepted_height
                .map(|height| height.to_string())
                .unwrap_or_else(|| "-".to_string()),
        );
    }
}

struct MiningResult {
    header: BlockHeader,
    tries: u64,
    elapsed_ms: u128,
    hashes_per_sec: f64,
    target_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MineOnceOutcome {
    Submitted,
    SkippedStaleTemplate,
    NodeRejectedStaleTemplate,
    BackendVerificationRejected,
    SubmitFinalityStillUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateSkipReason {
    Expired,
    NearExpiry,
}

impl TemplateSkipReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::NearExpiry => "near_expiry",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Expired => "template already expired",
            Self::NearExpiry => "template too close to expiry",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplateFreshness {
    now_unix: u64,
    expires_at_unix: u64,
    remaining_ms: u64,
    skip_reason: Option<TemplateSkipReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopRefreshDecision {
    RefreshWork,
}

fn apply_mined_header(
    block: &mut Block,
    mined_header: BlockHeader,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<()> {
    block.header = mined_header;
    block.hash = compute_mined_block_hash(&block.header, identity)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = parse_args()?;
    let client = Client::builder().build()?;
    let selection = mining_backend(&cfg)?;
    let backend = Arc::clone(&selection.backend);
    let mut telemetry = MinerTelemetry::new(backend.name(), cfg.threads);
    telemetry.log("miner_start");

    if cfg.loop_mode {
        let mut loop_control =
            LoopControlState::new(&selection.accelerator_devices, cfg.max_tries)?;
        loop {
            match mine_once(
                &client,
                &cfg,
                Arc::clone(&backend),
                &mut telemetry,
                Some(&mut loop_control),
            )
            .await
            {
                Ok(outcome) => {
                    let _decision = loop_refresh_decision_after_outcome(outcome);
                }
                Err(e) => eprintln!("mine loop error: {e}"),
            }
            sleep(Duration::from_millis(cfg.sleep_ms)).await;
        }
    } else {
        mine_once(&client, &cfg, backend, &mut telemetry, None).await?;
        Ok(())
    }
}

fn parse_args() -> Result<Config> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from<I, S>(args: I) -> Result<Config>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut node = "http://127.0.0.1:8080".to_string();
    let mut miner_address = String::new();
    let mut backend = BackendKind::Cpu;
    let mut max_tries = 50_000u64;
    let mut threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut loop_mode = false;
    let mut sleep_ms = 1500u64;
    let mut refresh_before_expiry_ms = 1000u64;
    let mut heartbeat = true;
    let mut worker_id = String::new();
    let mut gpu_device = None;
    let mut cuda_module = None;

    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                println!(
                    "backend_default=cpu backends=cpu,gpu,cuda,auto gpu_compiled={} cuda_compiled={}",
                    cfg!(feature = "gpu"),
                    cfg!(feature = "cuda")
                );
                std::process::exit(0);
            }
            "--node" => {
                node = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --node"))?
            }
            "--miner-address" => {
                miner_address = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --miner-address"))?
            }
            "--backend" => {
                backend = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --backend"))?
                    .parse()?
            }
            "--cuda-module" => {
                cuda_module = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value for --cuda-module"))?,
                ))
            }
            "--max-tries" => {
                max_tries = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --max-tries"))?
                    .parse()
                    .context("invalid --max-tries")?
            }
            "--threads" => {
                threads = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --threads"))?
                    .parse()
                    .context("invalid --threads")?
            }
            "--loop" => loop_mode = true,
            "--sleep-ms" => {
                sleep_ms = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --sleep-ms"))?
                    .parse()
                    .context("invalid --sleep-ms")?
            }
            "--refresh-before-expiry-ms" => {
                refresh_before_expiry_ms = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --refresh-before-expiry-ms"))?
                    .parse()
                    .context("invalid --refresh-before-expiry-ms")?
            }
            "--heartbeat" => heartbeat = true,
            "--no-heartbeat" => heartbeat = false,
            "--worker-id" => {
                worker_id = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --worker-id"))?
            }
            "--gpu-device" => {
                gpu_device = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value for --gpu-device"))?
                        .parse()
                        .context("invalid --gpu-device")?,
                )
            }
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            _ => {}
        }
    }

    if miner_address.trim().is_empty() {
        return Err(anyhow!(usage()));
    }

    if threads == 0 {
        return Err(anyhow!("--threads must be >= 1"));
    }

    if worker_id.trim().is_empty() {
        worker_id = default_worker_id(&miner_address);
    }

    Ok(Config {
        node,
        miner_address,
        backend,
        max_tries,
        threads,
        loop_mode,
        sleep_ms,
        refresh_before_expiry_ms,
        heartbeat,
        worker_id,
        gpu_device,
        cuda_module,
    })
}

fn usage() -> &'static str {
    "usage: pulsedag-miner --miner-address <address> [--node http://127.0.0.1:8080] [--backend cpu|gpu|cuda|mixed|auto] [--cuda-module PATH] [--gpu-device INDEX] [--max-tries 50000] [--threads N] [--loop] [--sleep-ms 1500] [--refresh-before-expiry-ms 1000] [--worker-id ID] [--no-heartbeat]\n\nMining backend defaults to cpu. The gpu backend is the canonical OpenCL kHeavyHash backend and requires the gpu feature; explicit gpu selection fails closed on OpenCL discovery, runtime, build, launch, or canonical re-verification errors. The explicit cuda backend requires the cuda feature plus --cuda-module PATH and never falls back to CPU on CUDA initialization, device, module, kernel, launch, or canonical re-verification errors. --gpu-device selects one CUDA or OpenCL GPU device. CUDA homogeneous multi-device software scheduling may be requested with PULSEDAG_MINER_CUDA_DEVICES=0,1 (mutually exclusive with --gpu-device); OpenCL multi-device selection uses PULSEDAG_MINER_GPU_DEVICES. The mixed backend requires both gpu+cuda features plus --cuda-module PATH and builds one canonical CUDA+OpenCL schedule; select vendor device lists with PULSEDAG_MINER_CUDA_DEVICES and PULSEDAG_MINER_GPU_DEVICES. Auto tries OpenCL then CPU when no CUDA module is supplied; when --cuda-module is supplied, auto tries CUDA first, then OpenCL, then CPU if accelerator initialization or device selection fails. Physical NVIDIA/AMD validation is not claimed by this software-only wiring."
}

fn mining_backend(cfg: &Config) -> Result<RuntimeBackendSelection> {
    match cfg.backend {
        BackendKind::Cpu => {
            println!("miner_backend requested=cpu active=cpu cpu_backend_available=true");
            Ok(RuntimeBackendSelection::cpu())
        }
        BackendKind::Gpu => {
            println!("miner_backend requested=gpu active=pending cpu_backend_available=true");
            gpu_mining_backend(cfg.gpu_device)
        }
        BackendKind::Cuda => {
            let module_path = cfg
                .cuda_module
                .as_deref()
                .ok_or_else(|| anyhow!("--backend cuda requires --cuda-module PATH"))?;
            println!(
                "miner_backend requested=cuda active=pending device_index={} module={}",
                cfg.gpu_device.unwrap_or(0),
                module_path.display()
            );
            cuda_mining_backend(module_path, cfg.gpu_device)
        }
        BackendKind::Mixed => {
            let module_path = cfg
                .cuda_module
                .as_deref()
                .ok_or_else(|| anyhow!("--backend mixed requires --cuda-module PATH"))?;
            println!(
                "miner_backend requested=mixed active=pending module={} mixed_vendor_software=true",
                module_path.display()
            );
            mixed_mining_backend(module_path, cfg.gpu_device)
        }
        BackendKind::Auto => {
            if let Some(module_path) = cfg.cuda_module.as_deref() {
                println!(
                    "miner_backend requested=auto preference=cuda_if_available device_index={} module={} fallback=opencl_then_cpu",
                    cfg.gpu_device.unwrap_or(0),
                    module_path.display()
                );
                match cuda_mining_backend(module_path, cfg.gpu_device) {
                    Ok(backend) => {
                        println!("miner_backend requested=auto cuda_backend_available=true gpu_backend_available=not_checked cpu_fallback_active=false active=cuda");
                        return Ok(backend);
                    }
                    Err(err) => {
                        println!(
                            "miner_backend requested=auto cuda_backend_available=false fallback=opencl_then_cpu reason={}",
                            err
                        );
                    }
                }
            }

            println!("miner_backend requested=auto preference=gpu_if_available cpu_backend_available=true");
            match gpu_mining_backend(cfg.gpu_device) {
                Ok(backend) => {
                    println!("miner_backend requested=auto gpu_backend_available=true cpu_fallback_active=false active=gpu");
                    Ok(backend)
                }
                Err(err) => {
                    println!(
                        "miner_backend requested=auto gpu_backend_available=false cpu_fallback_active=true active=cpu reason={}",
                        err
                    );
                    Ok(RuntimeBackendSelection::cpu())
                }
            }
        }
    }
}

#[cfg(not(feature = "gpu"))]
fn gpu_mining_backend(_device_index: Option<usize>) -> Result<RuntimeBackendSelection> {
    Err(anyhow!(
        "GPU backend requested but pulsedag-miner was built without the gpu feature."
    ))
}

#[cfg(feature = "gpu")]
fn gpu_mining_backend(device_index: Option<usize>) -> Result<RuntimeBackendSelection> {
    let config = GpuBackendConfig::default().with_device_index(device_index);
    let backend = GpuMiningBackend::new(config)?;
    let accelerator_devices = backend
        .selected_devices()
        .iter()
        .map(|device| AcceleratorDeviceKey::opencl(device.device_index))
        .collect();
    Ok(RuntimeBackendSelection {
        backend: Arc::new(backend),
        accelerator_devices,
    })
}

#[cfg(feature = "cuda")]
const CUDA_DEVICE_LIST_ENV: &str = "PULSEDAG_MINER_CUDA_DEVICES";

#[cfg(feature = "cuda")]
fn parse_cuda_device_indices(value: &str) -> Result<Vec<usize>> {
    let mut indices = Vec::new();
    for raw_index in value.split(',') {
        let token = raw_index.trim();
        if token.is_empty() {
            return Err(anyhow!(
                "{CUDA_DEVICE_LIST_ENV} contains an empty device index"
            ));
        }
        let index = token.parse::<usize>().map_err(|_| {
            anyhow!("invalid CUDA device index '{token}' in {CUDA_DEVICE_LIST_ENV}")
        })?;
        indices.push(index);
    }
    if indices.is_empty() {
        return Err(anyhow!(
            "{CUDA_DEVICE_LIST_ENV} must contain at least one device index"
        ));
    }
    indices.sort_unstable();
    if indices.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(anyhow!(
            "{CUDA_DEVICE_LIST_ENV} contains a duplicate device index"
        ));
    }
    Ok(indices)
}

#[cfg(feature = "cuda")]
fn requested_cuda_device_indices(explicit: Option<usize>) -> Result<Vec<usize>> {
    let env_value = std::env::var(CUDA_DEVICE_LIST_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty());
    match (explicit, env_value) {
        (Some(_), Some(_)) => Err(anyhow!(
            "explicit --gpu-device selection cannot be combined with {CUDA_DEVICE_LIST_ENV}"
        )),
        (Some(index), None) => Ok(vec![index]),
        (None, Some(value)) => parse_cuda_device_indices(&value),
        (None, None) => Ok(vec![0]),
    }
}

#[cfg(not(feature = "cuda"))]
fn cuda_mining_backend(
    _module_path: &Path,
    _device_index: Option<usize>,
) -> Result<RuntimeBackendSelection> {
    Err(anyhow!(
        "CUDA backend requested but pulsedag-miner was built without the cuda feature."
    ))
}

#[cfg(feature = "cuda")]
fn cuda_mining_backend(
    module_path: &Path,
    device_index: Option<usize>,
) -> Result<RuntimeBackendSelection> {
    let module_image = std::fs::read(module_path)
        .with_context(|| format!("failed to read CUDA module from {}", module_path.display()))?;
    if module_image.is_empty() {
        return Err(anyhow!("CUDA module at {} is empty", module_path.display()));
    }

    let device_indices = requested_cuda_device_indices(device_index)?;
    for &selected_index in &device_indices {
        cuda_driver_launch::probe_cuda_device(selected_index).with_context(|| {
            format!("CUDA Driver/device probe failed for device index {selected_index}")
        })?;
    }
    let config = if device_indices.len() == 1 {
        CudaBackendConfig::new(module_image, device_indices[0])?
    } else {
        CudaBackendConfig::for_devices(module_image, device_indices.clone())?
    };
    let selected = device_indices
        .iter()
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "cuda_backend configured devices={} device_indices={} module={} homogeneous_multidevice_software=true hardware_execution=NOT_CLAIMED GPU_MINING_NVIDIA_PASS=NOT_CLAIMED",
        device_indices.len(),
        selected,
        module_path.display()
    );
    Ok(RuntimeBackendSelection {
        backend: Arc::new(CudaMiningBackend::new(config)),
        accelerator_devices: device_indices
            .into_iter()
            .map(AcceleratorDeviceKey::cuda)
            .collect(),
    })
}

#[cfg(not(all(feature = "gpu", feature = "cuda")))]
fn mixed_mining_backend(
    _module_path: &Path,
    _device_index: Option<usize>,
) -> Result<RuntimeBackendSelection> {
    Err(anyhow!(
        "mixed backend requested but pulsedag-miner was not built with both gpu and cuda features."
    ))
}

#[cfg(all(feature = "gpu", feature = "cuda"))]
fn mixed_mining_backend(
    module_path: &Path,
    device_index: Option<usize>,
) -> Result<RuntimeBackendSelection> {
    if device_index.is_some() {
        return Err(anyhow!(
            "--backend mixed does not accept --gpu-device because CUDA and OpenCL indices are separate; use PULSEDAG_MINER_CUDA_DEVICES and PULSEDAG_MINER_GPU_DEVICES"
        ));
    }

    let module_image = std::fs::read(module_path)
        .with_context(|| format!("failed to read CUDA module from {}", module_path.display()))?;
    if module_image.is_empty() {
        return Err(anyhow!("CUDA module at {} is empty", module_path.display()));
    }

    let cuda_device_indices = requested_cuda_device_indices(None)?;
    for &selected_index in &cuda_device_indices {
        cuda_driver_launch::probe_cuda_device(selected_index).with_context(|| {
            format!("CUDA Driver/device probe failed for device index {selected_index}")
        })?;
    }
    let cuda_config = if cuda_device_indices.len() == 1 {
        CudaBackendConfig::new(module_image, cuda_device_indices[0])?
    } else {
        CudaBackendConfig::for_devices(module_image, cuda_device_indices)?
    };
    let cuda_backend = CudaMiningBackend::new(cuda_config);

    let opencl_backend =
        GpuMiningBackend::new(GpuBackendConfig::default().with_device_index(None))?;
    let backend = HeterogeneousMiningBackend::new(cuda_backend, opencl_backend)?;
    let accelerator_devices = backend.devices().to_vec();

    println!(
        "mixed_backend configured devices={} canonical_schedule=true mixed_vendor_software=true hardware_execution=NOT_CLAIMED GPU_MINING_NVIDIA_PASS=NOT_CLAIMED GPU_MINING_AMD_PASS=NOT_CLAIMED",
        accelerator_devices.len()
    );
    Ok(RuntimeBackendSelection {
        backend: Arc::new(backend),
        accelerator_devices,
    })
}

fn default_worker_id(miner_address: &str) -> String {
    let sanitized: String = miner_address
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("miner-{}-{}", sanitized, std::process::id())
}

fn submit_rejection_action(reason_code: &str) -> &'static str {
    match reason_code {
        "accepted" => "no action needed",
        SUBMIT_FINALITY_UNKNOWN_CODE => {
            "reconcile the submitted block hash; do not classify it as rejected or resubmit it"
        }
        "submit_timeout_before_acceptance" => {
            "node did not begin acceptance; fetch fresh work after node lock pressure clears"
        }
        "stale_template" => "refresh template and retry mining on latest work",
        "invalid_pow" => "hard warning: backend/canonical mismatch; discard nonce/header and verify miner target comparison before retry",
        "malformed_serialization" => "rebuild submit payload from a fresh template before retry",
        "missing_parent" => "refresh template; submitted parent is no longer in active DAG",
        "invalid_timestamp" => "refresh template and ensure system clocks are synchronized",
        "duplicate_block" => "stop resubmitting this block hash and fetch fresh work",
        "invalid_coinbase" => {
            "check miner address/coinbase construction and fetch a fresh template"
        }
        "invalid_merkle_or_payload" => {
            "refresh template; included transaction/payload no longer matches node template"
        }
        "unknown_validation_error" => "inspect node validation diagnostics and refresh template",
        "chain_id_mismatch" => "check miner --node target and network/chain configuration",
        "internal_error" => "check node logs and retry after the node recovers",
        "missing_template_id" | "unknown_template" => {
            "refresh template and submit with the returned template_id"
        }
        _ => "inspect node rejection reason and refresh template before retry",
    }
}

fn evaluate_template_freshness(
    now_unix: u64,
    expires_at_unix: u64,
    refresh_before_expiry_ms: u64,
) -> TemplateFreshness {
    let now_ms = now_unix.saturating_mul(1000);
    let expiry_ms = expires_at_unix.saturating_mul(1000);
    let remaining_ms = expiry_ms.saturating_sub(now_ms);

    let skip_reason = if now_ms >= expiry_ms {
        Some(TemplateSkipReason::Expired)
    } else if remaining_ms <= refresh_before_expiry_ms {
        Some(TemplateSkipReason::NearExpiry)
    } else {
        None
    };

    TemplateFreshness {
        now_unix,
        expires_at_unix,
        remaining_ms,
        skip_reason,
    }
}

#[cfg(test)]
fn should_skip_stale_submit(
    now_unix: u64,
    expires_at_unix: u64,
    refresh_before_expiry_ms: u64,
) -> Option<String> {
    let freshness =
        evaluate_template_freshness(now_unix, expires_at_unix, refresh_before_expiry_ms);
    freshness.skip_reason.map(|reason| {
        format!(
            "{} (skip_reason={} remaining_ms={} threshold_ms={} now_unix={} expires_at_unix={})",
            reason.message(),
            reason.as_str(),
            freshness.remaining_ms,
            refresh_before_expiry_ms,
            freshness.now_unix,
            freshness.expires_at_unix
        )
    })
}

fn loop_refresh_decision_after_outcome(_outcome: MineOnceOutcome) -> LoopRefreshDecision {
    // Loop mode deliberately returns to /mining/template after every iteration. This keeps
    // stale-template rejections and unresolved submit finality retryable without resubmitting
    // the same stale or non-final work.
    LoopRefreshDecision::RefreshWork
}

async fn acquire_template_and_runtime_job(
    client: &Client,
    cfg: &Config,
    mut loop_control: Option<&mut LoopControlState>,
) -> Result<(
    TemplateData,
    Option<ProtocolActivationIdentity>,
    Option<AcceleratorRuntimeJob>,
)> {
    let template_url = format!("{}/mining/template", cfg.node.trim_end_matches('/'));
    let template_request = client.post(&template_url).json(&TemplateRequest {
        miner_address: cfg.miner_address.clone(),
    });

    let template_resp = match template_request.send().await {
        Ok(response) => response,
        Err(err) => {
            if is_template_transport_error(&err) {
                if let Some(control) = loop_control.as_deref_mut() {
                    control.note_template_transport_failure();
                }
            }
            return Err(err).context("template request failed before HTTP response");
        }
    }
    .error_for_status()?;

    let template_api: ApiResponse<TemplateData> = match template_resp.json().await {
        Ok(template_api) => template_api,
        Err(err) => {
            if is_template_transport_error(&err) {
                if let Some(control) = loop_control.as_deref_mut() {
                    control.note_template_transport_failure();
                }
            }
            return Err(err).context("template response failed before complete decode");
        }
    };
    let template = template_api
        .data
        .ok_or_else(|| anyhow!("template endpoint returned no data"))?;

    let protocol_identity = validated_template_protocol_identity(
        &template.block.header,
        template.protocol_identity.as_ref(),
        template.protocol_identity_fingerprint.as_deref(),
    )?;

    let runtime_job = if let Some(control) = loop_control {
        let runtime_job = control.on_work_acquired(cfg.max_tries)?;
        if let Some(job) = runtime_job {
            println!(
                "accelerator_loop_control template_id={} transition={:?} generation={} reconnect_epoch={}",
                template.template_id,
                job.transition,
                job.generation,
                job.reconnect_epoch
            );
        }
        runtime_job
    } else {
        None
    };

    Ok((template, protocol_identity, runtime_job))
}

async fn mine_once(
    client: &Client,
    cfg: &Config,
    backend: Arc<dyn RuntimeMiningBackend>,
    telemetry: &mut MinerTelemetry,
    mut loop_control: Option<&mut LoopControlState>,
) -> Result<MineOnceOutcome> {
    let submit_url = format!("{}/mining/submit", cfg.node.trim_end_matches('/'));

    let (template, protocol_identity, runtime_job) =
        acquire_template_and_runtime_job(client, cfg, loop_control.as_deref_mut()).await?;
    let protocol_identity_fingerprint = template.protocol_identity_fingerprint.clone();
    let template_id = template.template_id;
    let mut block = template.block;
    telemetry.record_template_received(block.header.height);
    telemetry.log("template_received");

    let target_bits = if template.compact_target == 0 {
        block.header.difficulty
    } else {
        template.compact_target
    };
    let backend_name = backend.name();
    let mining = mine_header_with_backend(
        backend,
        block.header.clone(),
        cfg.max_tries,
        cfg.threads,
        target_bits,
        protocol_identity.clone(),
    )
    .await?;
    let mut verified_header = block.header.clone();
    verified_header.nonce = mining.header.nonce;
    apply_mined_header(&mut block, verified_header, protocol_identity.as_ref())?;
    telemetry.record_mining_result(mining.tries, mining.hashes_per_sec);
    telemetry.log("mining_result");

    let verification_result = match protocol_identity.as_ref() {
        Some(identity) => verify_backend_result_for_protocol(&block.header, target_bits, identity),
        None => verify_backend_result_with_core(&block.header, target_bits),
    };
    let verification = match verification_result {
        Ok(verification) => verification,
        Err(err) => {
            println!(
                "backend_verification_failed: backend={} nonce={} reason={}",
                backend_name, block.header.nonce, err
            );
            telemetry.record_backend_verification_failed();
            telemetry.log("backend_verification_failed");
            send_worker_heartbeat(client, cfg, telemetry).await;
            return Ok(MineOnceOutcome::BackendVerificationRejected);
        }
    };
    if !verification.accepted {
        println!(
            "backend_verification_failed: backend={} nonce={} pow_hash={} target_hex={} reason=hash_above_target",
            backend_name, block.header.nonce, verification.final_hash_hex, verification.target_hex
        );
        telemetry.record_backend_verification_failed();
        telemetry.log("backend_verification_failed");
        send_worker_heartbeat(client, cfg, telemetry).await;
        return Ok(MineOnceOutcome::BackendVerificationRejected);
    }

    println!(
        "template received: protocol_version={} id={} height={} hash={} difficulty={} created_at={} expires_at={} ttl={}s grace={}s target_hex={} protocol_identity_fingerprint={}",
        template.protocol_version,
        template_id,
        block.header.height,
        block.hash,
        block.header.difficulty,
        template.created_at_unix,
        template.expires_at_unix,
        template.freshness_ttl_secs,
        template.freshness_grace_secs,
        template.target_hex,
        protocol_identity_fingerprint.as_deref().unwrap_or("legacy-v1")
    );
    println!("mining: algorithm={} pow_engine=canonical_core template_id={} height={} target_hex={} nonce={} pow_hash={} attempts={} hashes_per_sec={:.2} accepted={} elapsed_ms={}",
        template.algorithm, template_id, block.header.height, mining.target_hex, block.header.nonce, verification.final_hash_hex, mining.tries, mining.hashes_per_sec, verification.accepted, mining.elapsed_ms);

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs();
    let freshness = evaluate_template_freshness(
        now_unix,
        template.expires_at_unix,
        cfg.refresh_before_expiry_ms,
    );
    if let Some(skip_reason) = freshness.skip_reason {
        println!(
            "stale-template safety: skip submit: template_id={} height={} created_at_unix={} expires_at_unix={} remaining_ms={} skip_reason={} reason={} threshold_ms={}",
            template_id,
            block.header.height,
            template.created_at_unix,
            template.expires_at_unix,
            freshness.remaining_ms,
            skip_reason.as_str(),
            skip_reason.message(),
            cfg.refresh_before_expiry_ms
        );
        println!("action: refresh template and retry mining on latest work");
        telemetry.record_stale_skip();
        telemetry.log("template_skipped_stale");
        send_worker_heartbeat(client, cfg, telemetry).await;
        return Ok(MineOnceOutcome::SkippedStaleTemplate);
    }

    if let Some(control) = loop_control.as_deref() {
        control.validate_job_identity(runtime_job)?;
    }

    let submitted_hash = block.hash.clone();
    let submitted_height = block.header.height;
    let submit_resp = client
        .post(&submit_url)
        .json(&SubmitRequest { template_id, block })
        .send()
        .await?
        .error_for_status()?;
    let submit_api: ApiResponse<SubmitData> = submit_resp.json().await?;

    if let Some(data) = submit_api.data {
        println!(
            "submit_result: accepted={} rejected={} reason_code={} block_hash={} height={} pow_accepted_dev={} stale_template={}",
            data.accepted,
            !data.accepted,
            data.reason_code,
            data.block_hash.as_deref().unwrap_or("-"),
            data.height
                .map(|height| height.to_string())
                .unwrap_or_else(|| "-".to_string()),
            data.pow_accepted_dev,
            data.stale_template
        );
        if data.accepted {
            telemetry.record_submit_accepted(data.height);
            telemetry.log("submit_accepted");
            send_worker_heartbeat(client, cfg, telemetry).await;
            return Ok(MineOnceOutcome::Submitted);
        }

        if data.reason_code == SUBMIT_FINALITY_UNKNOWN_CODE {
            let reconciliation_hash = data
                .block_hash
                .as_deref()
                .unwrap_or(submitted_hash.as_str())
                .to_string();
            telemetry.record_submit_finality_unknown();
            telemetry.log("submit_finality_unknown");
            println!(
                "submit_finality_unknown: block_hash={} height={} action=reconcile_by_hash attempts={} backoff_ms={}",
                reconciliation_hash,
                data.height.unwrap_or(submitted_height),
                RECONCILIATION_ATTEMPTS,
                RECONCILIATION_BACKOFF_MS
            );

            match reconcile_submit_finality(client, &cfg.node, &reconciliation_hash).await {
                ReconciliationOutcome::Accepted { height } => {
                    telemetry.record_reconciled_accepted(height.or(data.height));
                    telemetry.log("submit_reconciled_accepted");
                    println!(
                        "submit_reconciled: outcome=accepted block_hash={} height={}",
                        reconciliation_hash,
                        height
                            .or(data.height)
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
                ReconciliationOutcome::Rejected {
                    reason_code,
                    reason,
                } => {
                    telemetry.record_reconciled_rejected(reason_code.clone());
                    telemetry.log("submit_reconciled_rejected");
                    println!(
                        "submit_reconciled: outcome=rejected block_hash={} reason_code={} reason={}",
                        reconciliation_hash, reason_code, reason
                    );
                }
                ReconciliationOutcome::StillUnknown { detail } => {
                    telemetry.record_still_unknown();
                    telemetry.log("submit_finality_still_unknown");
                    println!(
                        "submit_reconciled: outcome=still_unknown block_hash={} detail={} action=fetch_fresh_work_without_resubmitting_hash",
                        reconciliation_hash, detail
                    );
                    send_worker_heartbeat(client, cfg, telemetry).await;
                    return Ok(MineOnceOutcome::SubmitFinalityStillUnknown);
                }
            }
            send_worker_heartbeat(client, cfg, telemetry).await;
            return Ok(MineOnceOutcome::Submitted);
        }

        telemetry.record_submit_rejected(data.reason_code.clone(), data.stale_template);
        telemetry.log("submit_rejected");
        send_worker_heartbeat(client, cfg, telemetry).await;
        if let Some(reason) = data.reason.as_deref() {
            println!(
                "submit_rejected: reason_code={} reason={}",
                data.reason_code, reason
            );
        }
        println!(
            "action: {}",
            submit_rejection_action(data.reason_code.as_str())
        );
        if data.reason_code == "stale_template" || data.stale_template {
            return Ok(MineOnceOutcome::NodeRejectedStaleTemplate);
        }
    } else if let Some(err) = submit_api.error {
        let reason_code = err.code.to_ascii_lowercase();
        println!(
            "submit_rejected: reason_code={} reason={}",
            reason_code, err.message
        );
        println!("action: {}", submit_rejection_action(reason_code.as_str()));
        telemetry.record_submit_rejected(reason_code.clone(), reason_code == "stale_template");
        telemetry.log("submit_rejected");
        send_worker_heartbeat(client, cfg, telemetry).await;
        if reason_code == "stale_template" {
            return Ok(MineOnceOutcome::NodeRejectedStaleTemplate);
        }
        return Err(anyhow!("submit rejected: {} - {}", err.code, err.message));
    }

    Ok(MineOnceOutcome::Submitted)
}

async fn send_worker_heartbeat(client: &Client, cfg: &Config, telemetry: &MinerTelemetry) {
    if !cfg.heartbeat {
        return;
    }

    let heartbeat_url = format!(
        "{}/mining/workers/heartbeat",
        cfg.node.trim_end_matches('/')
    );
    let payload = telemetry.heartbeat_payload(cfg);
    match client
        .post(&heartbeat_url)
        .timeout(Duration::from_millis(500))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            telemetry.log("heartbeat_sent");
        }
        Ok(resp) => {
            println!(
                "miner_telemetry event=heartbeat_skipped backend={} workers={} status={} reason=endpoint_unavailable",
                telemetry.backend,
                telemetry.workers,
                resp.status()
            );
        }
        Err(err) => {
            println!(
                "miner_telemetry event=heartbeat_skipped backend={} workers={} reason=endpoint_unavailable error={}",
                telemetry.backend,
                telemetry.workers,
                err
            );
        }
    }
}

async fn mine_header_with_backend(
    backend: Arc<dyn RuntimeMiningBackend>,
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
    target_bits: u32,
    identity: Option<ProtocolActivationIdentity>,
) -> Result<MiningResult> {
    let max_tries = max_tries.max(1);
    let start = Instant::now();

    let result = tokio::task::spawn_blocking(move || match identity.as_ref() {
        Some(identity) => {
            backend.mine_header_for_protocol(header, max_tries, threads, target_bits, identity)
        }
        None => backend.mine_header(header, max_tries, threads, target_bits),
    })
    .await
    .context("mining worker task panicked")??;

    let final_header = result.header;
    let tries = result.tries;

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let hashes_per_sec = if elapsed_secs > 0.0 {
        tries as f64 / elapsed_secs
    } else {
        0.0
    };

    Ok(MiningResult {
        header: final_header,
        tries,
        elapsed_ms: elapsed.as_millis(),
        hashes_per_sec,
        target_hex: pulsedag_core::pow::target_hex(&pulsedag_core::pow::target_from_bits(
            target_bits,
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::AcceleratorDeviceKey;
    use super::{
        acquire_template_and_runtime_job, apply_mined_header, default_worker_id,
        evaluate_template_freshness, loop_refresh_decision_after_outcome, mining_backend,
        parse_args_from, should_skip_stale_submit, submit_rejection_action, usage, BackendKind,
        Block, BlockHeader, Config, LoopControlState, LoopRefreshDecision, MineOnceOutcome,
        MinerTelemetry, SubmitRequest, TemplateSkipReason, SUBMIT_FINALITY_UNKNOWN_CODE,
    };
    use pulsedag_core::{ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION};
    use std::path::Path;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn telemetry_test_config() -> Config {
        Config {
            node: "http://127.0.0.1:8080".to_string(),
            miner_address: "addr".to_string(),
            backend: BackendKind::Cpu,
            max_tries: 1,
            threads: 2,
            loop_mode: false,
            sleep_ms: 1,
            refresh_before_expiry_ms: 1000,
            heartbeat: true,
            worker_id: "worker-1".to_string(),
            gpu_device: None,
            cuda_module: None,
        }
    }

    #[test]
    fn parser_defaults_backend_to_cpu() {
        let cfg = parse_args_from(["--miner-address", "addr"]).expect("valid args should parse");

        assert_eq!(cfg.backend, BackendKind::Cpu);
        assert!(cfg.cuda_module.is_none());
    }

    #[test]
    fn parser_accepts_explicit_cpu_backend() {
        let cfg = parse_args_from(["--miner-address", "addr", "--backend", "cpu"])
            .expect("explicit cpu backend should parse");

        assert_eq!(cfg.backend, BackendKind::Cpu);
    }

    #[test]
    fn parser_accepts_explicit_gpu_backend() {
        let cfg = parse_args_from(["--miner-address", "addr", "--backend", "gpu"])
            .expect("explicit gpu backend should parse");

        assert_eq!(cfg.backend, BackendKind::Gpu);
    }

    #[test]
    fn parser_accepts_explicit_cuda_backend_module_and_device() {
        let cfg = parse_args_from([
            "--miner-address",
            "addr",
            "--backend",
            "cuda",
            "--cuda-module",
            "kernel.ptx",
            "--gpu-device",
            "2",
        ])
        .expect("explicit CUDA backend should parse");

        assert_eq!(cfg.backend, BackendKind::Cuda);
        assert_eq!(cfg.cuda_module.as_deref(), Some(Path::new("kernel.ptx")));
        assert_eq!(cfg.gpu_device, Some(2));
    }

    #[cfg(feature = "cuda")]
    #[test]
    fn cuda_device_list_parser_is_canonical_and_fail_closed() {
        assert_eq!(
            super::parse_cuda_device_indices("3, 1,2").unwrap(),
            vec![1, 2, 3]
        );
        assert!(super::parse_cuda_device_indices("1,1").is_err());
        assert!(super::parse_cuda_device_indices("1,,2").is_err());
        assert!(super::parse_cuda_device_indices("cuda0").is_err());
    }

    #[test]
    fn parser_accepts_mixed_backend() {
        let cfg = parse_args_from([
            "--miner-address",
            "addr",
            "--backend",
            "mixed",
            "--cuda-module",
            "kernel.ptx",
        ])
        .unwrap();
        assert_eq!(cfg.backend, BackendKind::Mixed);
        assert_eq!(cfg.cuda_module, Some(PathBuf::from("kernel.ptx")));
    }

    #[test]
    fn parser_accepts_auto_backend() {
        let cfg = parse_args_from(["--miner-address", "addr", "--backend", "auto"])
            .expect("auto backend should parse");

        assert_eq!(cfg.backend, BackendKind::Auto);
    }

    #[test]
    fn parser_accepts_gpu_device_index() {
        let cfg = parse_args_from([
            "--miner-address",
            "addr",
            "--backend",
            "gpu",
            "--gpu-device",
            "2",
        ])
        .expect("explicit gpu device should parse");

        assert_eq!(cfg.backend, BackendKind::Gpu);
        assert_eq!(cfg.gpu_device, Some(2));
    }

    #[test]
    fn usage_describes_canonical_opencl_and_explicit_cuda_backends() {
        let text = usage();

        assert!(text.contains("--backend cpu|gpu|cuda|mixed|auto"));
        assert!(text.contains("--cuda-module PATH"));
        assert!(text.contains("PULSEDAG_MINER_CUDA_DEVICES"));
        assert!(text.contains("canonical OpenCL kHeavyHash backend"));
        assert!(text.contains("explicit gpu selection fails closed"));
        assert!(text.contains("cuda feature"));
        assert!(text.contains("never falls back"));
        assert!(text.contains("Physical NVIDIA/AMD validation is not claimed"));
        assert!(!text.contains("OpenCL scaffold"));
        assert!(!text.contains("not implemented yet"));
    }

    #[test]
    fn explicit_cuda_requires_module_without_fallback() {
        let err = match mining_backend(&Config {
            backend: BackendKind::Cuda,
            ..telemetry_test_config()
        }) {
            Ok(_) => panic!("explicit CUDA without a module must fail"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("--backend cuda requires --cuda-module PATH"));
    }

    #[cfg(not(feature = "cuda"))]
    #[test]
    fn explicit_cuda_without_feature_fails_clearly() {
        let err = match mining_backend(&Config {
            backend: BackendKind::Cuda,
            cuda_module: Some("kernel.ptx".into()),
            ..telemetry_test_config()
        }) {
            Ok(_) => panic!("CUDA without feature must fail"),
            Err(err) => err,
        };
        assert_eq!(
            err.to_string(),
            "CUDA backend requested but pulsedag-miner was built without the cuda feature."
        );
    }

    #[cfg(all(not(feature = "cuda"), not(feature = "gpu")))]
    #[test]
    fn auto_with_explicit_cuda_module_falls_back_to_cpu_without_accelerator_features() {
        let backend = mining_backend(&Config {
            backend: BackendKind::Auto,
            cuda_module: Some("kernel.ptx".into()),
            ..telemetry_test_config()
        })
        .expect("auto must preserve OpenCL-to-CPU fallback when CUDA is unavailable");

        assert_eq!(backend.name(), "cpu");
    }

    #[cfg(all(feature = "cuda", not(feature = "gpu")))]
    #[test]
    fn auto_with_unreadable_cuda_module_falls_back_to_cpu() {
        let backend = mining_backend(&Config {
            backend: BackendKind::Auto,
            cuda_module: Some("/definitely/not-present/pulsedag-kernel.ptx".into()),
            ..telemetry_test_config()
        })
        .expect("auto must fall through after CUDA initialization failure");

        assert_eq!(backend.name(), "cpu");
    }

    #[cfg(feature = "cuda")]
    #[test]
    fn explicit_cuda_unreadable_module_fails_before_runtime_selection() {
        let err = match mining_backend(&Config {
            backend: BackendKind::Cuda,
            cuda_module: Some("/definitely/not-present/pulsedag-kernel.ptx".into()),
            ..telemetry_test_config()
        }) {
            Ok(_) => panic!("unreadable CUDA module must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("failed to read CUDA module"));
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn auto_backend_without_gpu_feature_falls_back_to_cpu() {
        let backend = mining_backend(&Config {
            backend: BackendKind::Auto,
            ..telemetry_test_config()
        })
        .expect("auto backend should always resolve");

        assert_eq!(backend.name(), "cpu");
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn gpu_backend_without_feature_fails_clearly() {
        let err = match mining_backend(&Config {
            backend: BackendKind::Gpu,
            ..telemetry_test_config()
        }) {
            Ok(_) => panic!("gpu without feature must fail"),
            Err(err) => err,
        };

        assert_eq!(
            err.to_string(),
            "GPU backend requested but pulsedag-miner was built without the gpu feature."
        );
    }

    #[cfg(feature = "gpu")]
    #[test]
    #[ignore = "requires an OpenCL runtime and GPU device; the canonical kernel is intentionally not implemented yet"]
    fn gpu_backend_with_feature_is_not_implemented() {
        let backend = mining_backend(&Config {
            backend: BackendKind::Gpu,
            ..telemetry_test_config()
        })
        .expect("gpu backend should be selectable");
        let header = BlockHeader {
            version: 1,
            parents: vec!["p".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: 1,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };

        let err = backend
            .mine_header(header, 1, 1, 1)
            .expect_err("gpu backend scaffold must not mine yet");

        assert!(err
            .to_string()
            .contains("canonical kHeavyHash OpenCL mining is not implemented"));
    }

    #[test]
    fn telemetry_counters_increment_correctly() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_template_received(11);
        telemetry.record_mining_result(42, 2100.0);

        assert_eq!(telemetry.templates_received, 1);
        assert_eq!(telemetry.last_template_height, Some(11));
        assert_eq!(telemetry.attempts, 42);
        assert_eq!(telemetry.hashes_per_sec, 2100.0);
    }

    #[test]
    fn accepted_submit_updates_accepted_counters() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_submit_accepted(Some(12));

        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_accepted, 1);
        assert_eq!(telemetry.submits_rejected, 0);
        assert_eq!(telemetry.last_reject_code, None);
        assert_eq!(telemetry.last_accepted_height, Some(12));
    }

    #[test]
    fn rejected_submit_updates_rejection_counters() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_submit_rejected("invalid_pow", false);

        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_accepted, 0);
        assert_eq!(telemetry.submits_rejected, 1);
        assert_eq!(telemetry.last_reject_code.as_deref(), Some("invalid_pow"));
        assert_eq!(telemetry.invalid_pow_rejections, 1);
    }

    #[test]
    fn task29_finality_unknown_is_not_counted_as_rejected() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_submit_finality_unknown();

        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_finality_unknown, 1);
        assert_eq!(telemetry.submits_rejected, 0);
        assert_eq!(
            telemetry.last_reject_code.as_deref(),
            Some(SUBMIT_FINALITY_UNKNOWN_CODE)
        );
    }

    #[test]
    fn task29_reconciled_acceptance_does_not_double_count_submit() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_submit_finality_unknown();
        telemetry.record_reconciled_accepted(Some(9));

        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_accepted, 1);
        assert_eq!(telemetry.submits_rejected, 0);
        assert_eq!(telemetry.submits_reconciled_accepted, 1);
        assert_eq!(telemetry.last_accepted_height, Some(9));
    }

    #[test]
    fn task29_reconciled_rejection_does_not_double_count_submit() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_submit_finality_unknown();
        telemetry.record_reconciled_rejected("block_rejected");

        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_accepted, 0);
        assert_eq!(telemetry.submits_rejected, 1);
        assert_eq!(telemetry.submits_reconciled_rejected, 1);
    }

    #[test]
    fn task29_unresolved_unknown_remains_outside_rejection_totals() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_submit_finality_unknown();
        telemetry.record_still_unknown();

        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_rejected, 0);
        assert_eq!(telemetry.submits_still_unknown, 1);
    }

    #[test]
    fn backend_verification_failure_increments_local_telemetry_counter() {
        let mut telemetry = MinerTelemetry::new("gpu", 2);

        telemetry.record_backend_verification_failed();

        assert_eq!(telemetry.backend_verification_failures, 1);
        assert_eq!(telemetry.invalid_pow_rejections, 1);
        assert_eq!(telemetry.submits_total, 0);
        assert_eq!(
            telemetry.last_reject_code.as_deref(),
            Some("backend_verification_failed")
        );
    }

    #[test]
    fn backend_verification_failure_does_not_count_as_submit() {
        let mut telemetry = MinerTelemetry::new("gpu", 2);

        telemetry.record_backend_verification_failed();

        let payload = telemetry.heartbeat_payload(&telemetry_test_config());
        assert_eq!(payload.blocks_submitted, 0);
        assert_eq!(payload.accepted_blocks, 0);
        assert_eq!(payload.invalid_pow_rejections, 1);
    }

    #[test]
    fn stale_skip_increments_stale_counter() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);

        telemetry.record_stale_skip();

        assert_eq!(telemetry.templates_skipped_stale, 1);
        let payload = telemetry.heartbeat_payload(&telemetry_test_config());
        assert_eq!(payload.stale_rejections, 1);
    }

    #[test]
    fn cpu_backend_reports_backend_cpu() {
        let telemetry = MinerTelemetry::new("cpu", 4);

        assert_eq!(telemetry.backend, "cpu");
        assert_eq!(telemetry.workers, 4);
    }

    #[test]
    fn heartbeat_payload_keeps_miner_standalone_without_shares() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);
        telemetry.record_template_received(10);
        telemetry.record_submit_accepted(Some(10));

        let payload = telemetry.heartbeat_payload(&telemetry_test_config());

        assert_eq!(payload.worker_id, "worker-1");
        assert_eq!(payload.miner_address, "addr");
        assert_eq!(payload.templates_requested, 1);
        assert_eq!(payload.blocks_submitted, 1);
        assert_eq!(payload.accepted_blocks, 1);
        assert_eq!(payload.accepted_shares, 0);
    }

    #[test]
    fn default_worker_id_is_endpoint_safe() {
        let worker_id = default_worker_id("addr/with spaces");

        assert!(worker_id.starts_with("miner-addr_with_spaces-"));
    }

    #[test]
    fn stale_expired_template_skip_includes_reason_and_timing() {
        let freshness = evaluate_template_freshness(100, 99, 1000);

        assert!(freshness.skip_reason.is_some());
        assert_eq!(freshness.skip_reason, Some(TemplateSkipReason::Expired));
        assert_eq!(freshness.remaining_ms, 0);

        let reason = should_skip_stale_submit(100, 99, 1000).expect("must skip expired template");
        assert!(reason.contains("template already expired"));
        assert!(reason.contains("skip_reason=expired"));
        assert!(reason.contains("remaining_ms=0"));
        assert!(reason.contains("expires_at_unix=99"));
    }

    #[test]
    fn stale_near_expiry_template_skip_includes_reason_and_remaining_ms() {
        let freshness = evaluate_template_freshness(100, 101, 1500);

        assert!(freshness.skip_reason.is_some());
        assert_eq!(freshness.skip_reason, Some(TemplateSkipReason::NearExpiry));
        assert_eq!(freshness.remaining_ms, 1000);

        let reason = should_skip_stale_submit(100, 101, 1500)
            .expect("must skip template too close to expiry");
        assert!(reason.contains("template too close to expiry"));
        assert!(reason.contains("skip_reason=near_expiry"));
        assert!(reason.contains("remaining_ms=1000"));
        assert!(reason.contains("threshold_ms=1500"));
    }

    #[test]
    fn stale_fresh_template_allowed_when_outside_refresh_window() {
        let freshness = evaluate_template_freshness(100, 105, 1000);

        assert!(freshness.skip_reason.is_none());
        assert_eq!(freshness.skip_reason, None);
        assert_eq!(freshness.remaining_ms, 5000);
        assert!(should_skip_stale_submit(100, 105, 1000).is_none());
    }

    #[test]
    fn stale_node_side_rejection_is_retryable() {
        let action = submit_rejection_action("stale_template");

        assert!(action.contains("refresh template"));
        assert!(action.contains("retry mining"));
    }

    #[test]
    fn task29_finality_unknown_action_forbids_blind_resubmit() {
        let action = submit_rejection_action(SUBMIT_FINALITY_UNKNOWN_CODE);

        assert!(action.contains("reconcile"));
        assert!(action.contains("do not classify"));
        assert!(action.contains("resubmit"));
    }

    #[test]
    fn invalid_pow_rejection_is_hard_backend_canonical_warning() {
        let action = submit_rejection_action("invalid_pow");

        assert!(action.contains("hard warning"));
        assert!(action.contains("backend/canonical mismatch"));
        assert!(action.contains("discard nonce/header"));
    }

    #[derive(Debug, Clone)]
    enum TemplateLoopbackStep {
        Healthy(&'static str),
        Disconnect,
        HttpStatus(u16),
    }

    fn loopback_template_body(template_id: &str) -> String {
        serde_json::json!({
            "ok": true,
            "data": {
                "protocol_version": 1,
                "algorithm": "kHeavyHash",
                "template_id": template_id,
                "created_at_unix": 1,
                "expires_at_unix": 4_000_000_000u64,
                "freshness_ttl_secs": 60,
                "freshness_grace_secs": 5,
                "protocol_identity": null,
                "protocol_identity_fingerprint": null,
                "block": {
                    "header": {
                        "version": 1,
                        "parents": ["p"],
                        "timestamp": 1,
                        "nonce": 0,
                        "difficulty": 1,
                        "merkle_root": "m",
                        "state_root": "s",
                        "blue_score": 1,
                        "height": 1
                    },
                    "transactions": [],
                    "hash": "h"
                },
                "target_hex": "01",
                "compact_target": 1
            },
            "error": null,
            "meta": {}
        })
        .to_string()
    }

    async fn spawn_template_loopback(steps: Vec<TemplateLoopbackStep>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener must bind");
        let addr = listener
            .local_addr()
            .expect("loopback address must resolve");

        tokio::spawn(async move {
            for step in steps {
                let (mut socket, _) = listener
                    .accept()
                    .await
                    .expect("scripted loopback connection must arrive");
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0u8; 1024];
                    let read = socket
                        .read(&mut chunk)
                        .await
                        .expect("loopback request must be readable");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                    assert!(
                        request.len() < 16 * 1024,
                        "loopback request headers exceeded test bound"
                    );
                }
                let request = String::from_utf8_lossy(&request);
                assert!(
                    request.starts_with("POST /mining/template "),
                    "unexpected loopback request line: {request}"
                );

                match step {
                    TemplateLoopbackStep::Healthy(template_id) => {
                        let body = loopback_template_body(template_id);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("healthy loopback response must write");
                        socket
                            .shutdown()
                            .await
                            .expect("healthy loopback response must close");
                    }
                    TemplateLoopbackStep::Disconnect => {
                        drop(socket);
                    }
                    TemplateLoopbackStep::HttpStatus(status) => {
                        let body = "{}";
                        let response = format!(
                            "HTTP/1.1 {status} Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("HTTP error loopback response must write");
                        socket
                            .shutdown()
                            .await
                            .expect("HTTP error loopback response must close");
                    }
                }
            }
        });

        format!("http://{addr}")
    }

    #[tokio::test]
    async fn phase17_loopback_template_disconnect_recovery_has_exact_reconnect_semantics() {
        let node = spawn_template_loopback(vec![
            TemplateLoopbackStep::Healthy("tpl-initial"),
            TemplateLoopbackStep::Disconnect,
            TemplateLoopbackStep::Disconnect,
            TemplateLoopbackStep::Healthy("tpl-recovered"),
            TemplateLoopbackStep::Healthy("tpl-refresh"),
            TemplateLoopbackStep::HttpStatus(503),
            TemplateLoopbackStep::Healthy("tpl-after-http-error"),
        ])
        .await;
        let cfg = Config {
            node,
            heartbeat: false,
            max_tries: 8,
            ..telemetry_test_config()
        };
        let client = reqwest::Client::builder()
            .build()
            .expect("loopback client must build");
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::cuda(0)], cfg.max_tries)
            .expect("loop control must build");

        let (initial, initial_identity, initial_job) =
            acquire_template_and_runtime_job(&client, &cfg, Some(&mut state))
                .await
                .expect("initial template acquisition must succeed");
        assert!(initial_identity.is_none());
        let initial_job = initial_job.expect("accelerator runtime job must exist");
        assert_eq!(initial.template_id, "tpl-initial");
        assert_eq!(initial_job.transition, None);
        assert_eq!(initial_job.generation, 0);
        assert_eq!(initial_job.reconnect_epoch, 0);
        assert!(!state.reconnect_pending);

        for _ in 0..2 {
            let err = acquire_template_and_runtime_job(&client, &cfg, Some(&mut state))
                .await
                .expect_err("scripted TCP disconnect must fail acquisition");
            assert!(
                err.to_string()
                    .contains("template request failed before HTTP response"),
                "unexpected disconnect error: {err:#}"
            );
            assert!(state.reconnect_pending);
        }

        let (recovered, recovered_identity, recovered_job) =
            acquire_template_and_runtime_job(&client, &cfg, Some(&mut state))
                .await
                .expect("recovery template acquisition must succeed");
        assert!(recovered_identity.is_none());
        let recovered_job = recovered_job.expect("accelerator runtime job must exist");
        assert_eq!(recovered.template_id, "tpl-recovered");
        assert_eq!(
            recovered_job.transition,
            Some(super::AcceleratorControlTransition::Reconnect { max_tries: 8 })
        );
        assert_eq!(recovered_job.generation, 1);
        assert_eq!(recovered_job.reconnect_epoch, 1);
        assert!(!state.reconnect_pending);

        let (refreshed, refreshed_identity, refreshed_job) =
            acquire_template_and_runtime_job(&client, &cfg, Some(&mut state))
                .await
                .expect("healthy refresh acquisition must succeed");
        assert!(refreshed_identity.is_none());
        let refreshed_job = refreshed_job.expect("accelerator runtime job must exist");
        assert_eq!(refreshed.template_id, "tpl-refresh");
        assert_eq!(
            refreshed_job.transition,
            Some(super::AcceleratorControlTransition::JobRefresh { max_tries: 8 })
        );
        assert_eq!(refreshed_job.generation, 2);
        assert_eq!(refreshed_job.reconnect_epoch, 1);
        assert!(!state.reconnect_pending);

        let http_err = acquire_template_and_runtime_job(&client, &cfg, Some(&mut state))
            .await
            .expect_err("HTTP application failure must fail acquisition");
        assert!(
            http_err.to_string().contains("503"),
            "unexpected HTTP status error: {http_err:#}"
        );
        assert!(
            !state.reconnect_pending,
            "HTTP application status must not synthesize reconnect"
        );

        let (after_http_error, after_http_error_identity, after_http_error_job) =
            acquire_template_and_runtime_job(&client, &cfg, Some(&mut state))
                .await
                .expect("healthy acquisition after HTTP application error must succeed");
        assert!(after_http_error_identity.is_none());
        let after_http_error_job =
            after_http_error_job.expect("accelerator runtime job must exist");
        assert_eq!(after_http_error.template_id, "tpl-after-http-error");
        assert_eq!(
            after_http_error_job.transition,
            Some(super::AcceleratorControlTransition::JobRefresh { max_tries: 8 })
        );
        assert_eq!(after_http_error_job.generation, 3);
        assert_eq!(after_http_error_job.reconnect_epoch, 1);
        assert!(!state.reconnect_pending);
    }

    #[test]
    fn accelerator_loop_initial_work_does_not_fake_reconnect_or_refresh() {
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::opencl(0)], 8).unwrap();
        let job = state.on_work_acquired(8).unwrap().unwrap();
        assert_eq!(job.transition, None);
        assert_eq!(job.generation, 0);
        assert_eq!(job.reconnect_epoch, 0);
        assert!(!state.reconnect_pending);
    }

    #[test]
    fn accelerator_loop_consecutive_work_is_deterministic_job_refresh() {
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::opencl(0)], 8).unwrap();
        let first = state.on_work_acquired(8).unwrap().unwrap();
        let second = state.on_work_acquired(8).unwrap().unwrap();
        let third = state.on_work_acquired(8).unwrap().unwrap();
        assert_eq!(first.transition, None);
        assert_eq!(
            second.transition,
            Some(super::AcceleratorControlTransition::JobRefresh { max_tries: 8 })
        );
        assert_eq!(
            third.transition,
            Some(super::AcceleratorControlTransition::JobRefresh { max_tries: 8 })
        );
        assert_eq!(second.generation, first.generation + 1);
        assert_eq!(third.generation, second.generation + 1);
        assert_eq!(first.reconnect_epoch, 0);
        assert_eq!(second.reconnect_epoch, 0);
        assert_eq!(third.reconnect_epoch, 0);
    }

    #[test]
    fn accelerator_loop_transport_failure_before_first_work_is_not_reconnect() {
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::cuda(0)], 8).unwrap();
        state.note_template_transport_failure();
        let first = state.on_work_acquired(8).unwrap().unwrap();
        assert_eq!(first.transition, None);
        assert_eq!(first.generation, 0);
        assert_eq!(first.reconnect_epoch, 0);
    }

    #[test]
    fn accelerator_loop_repeated_template_transport_failures_reconnect_once() {
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::cuda(0)], 8).unwrap();
        state.on_work_acquired(8).unwrap();
        state.note_template_transport_failure();
        state.note_template_transport_failure();
        state.note_template_transport_failure();
        let reconnected = state.on_work_acquired(8).unwrap().unwrap();
        assert_eq!(
            reconnected.transition,
            Some(super::AcceleratorControlTransition::Reconnect { max_tries: 8 })
        );
        assert_eq!(reconnected.generation, 1);
        assert_eq!(reconnected.reconnect_epoch, 1);

        let refreshed = state.on_work_acquired(8).unwrap().unwrap();
        assert_eq!(
            refreshed.transition,
            Some(super::AcceleratorControlTransition::JobRefresh { max_tries: 8 })
        );
        assert_eq!(refreshed.generation, 2);
        assert_eq!(refreshed.reconnect_epoch, 1);
    }

    #[test]
    fn accelerator_loop_reconnect_rejects_pre_reconnect_ticket_and_job_identity() {
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::opencl(0)], 8).unwrap();
        let old_job = state.on_work_acquired(8).unwrap().unwrap();
        let stale = state
            .accelerator_mut()
            .unwrap()
            .dispatch_lane(0)
            .unwrap()
            .unwrap();
        state.note_template_transport_failure();
        let current_job = state.on_work_acquired(8).unwrap().unwrap();
        assert!(state
            .accelerator_mut()
            .unwrap()
            .complete_lane(stale)
            .is_err());
        assert!(state.validate_job_identity(Some(old_job)).is_err());
        state.validate_job_identity(Some(current_job)).unwrap();
    }

    #[test]
    fn accelerator_loop_refresh_rejects_prior_generation_ticket_and_job_identity() {
        let mut state = LoopControlState::new(&[AcceleratorDeviceKey::cuda(0)], 8).unwrap();
        let old_job = state.on_work_acquired(8).unwrap().unwrap();
        let stale = state
            .accelerator_mut()
            .unwrap()
            .dispatch_lane(0)
            .unwrap()
            .unwrap();
        let current_job = state.on_work_acquired(8).unwrap().unwrap();
        assert_eq!(current_job.reconnect_epoch, old_job.reconnect_epoch);
        assert!(state
            .accelerator_mut()
            .unwrap()
            .complete_lane(stale)
            .is_err());
        assert!(state.validate_job_identity(Some(old_job)).is_err());
        state.validate_job_identity(Some(current_job)).unwrap();
    }

    #[test]
    fn accelerator_loop_stale_and_finality_outcomes_do_not_fake_reconnect() {
        for outcome in [
            MineOnceOutcome::NodeRejectedStaleTemplate,
            MineOnceOutcome::SkippedStaleTemplate,
            MineOnceOutcome::SubmitFinalityStillUnknown,
        ] {
            assert_eq!(
                loop_refresh_decision_after_outcome(outcome),
                LoopRefreshDecision::RefreshWork
            );
        }
    }

    #[test]
    fn stale_loop_mode_refreshes_work_after_stale() {
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::NodeRejectedStaleTemplate),
            LoopRefreshDecision::RefreshWork
        );
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::SkippedStaleTemplate),
            LoopRefreshDecision::RefreshWork
        );
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::BackendVerificationRejected),
            LoopRefreshDecision::RefreshWork
        );
    }

    #[test]
    fn task29_unresolved_finality_refreshes_work_without_resubmit() {
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::SubmitFinalityStillUnknown),
            LoopRefreshDecision::RefreshWork
        );
    }

    #[test]
    fn parser_keeps_threads_validation() {
        let err = parse_args_from(["--miner-address", "addr", "--threads", "0"])
            .expect_err("zero threads must be rejected");

        assert!(err.to_string().contains("--threads must be >= 1"));
    }

    #[test]
    fn parser_keeps_loop_and_max_tries_options() {
        let cfg = parse_args_from([
            "--miner-address",
            "addr",
            "--max-tries",
            "7",
            "--threads",
            "2",
            "--loop",
        ])
        .expect("valid manual args should parse");

        assert_eq!(cfg.max_tries, 7);
        assert_eq!(cfg.threads, 2);
        assert!(cfg.loop_mode);
    }

    #[test]
    fn known_submit_rejection_classes_have_actionable_text() {
        for code in [
            "stale_template",
            "invalid_pow",
            "malformed_serialization",
            "missing_parent",
            "invalid_timestamp",
            "duplicate_block",
            "invalid_coinbase",
            "invalid_merkle_or_payload",
            "unknown_validation_error",
            "chain_id_mismatch",
            "internal_error",
            SUBMIT_FINALITY_UNKNOWN_CODE,
            "submit_timeout_before_acceptance",
        ] {
            let action = submit_rejection_action(code);
            assert!(!action.is_empty());
            assert_ne!(action, "no action needed");
        }
    }

    #[test]
    fn submit_payload_serializes_with_template_id_and_block() {
        let block = Block {
            header: BlockHeader {
                version: 1,
                parents: vec!["p".into()],
                timestamp: 1,
                nonce: 1,
                difficulty: 1,
                merkle_root: "m".into(),
                state_root: "s".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
            hash: "h".into(),
        };
        let req = SubmitRequest {
            template_id: "tpl-1".into(),
            block,
        };
        let v = serde_json::to_value(&req).expect("serialize");
        assert_eq!(v["template_id"], "tpl-1");
        assert!(v["block"].is_object());
    }

    #[test]
    fn nonzero_mined_nonce_recomputes_canonical_block_hash() {
        let header = BlockHeader {
            version: 1,
            parents: vec!["p".into()],
            timestamp: 1,
            nonce: 0,
            difficulty: 1,
            merkle_root: "m".into(),
            state_root: "s".into(),
            blue_score: 1,
            height: 1,
        };
        let template_hash = pulsedag_core::types::compute_block_hash(&header);
        let mut block = Block {
            hash: template_hash.clone(),
            header: header.clone(),
            transactions: vec![],
        };
        let mut mined_header = header;
        mined_header.nonce = 1;
        apply_mined_header(&mut block, mined_header, None).unwrap();
        assert_eq!(
            block.hash,
            pulsedag_core::types::compute_block_hash(&block.header)
        );
        assert_ne!(block.hash, template_hash);
    }

    #[test]
    fn activated_v2_mined_nonce_recomputes_chain_bound_block_hash() {
        let identity = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "44".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let header = BlockHeader {
            version: 2,
            parents: vec!["11".repeat(32)],
            timestamp: 1_700_000_000,
            nonce: 0,
            difficulty: 0x207f_ffff,
            merkle_root: "22".repeat(32),
            state_root: "33".repeat(32),
            blue_score: 1,
            height: 2,
        };
        let template_hash =
            pulsedag_core::compute_block_hash_v2(&header, &identity.chain_id).unwrap();
        let mut block = Block {
            hash: template_hash.clone(),
            header: header.clone(),
            transactions: vec![],
        };
        let mut mined_header = header;
        mined_header.nonce = 1;
        apply_mined_header(&mut block, mined_header, Some(&identity)).unwrap();

        assert_eq!(
            block.hash,
            pulsedag_core::compute_block_hash_v2(&block.header, &identity.chain_id).unwrap()
        );
        assert_ne!(block.hash, template_hash);
    }
}
