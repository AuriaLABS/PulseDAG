use anyhow::{anyhow, Context, Result};
use pulsedag_api::ApiResponse;
use pulsedag_core::types::{Block, BlockHeader};
#[cfg(feature = "gpu")]
use pulsedag_miner::GpuMiningBackend;
use pulsedag_miner::{CpuMiningBackend, MiningBackend};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendKind {
    Cpu,
    Gpu,
}

impl std::str::FromStr for BackendKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "gpu" => Ok(Self::Gpu),
            _ => Err(anyhow!(
                "invalid --backend: {value}; expected 'cpu' or 'gpu'"
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
    last_reject_code: Option<String>,
    last_template_height: Option<u64>,
    last_accepted_height: Option<u64>,
    node_stale_rejections: u64,
    invalid_pow_rejections: u64,
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
            last_reject_code: None,
            last_template_height: None,
            last_accepted_height: None,
            node_stale_rejections: 0,
            invalid_pow_rejections: 0,
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
        self.last_reject_code = Some(reason_code);
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
            "miner_telemetry event={} backend={} workers={} attempts={} hashes_per_sec={:.2} templates_received={} templates_skipped_stale={} submits_total={} submits_accepted={} submits_rejected={} last_reject_code={} last_template_height={} last_accepted_height={}",
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
            self.last_reject_code.as_deref().unwrap_or("-"),
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
    accepted: bool,
    tries: u64,
    final_hash_hex: String,
    elapsed_ms: u128,
    hashes_per_sec: f64,
    target_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MineOnceOutcome {
    Submitted,
    SkippedStaleTemplate,
    NodeRejectedStaleTemplate,
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

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = parse_args()?;
    let client = Client::builder().build()?;
    let backend = mining_backend(cfg.backend)?;
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

    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
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
            "--help" | "-h" => return Err(anyhow!(usage())),
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
    })
}

fn usage() -> &'static str {
    "usage: pulsedag-miner --miner-address <address> [--node http://127.0.0.1:8080] [--backend cpu|gpu] [--max-tries 50000] [--threads N] [--loop] [--sleep-ms 1500] [--refresh-before-expiry-ms 1000] [--worker-id ID] [--no-heartbeat]

Mining backend defaults to cpu. The gpu backend is optional and requires building pulsedag-miner with the gpu feature; the GPU kernel/backend is not implemented yet."
}

fn mining_backend(kind: BackendKind) -> Result<Arc<dyn MiningBackend>> {
    match kind {
        BackendKind::Cpu => Ok(Arc::new(CpuMiningBackend)),
        BackendKind::Gpu => gpu_mining_backend(),
    }
}

#[cfg(not(feature = "gpu"))]
fn gpu_mining_backend() -> Result<Arc<dyn MiningBackend>> {
    Err(anyhow!(
        "GPU backend requested but pulsedag-miner was built without the gpu feature."
    ))
}

#[cfg(feature = "gpu")]
fn gpu_mining_backend() -> Result<Arc<dyn MiningBackend>> {
    Ok(Arc::new(GpuMiningBackend))
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
        "stale_template" => "refresh template and retry mining on latest work",
        "invalid_pow" => "discard nonce/header and verify miner target comparison before retry",
        "malformed_block" => "rebuild the block from a fresh template before retry",
        "invalid_height" => "refresh template; submitted height does not match node state",
        "invalid_parent" => "refresh template; submitted parent set is no longer valid",
        "duplicate_block" => "stop resubmitting this block hash and fetch fresh work",
        "invalid_coinbase" => {
            "check miner address/coinbase construction and fetch a fresh template"
        }
        "invalid_transaction" => "refresh template; included transaction set is no longer valid",
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
    // stale-template rejections retryable without resubmitting the same stale work.
    LoopRefreshDecision::RefreshWork
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
    let mining = mine_header_with_backend(
        backend,
        block.header.clone(),
        cfg.max_tries,
        cfg.threads,
        target_bits,
    )
    .await?;
    block.header = mining.header;
    telemetry.record_mining_result(mining.tries, mining.hashes_per_sec);
    telemetry.log("mining_result");

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
    println!("mining: algorithm={} pow_engine=canonical_core template_id={} height={} target_hex={} nonce={} pow_hash={} attempts={} hashes_per_sec={:.2} accepted={} elapsed_ms={}",
        template.algorithm, template_id, block.header.height, mining.target_hex, block.header.nonce, mining.final_hash_hex, mining.tries, mining.hashes_per_sec, mining.accepted, mining.elapsed_ms);

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
        } else {
            telemetry.record_submit_rejected(data.reason_code.clone(), data.stale_template);
            telemetry.log("submit_rejected");
        }
        send_worker_heartbeat(client, cfg, telemetry).await;
        if !data.accepted {
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
    let accepted = result.accepted;
    let tries = result.tries;
    let final_hash_hex = result.final_hash_hex;

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let hashes_per_sec = if elapsed_secs > 0.0 {
        tries as f64 / elapsed_secs
    } else {
        0.0
    };

    Ok(MiningResult {
        header: final_header,
        accepted,
        tries,
        final_hash_hex,
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
        default_worker_id, evaluate_template_freshness, loop_refresh_decision_after_outcome,
        mining_backend, parse_args_from, should_skip_stale_submit, submit_rejection_action, usage,
        BackendKind, Block, BlockHeader, Config, LoopRefreshDecision, MineOnceOutcome,
        MinerTelemetry, SubmitRequest, TemplateSkipReason,
    };

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
        }
    }

    #[test]
    fn parser_defaults_backend_to_cpu() {
        let cfg = parse_args_from(["--miner-address", "addr"]).expect("valid args should parse");

        assert_eq!(cfg.backend, BackendKind::Cpu);
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
    fn usage_mentions_optional_gpu_backend() {
        let text = usage();

        assert!(text.contains("--backend cpu|gpu"));
        assert!(text.contains("gpu backend is optional"));
        assert!(text.contains("gpu feature"));
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn gpu_backend_without_feature_fails_clearly() {
        let err = match mining_backend(BackendKind::Gpu) {
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
    fn gpu_backend_with_feature_is_not_implemented() {
        let backend = mining_backend(BackendKind::Gpu).expect("gpu backend should be selectable");
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

        assert_eq!(err.to_string(), "GPU backend is not implemented yet.");
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
    fn stale_loop_mode_refreshes_work_after_stale() {
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::NodeRejectedStaleTemplate),
            LoopRefreshDecision::RefreshWork
        );
        assert_eq!(
            loop_refresh_decision_after_outcome(MineOnceOutcome::SkippedStaleTemplate),
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
            "malformed_block",
            "invalid_height",
            "invalid_parent",
            "duplicate_block",
            "invalid_coinbase",
            "invalid_transaction",
            "chain_id_mismatch",
            "internal_error",
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
}
