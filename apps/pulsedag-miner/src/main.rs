use anyhow::{anyhow, Context, Result};
use pulsedag_core::pow::{pow_accepts, pow_hash_hex};
use pulsedag_core::types::{Block, BlockHeader};
use pulsedag_rpc::api::ApiResponse;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
    threads: usize,
    loop_mode: bool,
    sleep_ms: u64,
}

struct MiningResult {
    header: BlockHeader,
    accepted: bool,
    tries: u64,
    final_hash_hex: String,
    elapsed_ms: u128,
    hashes_per_sec: f64,
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
    let mut threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut loop_mode = false;
    let mut sleep_ms = 1500u64;

    let mut args = std::env::args().skip(1);
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
            _ => {}
        }
    }

    if miner_address.trim().is_empty() {
        return Err(anyhow!(
            "usage: pulsedag-miner --miner-address <address> [--node http://127.0.0.1:8080] [--max-tries 50000] [--threads N] [--loop] [--sleep-ms 1500]"
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
    })
}

async fn mine_once(client: &Client, cfg: &Config) -> Result<()> {
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

    let mining = mine_header_multithread(block.header.clone(), cfg.max_tries, cfg.threads).await?;
    block.header = mining.header;

    println!(
        "template received: id={} height={} hash={} difficulty={}",
        template_id, block.header.height, block.hash, block.header.difficulty
    );
    println!(
        "mined externally: accepted={} tries={} nonce={} hash={}",
        mining.accepted, mining.tries, block.header.nonce, mining.final_hash_hex
    );
    println!(
        "metrics: elapsed_ms={} hashes_per_sec={:.2} threads={} tries={}",
        mining.elapsed_ms, mining.hashes_per_sec, cfg.threads, mining.tries
    );

    let submit_resp = client
        .post(&submit_url)
        .json(&SubmitRequest { template_id, block })
        .send()
        .await?
        .error_for_status()?;
    let submit_api: ApiResponse<SubmitData> = submit_resp.json().await?;

    if let Some(data) = submit_api.data {
        println!(
            "submitted: accepted={} block_hash={} height={} pow_accepted_dev={} stale_template={}",
            data.accepted, data.block_hash, data.height, data.pow_accepted_dev, data.stale_template
        );
    } else if let Some(err) = submit_api.error {
        return Err(anyhow!("submit rejected: {} - {}", err.code, err.message));
    }

    Ok(())
}

async fn mine_header_multithread(
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
) -> Result<MiningResult> {
    let max_tries = max_tries.max(1);
    let start = Instant::now();

    let (final_header, accepted, tries, final_hash_hex) =
        tokio::task::spawn_blocking(move || mine_header_partitioned(header, max_tries, threads))
            .await
            .context("mining worker task panicked")??;

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
    })
}

