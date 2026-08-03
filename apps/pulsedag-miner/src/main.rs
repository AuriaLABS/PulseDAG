use anyhow::{anyhow, Context, Result};
use pulsedag_api::ApiResponse;
use pulsedag_core::types::{compute_block_hash, Block, BlockHeader};
use pulsedag_miner::{verify_backend_result_with_core, CpuMiningBackend, MiningBackend};
#[cfg(feature = "gpu")]
use pulsedag_miner::{GpuBackendConfig, GpuMiningBackend};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

const SUBMIT_FINALITY_UNKNOWN_CODE: &str = "submit_finality_unknown";
const RECONCILIATION_ATTEMPTS: u32 = 20;
const RECONCILIATION_BACKOFF_MS: u64 = 500;
const RECONCILIATION_REQUEST_TIMEOUT_SECS: u64 = 2;

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

#[derive(Debug, Deserialize)]
struct BlockLookupData {
    hash: String,
    height: u64,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendKind {
    Cpu,
    Gpu,
    Auto,
}

impl std::str::FromStr for BackendKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "gpu" => Ok(Self::Gpu),
            "auto" => Ok(Self::Auto),
            _ => Err(anyhow!(
                "invalid --backend: {value}; expected 'cpu', 'gpu', or 'auto'"
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconciliationOutcome {
    Accepted { height: Option<u64> },
    Rejected { reason_code: String, reason: String },
    StillUnknown { detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopRefreshDecision {
    RefreshWork,
}

fn apply_mined_header(block: &mut Block, mined_header: BlockHeader) {
    block.header = mined_header;
    block.hash = compute_block_hash(&block.header);
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = parse_args()?;
    let client = Client::builder().build()?;
    let backend = mining_backend(&cfg)?;
    let mut telemetry = MinerTelemetry::new(backend.name(), cfg.threads);
    telemetry.log("miner_start");

    if cfg.loop_mode {
        loop {
            match mine_once(&client, &cfg, Arc::clone(&backend), &mut telemetry).await {
                Ok(outcome) => {
                    let _decision = loop_refresh_decision_after_outcome(outcome);
                }
                Err(e) => eprintln!("mine loop error: {e}"),
            }
            sleep(Duration::from_millis(cfg.sleep_ms)).await;
        }
    } else {
        mine_once(&client, &cfg, backend, &mut telemetry).await?;
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

    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                println!(
                    "backend_default=cpu backends=cpu,gpu,auto gpu_compiled={}",
                    cfg!(feature = "gpu")
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
    })
}

fn usage() -> &'static str {
    "usage: pulsedag-miner --miner-address <address> [--node http://127.0.0.1:8080] [--backend cpu|gpu|auto] [--gpu-device INDEX] [--max-tries 50000] [--threads N] [--loop] [--sleep-ms 1500] [--refresh-before-expiry-ms 1000] [--worker-id ID] [--no-heartbeat]\n\nMining backend defaults to cpu. The auto backend prefers GPU only when GPU is compiled and initialization succeeds; otherwise it falls back to CPU. The gpu backend is optional and requires building pulsedag-miner with the gpu feature. GPU device selection uses --gpu-device <index>, with conservative OpenCL batch/work defaults overrideable via PULSEDAG_MINER_GPU_BATCH_SIZE and PULSEDAG_MINER_GPU_WORK_SIZE. The canonical kHeavyHash OpenCL kernel is not implemented yet, so the gpu backend refuses to mine rather than using a non-canonical hash path."
}

fn mining_backend(cfg: &Config) -> Result<Arc<dyn MiningBackend>> {
    match cfg.backend {
        BackendKind::Cpu => {
            println!("miner_backend requested=cpu active=cpu cpu_backend_available=true");
            Ok(Arc::new(CpuMiningBackend))
        }
        BackendKind::Gpu => {
            println!("miner_backend requested=gpu active=pending cpu_backend_available=true");
            gpu_mining_backend(cfg.gpu_device)
        }
        BackendKind::Auto => {
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
                    Ok(Arc::new(CpuMiningBackend))
                }
            }
        }
    }
}

#[cfg(not(feature = "gpu"))]
fn gpu_mining_backend(_device_index: Option<usize>) -> Result<Arc<dyn MiningBackend>> {
    Err(anyhow!(
        "GPU backend requested but pulsedag-miner was built without the gpu feature."
    ))
}

#[cfg(feature = "gpu")]
fn gpu_mining_backend(device_index: Option<usize>) -> Result<Arc<dyn MiningBackend>> {
    let config = GpuBackendConfig::default().with_device_index(device_index);
    Ok(Arc::new(GpuMiningBackend::new(config)?))
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
        "invalid_coinbase" => "check miner address/coinbase construction and fetch a fresh template",
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
    LoopRefreshDecision::RefreshWork
}

fn classify_block_lookup(
    expected_hash: &str,
    data: Option<&BlockLookupData>,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> Option<ReconciliationOutcome> {
    if let Some(block) = data {
        if block.hash == expected_hash {
            return Some(ReconciliationOutcome::Accepted {
                height: Some(block.height),
            });
        }
    }

    match error_code.unwrap_or_default().to_ascii_lowercase().as_str() {
        "block_rejected" | "rejected" | "invalid_block" => Some(ReconciliationOutcome::Rejected {
            reason_code: error_code.unwrap_or("block_rejected").to_ascii_lowercase(),
            reason: error_message
                .unwrap_or("node reported a definitive rejected block outcome")
                .to_string(),
        }),
        _ => None,
    }
}

async fn reconcile_submit_finality(
    client: &Client,
    node: &str,
    block_hash: &str,
) -> ReconciliationOutcome {
    let lookup_url = format!("{}/blocks/{}", node.trim_end_matches('/'), block_hash);
    let mut last_detail = "block lookup has not completed".to_string();

    for attempt in 1..=RECONCILIATION_ATTEMPTS {
        match client
            .get(&lookup_url)
            .timeout(Duration::from_secs(RECONCILIATION_REQUEST_TIMEOUT_SECS))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<ApiResponse<BlockLookupData>>().await {
                    Ok(api) => {
                        let error_code = api.error.as_ref().map(|error| error.code.as_str());
                        let error_message = api.error.as_ref().map(|error| error.message.as_str());
                        if let Some(outcome) = classify_block_lookup(
                            block_hash,
                            api.data.as_ref(),
                            error_code,
                            error_message,
                        ) {
                            return outcome;
                        }
                        last_detail = format!(
                            "attempt {attempt}: block not present and node exposed no definitive rejection"
                        );
                    }
                    Err(error) => {
                        last_detail = format!(
                            "attempt {attempt}: block lookup response could not be decoded: {error}"
                        );
                    }
                }
            }
            Ok(response) => {
                last_detail = format!(
                    "attempt {attempt}: block lookup returned HTTP {}",
                    response.status()
                );
            }
            Err(error) => {
                last_detail = format!("attempt {attempt}: block lookup failed: {error}");
            }
        }

        if attempt < RECONCILIATION_ATTEMPTS {
            sleep(Duration::from_millis(RECONCILIATION_BACKOFF_MS)).await;
        }
    }

    ReconciliationOutcome::StillUnknown {
        detail: last_detail,
    }
}

async fn mine_once(
    client: &Client,
    cfg: &Config,
    backend: Arc<dyn MiningBackend>,
    telemetry: &mut MinerTelemetry,
) -> Result<MineOnceOutcome> {
    let template_url = format!("{}/mining/template", cfg.node.trim_end_matches('/'));
    let submit_url = format!("{}/mining/submit", cfg.node.trim_end_matches('/'));

    let template_resp = client
        .post(&template_url)
        .json(&TemplateRequest {
            miner_address: cfg.miner_address.clone(),
        })
        .send()
        .await?
        .error_for_status()?;
    let template_api: ApiResponse<TemplateData> = template_resp.json().await?;
    let template = template_api
        .data
        .ok_or_else(|| anyhow!("template endpoint returned no data"))?;

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
    )
    .await?;
    let mut verified_header = block.header.clone();
    verified_header.nonce = mining.header.nonce;
    apply_mined_header(&mut block, verified_header);
    telemetry.record_mining_result(mining.tries, mining.hashes_per_sec);
    telemetry.log("mining_result");

