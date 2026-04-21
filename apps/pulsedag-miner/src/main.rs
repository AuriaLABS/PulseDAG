use anyhow::{anyhow, Context, Result};
use pulsedag_core::types::Block;
use pulsedag_rpc::api::ApiResponse;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};

#[derive(Debug, Serialize)]
struct TemplateRequest {
    miner_address: String,
}

#[derive(Debug, Deserialize)]
struct TemplateData {
    template_id: String,
    block: Block,
}

#[derive(Debug, Serialize)]
struct SubmitRequest {
    template_id: String,
    block: Block,
}

#[derive(Debug, Deserialize)]
struct SubmitData {
    accepted: bool,
    block_hash: String,
    height: u64,
    pow_accepted_dev: bool,
    stale_template: bool,
}

struct Config {
    node: String,
    miner_address: String,
    max_tries: u64,
    loop_mode: bool,
    sleep_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = parse_args()?;
    let client = Client::builder().build()?;

    if cfg.loop_mode {
        loop {
            if let Err(e) = mine_once(&client, &cfg).await {
                eprintln!("mine loop error: {e}");
            }
            sleep(Duration::from_millis(cfg.sleep_ms)).await;
        }
    } else {
        mine_once(&client, &cfg).await
    }
}

fn parse_args() -> Result<Config> {
    let mut node = "http://127.0.0.1:8080".to_string();
    let mut miner_address = String::new();
    let mut max_tries = 50_000u64;
    let mut loop_mode = false;
    let mut sleep_ms = 1500u64;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--node" => node = args.next().ok_or_else(|| anyhow!("missing value for --node"))?,
            "--miner-address" => miner_address = args.next().ok_or_else(|| anyhow!("missing value for --miner-address"))?,
            "--max-tries" => max_tries = args.next().ok_or_else(|| anyhow!("missing value for --max-tries"))?.parse().context("invalid --max-tries")?,
            "--loop" => loop_mode = true,
            "--sleep-ms" => sleep_ms = args.next().ok_or_else(|| anyhow!("missing value for --sleep-ms"))?.parse().context("invalid --sleep-ms")?,
            _ => {}
        }
    }

    if miner_address.trim().is_empty() {
        return Err(anyhow!("usage: pulsedag-miner --miner-address <address> [--node http://127.0.0.1:8080] [--max-tries 50000] [--loop] [--sleep-ms 1500]"));
    }

    Ok(Config { node, miner_address, max_tries, loop_mode, sleep_ms })
}

async fn mine_once(client: &Client, cfg: &Config) -> Result<()> {
    let template_url = format!("{}/mining/template", cfg.node.trim_end_matches('/'));
    let submit_url = format!("{}/mining/submit", cfg.node.trim_end_matches('/'));

    let template_resp = client.post(&template_url).json(&TemplateRequest { miner_address: cfg.miner_address.clone() }).send().await?.error_for_status()?;
    let template_api: ApiResponse<TemplateData> = template_resp.json().await?;
    let template = template_api.data.ok_or_else(|| anyhow!("template endpoint returned no data"))?;

    let template_id = template.template_id;
    let mut block = template.block;

    let (header, accepted, tries, final_hash_hex) = pulsedag_core::dev_mine_header(block.header.clone(), cfg.max_tries);
    block.header = header;

    println!("template received: id={} height={} hash={} difficulty={}", template_id, block.header.height, block.hash, block.header.difficulty);
    println!("mined externally: accepted={} tries={} nonce={} hash={}", accepted, tries, block.header.nonce, final_hash_hex);

    let submit_resp = client.post(&submit_url).json(&SubmitRequest { template_id, block }).send().await?.error_for_status()?;
    let submit_api: ApiResponse<SubmitData> = submit_resp.json().await?;

    if let Some(data) = submit_api.data {
        println!("submitted: accepted={} block_hash={} height={} pow_accepted_dev={} stale_template={}", data.accepted, data.block_hash, data.height, data.pow_accepted_dev, data.stale_template);
    } else if let Some(err) = submit_api.error {
        return Err(anyhow!("submit rejected: {} - {}", err.code, err.message));
    }

    Ok(())
}