fn mine_header_partitioned(
    header: BlockHeader,
    max_tries: u64,
    threads: usize,
) -> Result<(BlockHeader, bool, u64, String)> {
    let max_tries = max_tries.max(1);
    let effective_threads = threads.max(1).min(max_tries as usize);
    let found = Arc::new(AtomicBool::new(false));
    let tries = Arc::new(AtomicU64::new(0));
    let winner: Arc<Mutex<Option<(BlockHeader, String)>>> = Arc::new(Mutex::new(None));
    let mut handles = Vec::with_capacity(effective_threads);

    for tid in 0..effective_threads {
        let found = Arc::clone(&found);
        let tries = Arc::clone(&tries);
        let winner = Arc::clone(&winner);
        let thread_header = header.clone();

        let handle = std::thread::spawn(move || -> Result<()> {
            let mut local_tries = 0u64;
            let mut nonce = tid as u64;

            while nonce < max_tries {
                if found.load(Ordering::Relaxed) {
                    break;
                }

                let mut candidate = thread_header.clone();
                candidate.nonce = nonce;
                local_tries = local_tries.saturating_add(1);

                let hash_hex = pow_hash_hex(&candidate);
                if pow_accepts(&candidate) {
                    let already_found = found.swap(true, Ordering::SeqCst);
                    if !already_found {
                        let mut guard = winner.lock().map_err(|_| {
                            anyhow!("winner mutex poisoned during candidate selection")
                        })?;
                        *guard = Some((candidate, hash_hex));
                    }
                    break;
                }

                nonce = nonce.saturating_add(effective_threads as u64);
            }

            tries.fetch_add(local_tries, Ordering::Relaxed);
            Ok(())
        });
        handles.push(handle);
    }

    for handle in handles {
        let thread_result = handle
            .join()
            .map_err(|_| anyhow!("a mining thread panicked during execution"))?;
        thread_result?;
    }

    let total_tries = tries.load(Ordering::Relaxed).min(max_tries);
    let winner_candidate = winner
        .lock()
        .map_err(|_| anyhow!("winner mutex poisoned when finalizing result"))?
        .clone();
    if let Some((winner_header, winner_hash)) = winner_candidate {
        return Ok((winner_header, true, total_tries, winner_hash));
    }

    let mut fallback_header = header;
    fallback_header.nonce = max_tries.saturating_sub(1);
    let fallback_hash = pow_hash_hex(&fallback_header);
    Ok((fallback_header, false, total_tries.max(1), fallback_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        accept_block,
        genesis::init_chain_state,
        mining::{build_candidate_block, build_coinbase_transaction},
        pow::{pow_hash_hex, pow_preimage_bytes},
        AcceptSource,
    };

    fn fixture_header() -> BlockHeader {
        BlockHeader {
            version: 1,
            parents: vec!["p0".to_string(), "p1".to_string()],
            timestamp: 1_700_000_123,
            difficulty: 7,
            nonce: 42,
            merkle_root: "merkle-fixture".to_string(),
            state_root: "state-fixture".to_string(),
            blue_score: 88,
            height: 99,
        }
    }

    #[test]
    fn miner_and_node_pow_match_for_identical_header() {
        let mut header = fixture_header();
        header.nonce = 17;
        let (_candidate, miner_accepts, _tries, miner_hash) =
            mine_header_partitioned(header.clone(), 1, 1).expect("single-try mining");
        let node_hash = pow_hash_hex(&header);
        assert_eq!(miner_hash, node_hash);
        assert_eq!(miner_accepts, pow_accepts(&header));
    }

    #[test]
    fn miner_found_pow_is_accepted_by_node() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);

        let (mined_header, accepted, _tries, _hash) =
            mine_header_partitioned(block.header.clone(), 50_000, 4).expect("mine result");
        assert!(
            accepted,
            "expected at least one valid nonce with difficulty=1"
        );
        block.header = mined_header;

        assert!(accept_block(block, &mut state, AcceptSource::Rpc).is_ok());
    }

    #[test]
    fn mismatched_header_or_nonce_is_rejected_cleanly() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);

        let (mined_header, accepted, _tries, _hash) =
            mine_header_partitioned(block.header.clone(), 50_000, 2).expect("mine result");
        assert!(accepted, "expected valid mined header");
        block.header = mined_header;
        block.header.nonce = block.header.nonce.saturating_add(1);

        let err = accept_block(block, &mut state, AcceptSource::Rpc).expect_err("must reject");
        let msg = err.to_string();
        assert!(msg.contains("pow rejected"), "unexpected error: {msg}");
    }

    #[test]
    fn fixtures_are_deterministic_across_cross_checks() {
        let header = fixture_header();
        let hash_once = pow_hash_hex(&header);
        let hash_twice = pow_hash_hex(&header);
        let preimage_once = pow_preimage_bytes(&header);
        let preimage_twice = pow_preimage_bytes(&header);

        assert_eq!(hash_once, hash_twice);
        assert_eq!(preimage_once, preimage_twice);
        assert_eq!(
            hash_once,
            "82431f18a31e32d89f72e0c94e9ac0cd39f07d8ce1904d0038f352580f87782f"
        );
    }
}