    let verification = match verify_backend_result_with_core(&block.header, target_bits) {
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
        "template received: protocol_version={} id={} height={} hash={} difficulty={} created_at={} expires_at={} ttl={}s grace={}s target_hex={}",
        template.protocol_version,
        template_id,
        block.header.height,
        block.hash,
        block.header.difficulty,
        template.created_at_unix,
        template.expires_at_unix,
        template.freshness_ttl_secs,
        template.freshness_grace_secs,
        template.target_hex
    );
    println!(
        "mining: algorithm={} pow_engine=canonical_core template_id={} height={} target_hex={} nonce={} pow_hash={} attempts={} hashes_per_sec={:.2} accepted={} elapsed_ms={}",
        template.algorithm,
        template_id,
        block.header.height,
        mining.target_hex,
        block.header.nonce,
        verification.final_hash_hex,
        mining.tries,
        mining.hashes_per_sec,
        verification.accepted,
        mining.elapsed_ms
    );

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

            let outcome = reconcile_submit_finality(client, &cfg.node, &reconciliation_hash).await;
            match outcome {
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
        Ok(resp) if resp.status().is_success() => telemetry.log("heartbeat_sent"),
        Ok(resp) => println!(
            "miner_telemetry event=heartbeat_skipped backend={} workers={} status={} reason=endpoint_unavailable",
            telemetry.backend,
            telemetry.workers,
            resp.status()
        ),
        Err(err) => println!(
            "miner_telemetry event=heartbeat_skipped backend={} workers={} reason=endpoint_unavailable error={}",
            telemetry.backend, telemetry.workers, err
        ),
    }
}

