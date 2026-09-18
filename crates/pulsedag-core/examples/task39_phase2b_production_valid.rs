use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_block_to_dag_metadata, address_from_public_key, apply_transaction, block_subsidy,
    build_candidate_block, build_coinbase_transaction, compute_txid, consensus_difficulty_snapshot,
    dev_mine_header, refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
    signing_message, validate_block, ChainState, OutPoint, Transaction, TxInput, TxOutput,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

const SCHEMA: &str = "pulsedag.task39.phase2b-production-valid.v1";
const CHAIN_ID: &str = "pulsedag-task39-phase2b-production-valid";
const DEFAULT_BLOCKS: usize = 4_096;
const MILLION: usize = 1_000_000;
const RETAINED_BLOCKS: usize = 64;
const MAX_POW_TRIES: u64 = 128;

#[derive(Debug)]
struct Args {
    blocks: usize,
    out: PathBuf,
    candidate_sha: String,
    candidate_tree: String,
    self_test: bool,
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema: &'static str,
    candidate_sha: String,
    candidate_tree_sha: String,
    requested_blocks: usize,
    production_validated_blocks: usize,
    corpus_digest: String,
    elapsed_ms: u128,
    final_height: u64,
    final_tip: String,
    final_state_root: String,
    final_difficulty_bits: u32,
    pow_attempts_total: u64,
    pow_attempts_max: u64,
    pow_failures: u64,
    max_context_blocks: usize,
    max_utxo_entries: usize,
    retained_context_limit: usize,
    retarget_window_size: usize,
    production_valid_pow_and_state_root_per_block: bool,
    bounded_validation_context: bool,
    million_block_count_observed: bool,
    coverage: Coverage,
    runtime_gate_result: String,
    fail_reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Coverage {
    completion_eligible: bool,
    missing_required_measurements: Vec<&'static str>,
}

fn parse_args() -> Result<Args, String> {
    let mut blocks = DEFAULT_BLOCKS;
    let mut out = PathBuf::from("task39-phase2b-production-valid.json");
    let mut candidate_sha = env::var("CANDIDATE_SHA").unwrap_or_else(|_| "unknown".into());
    let mut candidate_tree = env::var("CANDIDATE_TREE").unwrap_or_else(|_| "unknown".into());
    let mut self_test = false;
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--blocks" => {
                blocks = it
                    .next()
                    .ok_or("missing --blocks value")?
                    .parse()
                    .map_err(|_| "invalid --blocks")?
            }
            "--out" => out = PathBuf::from(it.next().ok_or("missing --out value")?),
            "--candidate-sha" => {
                candidate_sha = it.next().ok_or("missing --candidate-sha value")?
            }
            "--candidate-tree" => {
                candidate_tree = it.next().ok_or("missing --candidate-tree value")?
            }
            "--self-test" => self_test = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if blocks == 0 {
        return Err("--blocks must be >= 1".into());
    }
    Ok(Args {
        blocks,
        out,
        candidate_sha,
        candidate_tree,
        self_test,
    })
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[39_u8; 32])
}

fn public_key_hex(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}

fn signed_carry_transaction(
    key: &SigningKey,
    previous_carry: OutPoint,
    coinbase_outpoint: OutPoint,
    address: &str,
    amount: u64,
    nonce: u64,
) -> Transaction {
    let public_key = public_key_hex(key);
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs: vec![
            TxInput {
                previous_output: previous_carry,
                public_key: public_key.clone(),
                signature: String::new(),
            },
            TxInput {
                previous_output: coinbase_outpoint,
                public_key: public_key.clone(),
                signature: String::new(),
            },
        ],
        outputs: vec![TxOutput {
            address: address.to_string(),
            amount,
        }],
        fee: 0,
        nonce,
    };
    let signature = hex::encode(key.sign(&signing_message(&tx)).to_bytes());
    for input in &mut tx.inputs {
        input.signature = signature.clone();
    }
    tx.txid = compute_txid(&tx);
    tx
}

