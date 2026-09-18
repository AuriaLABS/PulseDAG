use pulsedag_core::retarget::CONSENSUS_POW_LIMIT_BITS;
use pulsedag_core::{
    accept_block_to_dag_metadata, build_candidate_block, build_coinbase_transaction,
    commit_rebuilt_state, merge_set_digest, ordered_dag_digest, rebuild_state_from_ordered_dag,
    refresh_block_consensus_ids, refresh_ordered_dag_phase, refresh_selected_chain_phase,
    selection_digest, state_digest, Block, ChainState, ConsensusMode, SelectedParentPolicy,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

const SCHEMA: &str = "pulsedag.task39.phase2a-replay.v1";
const CHAIN_ID: &str = "pulsedag-task39-phase2a-replay";
const FORK_INTERVAL: u64 = 64;
const DEFAULT_WINDOW: usize = 32;
const MILLION: usize = 1_000_000;

#[derive(Debug)]
struct Args {
    blocks: usize,
    window: usize,
    out: PathBuf,
    candidate_sha: String,
    candidate_tree: String,
    self_test: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct Observation {
    selected_tip: Option<String>,
    selected_chain_len: usize,
    selected_chain_digest: String,
    selection_digest: String,
    merge_set_digest: String,
    ordered_dag_len: usize,
    ordered_dag_digest: String,
    transaction_outcomes_digest: String,
    applied_transactions: usize,
    skipped_conflicting_transactions: usize,
    utxo_entries: usize,
    state_digest: String,
    state_root: String,
    canonical_bundle_digest: String,
}

#[derive(Debug, Serialize)]
struct ReplayRun {
    name: String,
    elapsed_ms: u128,
    buffered_orphan_peak: usize,
    observation: Observation,
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema: &'static str,
    candidate_sha: String,
    candidate_tree_sha: String,
    requested_blocks: usize,
    generated_blocks: usize,
    corpus_digest: String,
    topology: Topology,
    corpus_contract: CorpusContract,
    runs: Vec<ReplayRun>,
    exact_equivalence: bool,
    million_block_count_observed: bool,
    coverage: Coverage,
    runtime_gate_result: String,
    fail_reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Topology {
    chain_id: &'static str,
    fork_interval: u64,
    permutation_window: usize,
    includes_parallel_blocks: bool,
    includes_multi_parent_joins: bool,
}

#[derive(Debug, Serialize)]
struct CorpusContract {
    metadata_replay_uses_production_ghostdag_classifier: bool,
    canonical_rebuild_uses_production_ordered_dag_state_rebuild: bool,
    transaction_model: &'static str,
    production_valid_pow_and_state_root_per_block: bool,
    limitation: &'static str,
}

#[derive(Debug, Serialize)]
struct Coverage {
    completion_eligible: bool,
    missing_required_measurements: Vec<&'static str>,
}

fn parse_args() -> Result<Args, String> {
    let mut blocks = 4_096usize;
    let mut window = DEFAULT_WINDOW;
    let mut out = PathBuf::from("task39-phase2a-replay.json");
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
            "--window" => {
                window = it
                    .next()
                    .ok_or("missing --window value")?
                    .parse()
                    .map_err(|_| "invalid --window")?
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
    if blocks < 8 {
        return Err("--blocks must be >= 8".into());
    }
    if window < 2 {
        return Err("--window must be >= 2".into());
    }
    Ok(Args {
        blocks,
        window,
        out,
        candidate_sha,
        candidate_tree,
        self_test,
    })
}

fn ghostdag_state() -> ChainState {
    let mut state = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    state.dag.consensus_mode = ConsensusMode::GhostdagDev;
    state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
    state.dag.merge_set_k = 8;
    state
}

fn make_block(parents: Vec<String>, height: u64, sequence: u64) -> Block {
    let tx = build_coinbase_transaction(&format!("phase2a-miner-{}", sequence % 8), 50, sequence);
    let mut block = build_candidate_block(parents, height, CONSENSUS_POW_LIMIT_BITS, vec![tx]);
    block.header.timestamp = 1_700_000_000_u64.saturating_add(sequence);
    block.header.nonce = sequence;
    block.header.state_root = format!("phase2a-metadata-state-{sequence:016x}");
    refresh_block_consensus_ids(&mut block);
    block
}

fn accept_metadata(state: &mut ChainState, block: &Block) -> Result<(), String> {
    accept_block_to_dag_metadata(block, state).map_err(|e| e.to_string())
}

fn generate_corpus(count: usize) -> Result<Vec<Block>, String> {
    let mut state = ghostdag_state();
    let mut blocks = Vec::with_capacity(count);
    let mut tip = state.dag.genesis_hash.clone();
    let mut tip_height = 0_u64;
    let mut pending_join: Option<(String, String, u64)> = None;
    let mut sequence = 1_u64;

    while blocks.len() < count {
        if let Some((left, right, fork_height)) = pending_join.take() {
            let mut parents = vec![left, right];
            parents.sort();
            let block = make_block(parents, fork_height.saturating_add(1), sequence);
            accept_metadata(&mut state, &block)?;
            tip = block.hash.clone();
            tip_height = block.header.height;
            blocks.push(block);
            sequence = sequence.saturating_add(1);
            continue;
        }

        let remaining = count.saturating_sub(blocks.len());
        let next_height = tip_height.saturating_add(1);
        if next_height.is_multiple_of(FORK_INTERVAL) && remaining >= 2 {
            let left = make_block(vec![tip.clone()], next_height, sequence);
            sequence = sequence.saturating_add(1);
            let right = make_block(vec![tip.clone()], next_height, sequence);
            sequence = sequence.saturating_add(1);
            accept_metadata(&mut state, &left)?;
            accept_metadata(&mut state, &right)?;
            pending_join = Some((left.hash.clone(), right.hash.clone(), next_height));
            blocks.push(left);
            if blocks.len() < count {
                blocks.push(right);
            }
        } else {
            let block = make_block(vec![tip.clone()], next_height, sequence);
            sequence = sequence.saturating_add(1);
            accept_metadata(&mut state, &block)?;
            tip = block.hash.clone();
            tip_height = next_height;
            blocks.push(block);
        }
    }

    blocks.truncate(count);
    Ok(blocks)
}

fn digest_strings(domain: &str, values: impl IntoIterator<Item = String>) -> String {
    let mut h = Sha256::new();
    h.update(domain.as_bytes());
    for value in values {
        h.update([0]);
        h.update(value.as_bytes());
    }
    hex::encode(h.finalize())
}

fn corpus_digest(blocks: &[Block]) -> String {
    digest_strings(
        "PulseDAG:task39-phase2a-corpus:v1",
        blocks.iter().enumerate().map(|(i, block)| {
            format!(
                "{}|{}|{}|{}|{}|{}",
                i,
                block.hash,
                block.header.height,
                block.header.timestamp,
                block.header.parents.join(","),
                block
                    .transactions
                    .iter()
                    .map(|tx| tx.txid.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }),
    )
}

fn canonical_order(len: usize) -> Vec<usize> {
    (0..len).collect()
}

fn window_permutation(len: usize, window: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(len);
    let mut start = 0usize;
    let mut chunk = 0usize;
    while start < len {
        let end = (start + window).min(len);
        let mut indices = (start..end).collect::<Vec<_>>();
        if chunk.is_multiple_of(2) {
            indices.reverse();
        } else if !indices.is_empty() {
            let rotate = (chunk * 7) % indices.len();
            indices.rotate_left(rotate);
        }
        out.extend(indices);
        start = end;
        chunk += 1;
    }
    out
}

fn parents_known(block: &Block, state: &ChainState) -> bool {
    block
        .header
        .parents
        .iter()
        .all(|p| state.dag.blocks.contains_key(p))
}

fn replay(blocks: &[Block], order: &[usize], name: &str) -> Result<ReplayRun, String> {
    let started = Instant::now();
    let mut state = ghostdag_state();
    let mut pending = VecDeque::<usize>::new();
    let mut peak = 0usize;

    for index in order {
        pending.push_back(*index);
        peak = peak.max(pending.len());
        loop {
            let pass = pending.len();
            let mut progressed = false;
            for _ in 0..pass {
                let idx = pending.pop_front().expect("pending length checked");
                let block = &blocks[idx];
                if parents_known(block, &state) {
                    accept_metadata(&mut state, block)?;
                    progressed = true;
                } else {
                    pending.push_back(idx);
                }
            }
            peak = peak.max(pending.len());
            if !progressed {
                break;
            }
        }
    }

    if !pending.is_empty() {
        return Err(format!(
            "replay {name} left {} unresolved blocks",
            pending.len()
        ));
    }

    refresh_selected_chain_phase(&mut state);
    refresh_ordered_dag_phase(&mut state);
    let rebuilt = rebuild_state_from_ordered_dag(&state).map_err(|e| e.to_string())?;
    let applied_transactions = rebuilt.diagnostics.applied_transactions;
    let skipped_conflicting_transactions = rebuilt.diagnostics.skipped_conflicting_transactions;
    commit_rebuilt_state(&mut state, rebuilt);
    let observation = observe(
        &state,
        applied_transactions,
        skipped_conflicting_transactions,
    )?;

    Ok(ReplayRun {
        name: name.to_string(),
        elapsed_ms: started.elapsed().as_millis(),
        buffered_orphan_peak: peak,
        observation,
    })
}

fn observe(
    state: &ChainState,
    applied_transactions: usize,
    skipped_conflicting_transactions: usize,
) -> Result<Observation, String> {
    let selected_chain_digest = digest_strings(
        "PulseDAG:task39-phase2a-selected-chain:v1",
        state.dag.selected_chain.iter().cloned(),
    );
    let genesis_hash = state.dag.genesis_hash.clone();
    let transaction_outcomes_digest = digest_strings(
        "PulseDAG:task39-phase2a-transaction-outcomes:v1",
        state
            .dag
            .ordered_dag
            .iter()
            .filter(|hash| **hash != genesis_hash)
            .flat_map(|hash| {
                state
                    .dag
                    .blocks
                    .get(hash)
                    .into_iter()
                    .flat_map(move |block| {
                        block
                            .transactions
                            .iter()
                            .map(move |tx| format!("{hash}|{}|applied", tx.txid))
                    })
            }),
    );
    let selection = selection_digest(state);
    let merge_set = merge_set_digest(state);
    let ordered = ordered_dag_digest(state);
    let state_commitment = state_digest(state).map_err(|e| e.to_string())?;
    let state_root = state.utxo.compute_state_root().map_err(|e| e.to_string())?;
    let selected_tip = pulsedag_core::preferred_tip_hash(state);
    let canonical_bundle_digest = digest_strings(
        "PulseDAG:task39-phase2a-canonical-bundle:v1",
        [
            selected_tip.clone().unwrap_or_default(),
            selected_chain_digest.clone(),
            selection.clone(),
            merge_set.clone(),
            ordered.clone(),
            transaction_outcomes_digest.clone(),
            state_commitment.clone(),
            state_root.clone(),
            applied_transactions.to_string(),
            skipped_conflicting_transactions.to_string(),
        ],
    );

    Ok(Observation {
        selected_tip,
        selected_chain_len: state.dag.selected_chain.len(),
        selected_chain_digest,
        selection_digest: selection,
        merge_set_digest: merge_set,
        ordered_dag_len: state.dag.ordered_dag.len(),
        ordered_dag_digest: ordered,
        transaction_outcomes_digest,
        applied_transactions,
        skipped_conflicting_transactions,
        utxo_entries: state.utxo.utxos.len(),
        state_digest: state_commitment,
        state_root,
        canonical_bundle_digest,
    })
}

fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(path, [bytes, b"\n".to_vec()].concat()).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    let mut args = parse_args()?;
    if args.self_test {
        args.blocks = 128;
        args.window = 16;
    }

    let generation_started = Instant::now();
    let blocks = generate_corpus(args.blocks)?;
    let generation_ms = generation_started.elapsed().as_millis();
    let corpus = corpus_digest(&blocks);

    let canonical = replay(&blocks, &canonical_order(blocks.len()), "canonical")?;
    let permuted = replay(
        &blocks,
        &window_permutation(blocks.len(), args.window),
        "window_permuted_orphan_buffered",
    )?;
    let exact_equivalence = canonical.observation == permuted.observation;
    let mut missing = vec![
        "production_valid_pow_and_state_root_per_block",
        "restart_snapshot_prune_same_corpus",
    ];
    if blocks.len() < MILLION {
        missing.push("million_block_exact_candidate_run");
    }

    let fail_reasons = if exact_equivalence {
        Vec::new()
    } else {
        vec!["equivalent arrival histories produced different canonical observations".to_string()]
    };

    let manifest = Manifest {
        schema: SCHEMA,
        candidate_sha: args.candidate_sha,
        candidate_tree_sha: args.candidate_tree,
        requested_blocks: args.blocks,
        generated_blocks: blocks.len(),
        corpus_digest: corpus,
        topology: Topology {
            chain_id: CHAIN_ID,
            fork_interval: FORK_INTERVAL,
            permutation_window: args.window,
            includes_parallel_blocks: true,
            includes_multi_parent_joins: true,
        },
        corpus_contract: CorpusContract {
            metadata_replay_uses_production_ghostdag_classifier: true,
            canonical_rebuild_uses_production_ordered_dag_state_rebuild: true,
            transaction_model: "unique coinbase-only transactions; final canonical rebuild applies production transaction state transition",
            production_valid_pow_and_state_root_per_block: false,
            limitation: "Phase 2a is a scalable metadata-replay foundation. It does not claim every generated header satisfies PoW/state-root validation.",
        },
        runs: vec![canonical, permuted],
        exact_equivalence,
        million_block_count_observed: blocks.len() >= MILLION,
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

    write_manifest(&args.out, &manifest)?;
    eprintln!(
        "task39 phase2a replay: blocks={} generation_ms={} equivalent={} out={}",
        blocks.len(),
        generation_ms,
        manifest.exact_equivalence,
        args.out.display()
    );
    if manifest.runtime_gate_result != "PASS" {
        return Err("phase2a replay gate failed".into());
    }
    Ok(())
}