async fn mine_header_with_backend(
    backend: Arc<dyn MiningBackend>,
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
    target_bits: u32,
) -> Result<MiningResult> {
    let max_tries = max_tries.max(1);
    let start = Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        backend.mine_header(header, max_tries, threads, target_bits)
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
    use super::{
        apply_mined_header, classify_block_lookup, default_worker_id, evaluate_template_freshness,
        loop_refresh_decision_after_outcome, parse_args_from, should_skip_stale_submit,
        submit_rejection_action, BackendKind, Block, BlockHeader, BlockLookupData,
        LoopRefreshDecision, MineOnceOutcome, MinerTelemetry, ReconciliationOutcome,
        SUBMIT_FINALITY_UNKNOWN_CODE,
    };

    #[test]
    fn parser_defaults_backend_to_cpu() {
        let cfg = parse_args_from(["--miner-address", "addr"]).expect("valid args");
        assert_eq!(cfg.backend, BackendKind::Cpu);
    }

    #[test]
    fn parser_keeps_threads_validation() {
        let err = parse_args_from(["--miner-address", "addr", "--threads", "0"])
            .expect_err("zero threads must fail");
        assert!(err.to_string().contains("--threads must be >= 1"));
    }

    #[test]
    fn accepted_submit_updates_accepted_counters() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);
        telemetry.record_submit_accepted(Some(12));
        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_accepted, 1);
        assert_eq!(telemetry.submits_rejected, 0);
    }

    #[test]
    fn finality_unknown_is_not_counted_as_rejected() {
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
    fn reconciled_acceptance_does_not_double_count_submit() {
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
    fn unresolved_unknown_remains_outside_rejection_totals() {
        let mut telemetry = MinerTelemetry::new("cpu", 2);
        telemetry.record_submit_finality_unknown();
        telemetry.record_still_unknown();
        assert_eq!(telemetry.submits_total, 1);
        assert_eq!(telemetry.submits_rejected, 0);
        assert_eq!(telemetry.submits_still_unknown, 1);
    }

    #[test]
    fn block_lookup_reconciles_matching_hash_to_accepted() {
        let block = BlockLookupData {
            hash: "abc".to_string(),
            height: 7,
        };
        assert_eq!(
            classify_block_lookup("abc", Some(&block), None, None),
            Some(ReconciliationOutcome::Accepted { height: Some(7) })
        );
    }

    #[test]
    fn not_found_lookup_is_not_a_definitive_rejection() {
        assert_eq!(
            classify_block_lookup("abc", None, Some("NOT_FOUND"), Some("block not found")),
            None
        );
    }

    #[test]
    fn explicit_rejected_lookup_is_supported() {
        assert_eq!(
            classify_block_lookup(
                "abc",
                None,
                Some("BLOCK_REJECTED"),
                Some("definitive rejection")
            ),
            Some(ReconciliationOutcome::Rejected {
                reason_code: "block_rejected".to_string(),
                reason: "definitive rejection".to_string(),
            })
        );
    }

    #[test]
    fn finality_unknown_action_forbids_blind_resubmit() {
        let action = submit_rejection_action(SUBMIT_FINALITY_UNKNOWN_CODE);
        assert!(action.contains("reconcile"));
        assert!(action.contains("do not classify"));
        assert!(action.contains("resubmit"));
    }

    #[test]
    fn stale_freshness_rules_are_preserved() {
        assert!(should_skip_stale_submit(100, 99, 1000).is_some());
        assert!(should_skip_stale_submit(100, 105, 1000).is_none());
        assert_eq!(
            evaluate_template_freshness(100, 101, 1500).remaining_ms,
            1000
        );
    }

    #[test]
    fn loop_always_fetches_fresh_work() {
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::SubmitFinalityStillUnknown),
            LoopRefreshDecision::RefreshWork
        );
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::NodeRejectedStaleTemplate),
            LoopRefreshDecision::RefreshWork
        );
    }

    #[test]
    fn default_worker_id_is_endpoint_safe() {
        assert!(default_worker_id("addr/with spaces").starts_with("miner-addr_with_spaces-"));
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
        apply_mined_header(&mut block, mined_header);
        assert_eq!(
            block.hash,
            pulsedag_core::types::compute_block_hash(&block.header)
        );
        assert_ne!(block.hash, template_hash);
    }
}