fn prune_validation_context(state: &mut ChainState) {
    while state.dag.selected_chain.len() > RETAINED_BLOCKS {
        let stale = state.dag.selected_chain.remove(0);
        state.dag.ordered_dag.retain(|hash| hash != &stale);
        state.dag.blocks.remove(&stale);
        state.dag.children.remove(&stale);
        state.dag.selected_parents.remove(&stale);
        state.dag.merge_set_blues.remove(&stale);
        state.dag.merge_set_reds.remove(&stale);
        state.dag.blue_work.remove(&stale);
        state.dag.merge_set_diagnostics.remove(&stale);
    }
}

fn advance_validated_context(
    state: &mut ChainState,
    block: &pulsedag_core::Block,
) -> Result<String, String> {
    for tx in &block.transactions {
        apply_transaction(tx, state, block.header.height).map_err(|e| e.to_string())?;
    }
    let observed_root = state.utxo.compute_state_root().map_err(|e| e.to_string())?;
    if observed_root != block.header.state_root {
        return Err(format!(
            "live state root mismatch after validated block {}: {} != {}",
            block.hash, observed_root, block.header.state_root
        ));
    }

    accept_block_to_dag_metadata(block, state).map_err(|e| e.to_string())?;
    state.dag.selected_chain.push(block.hash.clone());
    state.dag.ordered_dag.push(block.hash.clone());
    state.dag.ordered_dag_tip = Some(block.hash.clone());
    state.dag.ordered_dag_state_root = Some(observed_root.clone());
    prune_validation_context(state);
    Ok(observed_root)
}

fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|e| e.to_string())
}

