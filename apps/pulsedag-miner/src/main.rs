use anyhow::{anyhow, Context, Result};
use pulsedag_api::ApiResponse;
use pulsedag_core::types::{Block, BlockHeader};
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
    max_tries: u64,
    threads: usize,
    loop_mode: bool,
    sleep_ms: u64,
    refresh_before_expiry_ms: u64,
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
    let backend: Arc<dyn MiningBackend> = Arc::new(CpuMiningBackend);

    if cfg.loop_mode {
        loop {
            match mine_once(&client, &cfg, Arc::clone(&backend)).await {
                Ok(outcome) => {
                    let _decision = loop_refresh_decision_after_outcome(outcome);
                }
                Err(e) => eprintln!("mine loop error: {e}"),
            }
            sleep(Duration::from_millis(cfg.sleep_ms)).await;
        }
    } else {
        mine_once(&client, &cfg, backend).await?;
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
    let mut max_tries = 50_000u64;
    let mut threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut loop_mode = false;
    let mut sleep_ms = 1500u64;
    let mut refresh_before_expiry_ms = 1000u64;

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
            _ => {}
        }
    }

    if miner_address.trim().is_empty() {
        return Err(anyhow!(
            "usage: pulsedag-miner --miner-address <address> [--node http://127.0.0.1:8080] [--max-tries 50000] [--threads N] [--loop] [--sleep-ms 1500] [--refresh-before-expiry-ms 1000]"
        ));
    }

    if threads == 0 {
        return Err(anyhow!("--threads must be >= 1"));
    }

    Ok(Config {
        node,
        miner_address,
        max_tries,
        threads,
        loop_mode,
        sleep_ms,
        refresh_before_expiry_ms,
    })
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
        if reason_code == "stale_template" {
            return Ok(MineOnceOutcome::NodeRejectedStaleTemplate);
        }
        return Err(anyhow!("submit rejected: {} - {}", err.code, err.message));
    }

    Ok(MineOnceOutcome::Submitted)
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
        evaluate_template_freshness, loop_refresh_decision_after_outcome, parse_args_from,
        should_skip_stale_submit, submit_rejection_action, Block, BlockHeader, LoopRefreshDecision,
        MineOnceOutcome, SubmitRequest, TemplateSkipReason,
    };

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
