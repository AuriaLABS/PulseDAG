use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_block_to_dag_metadata, address_from_public_key, apply_transaction, block_subsidy,
    build_candidate_block, build_coinbase_transaction, compute_txid, consensus_difficulty_snapshot,
    dev_mine_header, refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
    signing_message, state_digest, validate_block, Block, ChainState, OutPoint, Transaction,
    TxInput, TxOutput,
};
use pulsedag_storage::Storage;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const SCHEMA: &str = "pulsedag.task39.phase2c-snapshot-prune.v1";
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
    final_state_digest: String,
    persisted_blocks_before_prune: usize,
    retained_blocks_after_prune: usize,
    pruned_blocks: usize,
    snapshot_exists_before_prune: bool,
    snapshot_export_restore_guarantees_explicit: bool,
    cold_restart_state_digest_matches: bool,
    cold_restart_state_root_matches: bool,
    cold_restart_tip_matches: bool,
    imported_snapshot_state_digest_matches: bool,
    imported_snapshot_state_root_matches: bool,
    imported_snapshot_tip_matches: bool,
    imported_snapshot_survives_cold_restart: bool,
    source_chain_anchor_valid: bool,
    restored_chain_anchor_valid: bool,
    max_context_blocks: usize,
    max_utxo_entries: usize,
    retained_context_limit: usize,
    retarget_window_size: usize,
    production_valid_pow_and_state_root_per_block: bool,
    restart_snapshot_prune_same_corpus: bool,
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
    let mut out = PathBuf::from("task39-phase2c-snapshot-prune.json");
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
    if blocks <= RETAINED_BLOCKS {
        return Err(format!(
            "--blocks must be > retained context limit {RETAINED_BLOCKS}"
        ));
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

fn advance_validated_context(state: &mut ChainState, block: &Block) -> Result<String, String> {
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

fn temp_db_path(label: &str) -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    env::temp_dir()
        .join(format!("pulsedag-task39-phase2c-{label}-{unique}"))
        .to_string_lossy()
        .into_owned()
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
    if blocks <= RETAINED_BLOCKS {
        return Err("self-test block count must exceed retained context limit".into());
    }

    let started = Instant::now();
    let source_path = temp_db_path("source");
    let restore_path = temp_db_path("restore");
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

    let source = Storage::open(&source_path).map_err(|e| e.to_string())?;
    let mut carry_outpoint: Option<OutPoint> = None;
    let mut carry_amount = 0_u64;
    let mut corpus = Sha256::new();
    corpus.update(b"PulseDAG:task39-phase2b-production-valid-corpus:v1");
    let mut max_context_blocks = state.dag.blocks.len();
    let mut max_utxo_entries = state.utxo.utxos.len();
    let mut final_state_root = state.utxo.compute_state_root().map_err(|e| e.to_string())?;
    let mut final_tip = state.dag.genesis_hash.clone();

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
        if !mined {
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
        source
            .persist_block_and_chain_state(&block, &state)
            .map_err(|e| e.to_string())?;

        carry_outpoint = Some(next_carry_outpoint);
        carry_amount = next_carry_amount;
        final_tip = block.hash.clone();
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

    let corpus_digest = hex::encode(corpus.finalize());
    let final_state_digest = state_digest(&state).map_err(|e| e.to_string())?;
    let snapshot_exists_before_prune = source.snapshot_exists().map_err(|e| e.to_string())?;
    let persisted_blocks_before_prune = source.list_blocks().map_err(|e| e.to_string())?.len();
    let retained_hashes = state.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
    let generation = source
        .accepted_storage_generation()
        .map_err(|e| e.to_string())?;
    let pruned_blocks = source
        .commit_compact_prune(&state, &retained_hashes, generation)
        .map_err(|e| e.to_string())?;
    let retained_blocks_after_prune = source.list_blocks().map_err(|e| e.to_string())?.len();
    let source_chain_anchor_valid = source
        .chain_anchor_valid(&state)
        .map_err(|e| e.to_string())?;
    let (snapshot_bundle, snapshot_report) = source
        .export_snapshot_bundle(Some(CHAIN_ID))
        .map_err(|e| e.to_string())?;
    let snapshot_export_restore_guarantees_explicit = snapshot_report.restore_guarantees_explicit;
    drop(source);

    let reopened = Storage::open(&source_path).map_err(|e| e.to_string())?;
    let cold = reopened
        .load_chain_state()
        .map_err(|e| e.to_string())?
        .ok_or("cold restart did not load chain state")?;
    let cold_digest = state_digest(&cold).map_err(|e| e.to_string())?;
    let cold_root = cold.utxo.compute_state_root().map_err(|e| e.to_string())?;
    let cold_tip = cold
        .dag
        .ordered_dag_tip
        .clone()
        .ok_or("cold restart state has no ordered DAG tip")?;
    let cold_restart_state_digest_matches = cold_digest == final_state_digest;
    let cold_restart_state_root_matches = cold_root == final_state_root;
    let cold_restart_tip_matches = cold_tip == final_tip;
    drop(reopened);

    let restore = Storage::open(&restore_path).map_err(|e| e.to_string())?;
    let import_report = restore
        .import_snapshot_bundle(snapshot_bundle, Some(CHAIN_ID))
        .map_err(|e| e.to_string())?;
    if !import_report.restore_guarantees_explicit {
        return Err("snapshot import report did not provide explicit restore guarantees".into());
    }
    let imported = restore
        .load_chain_state()
        .map_err(|e| e.to_string())?
        .ok_or("snapshot import did not persist chain state")?;
    let imported_digest = state_digest(&imported).map_err(|e| e.to_string())?;
    let imported_root = imported
        .utxo
        .compute_state_root()
        .map_err(|e| e.to_string())?;
    let imported_tip = imported
        .dag
        .ordered_dag_tip
        .clone()
        .ok_or("imported snapshot has no ordered DAG tip")?;
    let imported_snapshot_state_digest_matches = imported_digest == final_state_digest;
    let imported_snapshot_state_root_matches = imported_root == final_state_root;
    let imported_snapshot_tip_matches = imported_tip == final_tip;
    let restored_chain_anchor_valid = restore
        .chain_anchor_valid(&imported)
        .map_err(|e| e.to_string())?;
    drop(restore);

    let restore = Storage::open(&restore_path).map_err(|e| e.to_string())?;
    let imported_after_restart = restore
        .load_chain_state()
        .map_err(|e| e.to_string())?
        .ok_or("restored snapshot did not survive cold restart")?;
    let imported_snapshot_survives_cold_restart =
        state_digest(&imported_after_restart).map_err(|e| e.to_string())? == final_state_digest
            && imported_after_restart
                .utxo
                .compute_state_root()
                .map_err(|e| e.to_string())?
                == final_state_root
            && imported_after_restart.dag.ordered_dag_tip.as_ref() == Some(&final_tip);
    drop(restore);

    let restart_snapshot_prune_same_corpus = snapshot_exists_before_prune
        && snapshot_export_restore_guarantees_explicit
        && pruned_blocks > 0
        && persisted_blocks_before_prune == blocks
        && retained_blocks_after_prune == retained_hashes.len()
        && cold_restart_state_digest_matches
        && cold_restart_state_root_matches
        && cold_restart_tip_matches
        && imported_snapshot_state_digest_matches
        && imported_snapshot_state_root_matches
        && imported_snapshot_tip_matches
        && imported_snapshot_survives_cold_restart
        && source_chain_anchor_valid
        && restored_chain_anchor_valid;

    let mut missing = vec!["production_valid_parallel_dag_same_corpus"];
    if blocks < MILLION {
        missing.push("million_block_exact_candidate_run");
    }
    let fail_reasons = if restart_snapshot_prune_same_corpus {
        Vec::new()
    } else {
        vec![
            "snapshot/prune/restart/restore contract did not hold on the production-valid corpus"
                .to_string(),
        ]
    };

    let manifest = Manifest {
        schema: SCHEMA,
        candidate_sha: args.candidate_sha,
        candidate_tree_sha: args.candidate_tree,
        requested_blocks: blocks,
        production_validated_blocks: blocks,
        corpus_digest,
        elapsed_ms: started.elapsed().as_millis(),
        final_height: state.dag.best_height,
        final_tip,
        final_state_root,
        final_state_digest,
        persisted_blocks_before_prune,
        retained_blocks_after_prune,
        pruned_blocks,
        snapshot_exists_before_prune,
        snapshot_export_restore_guarantees_explicit,
        cold_restart_state_digest_matches,
        cold_restart_state_root_matches,
        cold_restart_tip_matches,
        imported_snapshot_state_digest_matches,
        imported_snapshot_state_root_matches,
        imported_snapshot_tip_matches,
        imported_snapshot_survives_cold_restart,
        source_chain_anchor_valid,
        restored_chain_anchor_valid,
        max_context_blocks,
        max_utxo_entries,
        retained_context_limit: RETAINED_BLOCKS,
        retarget_window_size: retarget_window,
        production_valid_pow_and_state_root_per_block: true,
        restart_snapshot_prune_same_corpus,
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
    };

    let _ = fs::remove_dir_all(&source_path);
    let _ = fs::remove_dir_all(&restore_path);
    Ok(manifest)
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    let out = args.out.clone();
    let manifest = run(args)?;
    write_manifest(&out, &manifest)?;
    eprintln!(
        "task39 phase2c snapshot/prune: blocks={} pruned={} retained={} same_corpus={} elapsed_ms={} result={} out={}",
        manifest.production_validated_blocks,
        manifest.pruned_blocks,
        manifest.retained_blocks_after_prune,
        manifest.restart_snapshot_prune_same_corpus,
        manifest.elapsed_ms,
        manifest.runtime_gate_result,
        out.display()
    );
    if manifest.runtime_gate_result != "PASS" {
        return Err("phase2c snapshot/prune gate failed".into());
    }
    Ok(())
}