fn run(args: Args) -> Result<Manifest, String> {
    let blocks = if args.self_test { 128 } else { args.blocks };
    let started = Instant::now();
    let key = signing_key();
    let address = address_from_public_key(&public_key_hex(&key));
    let mut state = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    let retarget_window = consensus_difficulty_snapshot(&state).policy.window_size;
    if RETAINED_BLOCKS <= retarget_window {
        return Err(format!(
            "retained validation context {} must exceed retarget window {}",
            RETAINED_BLOCKS, retarget_window
        ));
    }

    let mut carry_outpoint: Option<OutPoint> = None;
    let mut carry_amount = 0_u64;
    let mut corpus = Sha256::new();
    corpus.update(b"PulseDAG:task39-phase2b-production-valid-corpus:v1");
    let mut pow_attempts_total = 0_u64;
    let mut pow_attempts_max = 0_u64;
    let mut pow_failures = 0_u64;
    let mut max_context_blocks = state.dag.blocks.len();
    let mut max_utxo_entries = state.utxo.utxos.len();
    let mut final_state_root = state.utxo.compute_state_root().map_err(|e| e.to_string())?;
    let mut final_tip = state.dag.genesis_hash.clone();
    let mut final_difficulty_bits = 0_u32;

    for index in 0..blocks {
        let height = state.dag.best_height.saturating_add(1);
        let parent = state
            .dag
            .tips
            .iter()
            .next()
            .cloned()
            .ok_or("validation context has no tip")?;
        let difficulty = consensus_difficulty_snapshot(&state).expected_bits;
        let subsidy = block_subsidy(height);
        let coinbase = build_coinbase_transaction(&address, subsidy, height);
        let coinbase_outpoint = OutPoint {
            txid: coinbase.txid.clone(),
            index: 0,
        };

        let mut transactions = vec![coinbase];
        let next_carry_amount = carry_amount
            .checked_add(subsidy)
            .ok_or("carry amount overflow")?;
        let next_carry_outpoint = if let Some(previous_carry) = carry_outpoint.take() {
            let tx = signed_carry_transaction(
                &key,
                previous_carry,
                coinbase_outpoint,
                &address,
                next_carry_amount,
                height,
            );
            let outpoint = OutPoint {
                txid: tx.txid.clone(),
                index: 0,
            };
            transactions.push(tx);
            outpoint
        } else {
            coinbase_outpoint
        };

        let mut block = build_candidate_block(vec![parent], height, difficulty, transactions);
        block.header.timestamp = height.saturating_mul(60);
        refresh_block_consensus_ids_with_state(&mut block, &state).map_err(|e| e.to_string())?;
        let (header, mined, attempts, _) = dev_mine_header(block.header.clone(), MAX_POW_TRIES);
        pow_attempts_total = pow_attempts_total.saturating_add(attempts);
        pow_attempts_max = pow_attempts_max.max(attempts);
        if !mined {
            pow_failures = pow_failures.saturating_add(1);
            return Err(format!(
                "PoW search failed at block index {index} height {height} after {attempts} attempts"
            ));
        }
        block.header = header;
        refresh_block_consensus_ids(&mut block);

        validate_block(&block, &state).map_err(|e| {
            format!("production validation rejected block index {index} height {height}: {e}")
        })?;

        final_state_root = advance_validated_context(&mut state, &block)?;
        carry_outpoint = Some(next_carry_outpoint);
        carry_amount = next_carry_amount;
        final_tip = block.hash.clone();
        final_difficulty_bits = block.header.difficulty;
        max_context_blocks = max_context_blocks.max(state.dag.blocks.len());
        max_utxo_entries = max_utxo_entries.max(state.utxo.utxos.len());

        corpus.update((index as u64).to_le_bytes());
        corpus.update(block.hash.as_bytes());
        corpus.update([0]);
        corpus.update(block.header.state_root.as_bytes());
        corpus.update(block.header.difficulty.to_le_bytes());
        corpus.update(block.header.nonce.to_le_bytes());
        for tx in &block.transactions {
            corpus.update([0]);
            corpus.update(tx.txid.as_bytes());
        }
    }

    let production_valid = pow_failures == 0 && state.dag.best_height == blocks as u64;
    let mut missing = vec![
        "restart_snapshot_prune_same_corpus",
        "production_valid_parallel_dag_same_corpus",
    ];
    if blocks < MILLION {
        missing.push("million_block_exact_candidate_run");
    }

    let fail_reasons = if production_valid {
        Vec::new()
    } else {
        vec!["not every generated block passed production validation".to_string()]
    };

    Ok(Manifest {
        schema: SCHEMA,
        candidate_sha: args.candidate_sha,
        candidate_tree_sha: args.candidate_tree,
        requested_blocks: blocks,
        production_validated_blocks: blocks,
        corpus_digest: hex::encode(corpus.finalize()),
        elapsed_ms: started.elapsed().as_millis(),
        final_height: state.dag.best_height,
        final_tip,
        final_state_root,
        final_difficulty_bits,
        pow_attempts_total,
        pow_attempts_max,
        pow_failures,
        max_context_blocks,
        max_utxo_entries,
        retained_context_limit: RETAINED_BLOCKS,
        retarget_window_size: retarget_window,
        production_valid_pow_and_state_root_per_block: production_valid,
        bounded_validation_context: true,
        million_block_count_observed: blocks >= MILLION,
        coverage: Coverage {
            completion_eligible: false,
            missing_required_measurements: missing,
        },
        runtime_gate_result: if fail_reasons.is_empty() {
            "PASS".into()
        } else {
            "FAIL".into()
        },
        fail_reasons,
    })
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    let out = args.out.clone();
    let manifest = run(args)?;
    write_manifest(&out, &manifest)?;
    eprintln!(
        "task39 phase2b production-valid replay: blocks={} elapsed_ms={} pow_attempts={} max_context_blocks={} max_utxo_entries={} result={} out={}",
        manifest.production_validated_blocks,
        manifest.elapsed_ms,
        manifest.pow_attempts_total,
        manifest.max_context_blocks,
        manifest.max_utxo_entries,
        manifest.runtime_gate_result,
        out.display()
    );
    if manifest.runtime_gate_result != "PASS" {
        return Err("phase2b production-valid gate failed".into());
    }
    Ok(())
}
