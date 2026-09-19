use pulsedag_core::retarget::expected_difficulty_for_parent;
use pulsedag_core::{
    block_subsidy, build_candidate_block_v2, build_coinbase_transaction_v2,
    canonicalize_block_parents_v2, classify_merge_set_v1,
    commit_ghostdag_v1_metadata_for_activated_v2, compute_block_hash_v2,
    drive_activated_v2_p2p_block_atomically, materialize_authoritative_state_v2, merge_set_digest,
    ordered_dag_digest, rebuild_authoritative_state_v2, selection_digest, state_digest,
    validate_pow_for_protocol, ActivatedV2P2pRuntime, ActivatedV2P2pRuntimeOutcome, Block,
    CandidateBlockV2Spec, ChainState, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

const SCHEMA: &str = "pulsedag.task39.phase2d-parallel-valid.v1";
const CHAIN_ID: &str = "pulsedag-task39-phase2d-parallel-valid";
const DEFAULT_BLOCKS: usize = 4_096;
const SELF_TEST_BLOCKS: usize = 192;
const FORK_INTERVAL: u64 = 64;
const MAX_POW_TRIES: u64 = 200_000;
const MILLION: usize = 1_000_000;

#[derive(Debug)]
struct Args {
    blocks: usize,
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
    state_root: String,
    state_digest: String,
    chain_state_generation: u64,
    canonical_bundle_digest: String,
}

#[derive(Debug, Serialize)]
struct ReplayRun {
    name: &'static str,
    elapsed_ms: u128,
    accepted_events: usize,
    staged_events: usize,
    promoted_events: usize,
    promoted_blocks: usize,
    missing_parent_events: usize,
    retried_events: usize,
    duplicate_events: usize,
    rejected_events: usize,
    final_pending_count: usize,
    final_staged_count: usize,
    finalized_blocks: usize,
    observation: Observation,
}

#[derive(Debug, Serialize)]
struct Topology {
    fork_interval: u64,
    parallel_pairs: usize,
    multi_parent_joins: usize,
    includes_parallel_blocks: bool,
    includes_multi_parent_joins: bool,
    alternate_order_forces_missing_parent_recovery: bool,
}

#[derive(Debug, Serialize)]
struct CorpusContract {
    activated_v2_context_validation: bool,
    activated_v2_runtime_staging_and_promotion: bool,
    production_pow_validation: bool,
    authoritative_ordered_dag_state_replay: bool,
    same_corpus_two_arrival_orders: bool,
    deterministic_timestamps_and_nonce_search: bool,
}

#[derive(Debug, Serialize)]
struct Coverage {
    completion_eligible: bool,
    missing_required_measurements: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema: &'static str,
    candidate_sha: String,
    candidate_tree_sha: String,
    requested_blocks: usize,
    generated_blocks: usize,
    corpus_digest: String,
    corpus_generation_elapsed_ms: u128,
    pow_attempts_total: u64,
    pow_attempts_max: u64,
    pow_failures: u64,
    topology: Topology,
    corpus_contract: CorpusContract,
    runs: Vec<ReplayRun>,
    exact_equivalence: bool,
    production_valid_parallel_dag_same_corpus: bool,
    million_block_count_observed: bool,
    coverage: Coverage,
    runtime_gate_result: String,
    fail_reasons: Vec<String>,
}

#[derive(Default)]
struct PowStats {
    attempts_total: u64,
    attempts_max: u64,
    failures: u64,
}

#[derive(Default)]
struct OutcomeStats {
    accepted_events: usize,
    staged_events: usize,
    promoted_events: usize,
    promoted_blocks: usize,
    missing_parent_events: usize,
    retried_events: usize,
    duplicate_events: usize,
    rejected_events: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut blocks = DEFAULT_BLOCKS;
    let mut out = PathBuf::from("task39-phase2d-parallel-valid.json");
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
    if blocks < 8 {
        return Err("--blocks must be >= 8".into());
    }
    Ok(Args {
        blocks,
        out,
        candidate_sha,
        candidate_tree,
        self_test,
    })
}

fn identity(state: &ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn digest_strings(domain: &str, values: impl IntoIterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for value in values {
        hasher.update([0]);
        hasher.update(value.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn selected_chain_digest(state: &ChainState) -> String {
    digest_strings(
        "PulseDAG:task39-phase2d-selected-chain:v1",
        state
            .dag
            .selected_chain
            .iter()
            .enumerate()
            .map(|(index, hash)| format!("{index}:{hash}")),
    )
}

fn transaction_outcomes_digest(
    state: &ChainState,
    skipped_conflicting_transactions: usize,
) -> String {
    let outcome = if skipped_conflicting_transactions == 0 {
        "applied"
    } else {
        "ordered"
    };
    let mut values = Vec::new();
    for hash in &state.dag.ordered_dag {
        if hash == &state.dag.genesis_hash {
            continue;
        }
        if let Some(block) = state.dag.blocks.get(hash) {
            for transaction in &block.transactions {
                values.push(format!("{hash}|{}|{outcome}", transaction.txid));
            }
        }
    }
    digest_strings("PulseDAG:task39-phase2d-transaction-outcomes:v1", values)
}

fn finalized_block(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    parents: Vec<String>,
    sequence: u64,
    pow_stats: &mut PowStats,
) -> Result<Block, String> {
    let parents = canonicalize_block_parents_v2(&parents).map_err(|error| error.to_string())?;
    let height = parents
        .iter()
        .map(|parent| {
            state
                .dag
                .blocks
                .get(parent)
                .map(|block| block.header.height.saturating_add(1))
                .ok_or_else(|| format!("missing generation parent {parent}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or("candidate has no generation parents")?;
    let timestamp = parents
        .iter()
        .map(|parent| {
            state
                .dag
                .blocks
                .get(parent)
                .map(|block| block.header.timestamp)
                .ok_or_else(|| format!("missing timestamp parent {parent}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(0)
        .saturating_add(60)
        .max(1);
    let coinbase = build_coinbase_transaction_v2(
        &format!("pulse1task39phase2d{:016x}", sequence),
        block_subsidy(height),
        sequence,
        &identity.chain_id,
    )
    .map_err(|error| error.to_string())?;
    let mut block = build_candidate_block_v2(
        CandidateBlockV2Spec {
            parents,
            timestamp,
            height,
            blue_score: 0,
            difficulty: 1,
            state_root: "00".repeat(32),
        },
        vec![coinbase],
        &identity.chain_id,
    )
    .map_err(|error| error.to_string())?;

    let classification =
        classify_merge_set_v1(&block, state).map_err(|error| format!("{error:?}"))?;
    block.header.blue_score = classification.blue_score;
    let selected_parent = classification
        .selected_parent
        .ok_or("generation classification has no selected parent")?;
    block.header.difficulty =
        expected_difficulty_for_parent(state, &selected_parent).ok_or_else(|| {
            format!("missing difficulty context for generation parent {selected_parent}")
        })?;
    block.hash = compute_block_hash_v2(&block.header, &identity.chain_id)
        .map_err(|error| error.to_string())?;

    let mut first_state = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut first_state, identity)
        .map_err(|error| error.to_string())?;
    let first = rebuild_authoritative_state_v2(&first_state).map_err(|error| error.to_string())?;
    block.header.state_root = first.diagnostics.state_root;
    block.hash = compute_block_hash_v2(&block.header, &identity.chain_id)
        .map_err(|error| error.to_string())?;

    let mut final_state = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut final_state, identity)
        .map_err(|error| error.to_string())?;
    let final_replay =
        rebuild_authoritative_state_v2(&final_state).map_err(|error| error.to_string())?;
    if final_replay.diagnostics.state_root != block.header.state_root {
        return Err(format!(
            "unstable generation state root for block {}: {} != {}",
            block.hash, block.header.state_root, final_replay.diagnostics.state_root
        ));
    }
    if final_replay.diagnostics.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(format!(
            "generated block {} is not its authoritative ordered-DAG tip {:?}",
            block.hash, final_replay.diagnostics.ordered_dag_tip
        ));
    }

    let mut mined = false;
    for nonce in 0..=MAX_POW_TRIES {
        block.header.nonce = nonce;
        block.hash =
            compute_block_hash_v2(&block.header, &identity.chain_id).map_err(|error| error.to_string())?;
        pow_stats.attempts_total = pow_stats.attempts_total.saturating_add(1);
        pow_stats.attempts_max = pow_stats.attempts_max.max(nonce.saturating_add(1));
        if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
            mined = true;
            break;
        }
    }
    if !mined {
        pow_stats.failures = pow_stats.failures.saturating_add(1);
        return Err(format!(
            "PoW search failed after {} tries for height {}",
            MAX_POW_TRIES.saturating_add(1),
            height
        ));
    }
    Ok(block)
}

fn selected_tip(state: &ChainState) -> Result<String, String> {
    Ok(state
        .dag
        .selected_chain
        .last()
        .cloned()
        .unwrap_or_else(|| state.dag.genesis_hash.clone()))
}

fn materialize_after_block(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    block: &Block,
) -> Result<ChainState, String> {
    let mut working = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)
        .map_err(|error| error.to_string())?;
    materialize_authoritative_state_v2(&working).map_err(|error| error.to_string())
}

fn generate_corpus(
    count: usize,
) -> Result<(Vec<Block>, ChainState, PowStats, usize, usize), String> {
    let mut state = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    let identity = identity(&state);
    let mut blocks = Vec::with_capacity(count);
    let mut pow_stats = PowStats::default();
    let mut sequence = 1_u64;
    let mut parallel_pairs = 0usize;
    let mut joins = 0usize;

    while blocks.len() < count {
        let parent = selected_tip(&state)?;
        let next_height = state
            .dag
            .blocks
            .get(&parent)
            .map(|block| block.header.height.saturating_add(1))
            .ok_or_else(|| format!("selected tip {parent} has no block"))?;
        let remaining = count.saturating_sub(blocks.len());

        if next_height % FORK_INTERVAL == 0 && remaining >= 3 {
            let main = finalized_block(
                &state,
                &identity,
                vec![parent.clone()],
                sequence,
                &mut pow_stats,
            )?;
            sequence = sequence.saturating_add(1);
            let side = finalized_block(&state, &identity, vec![parent], sequence, &mut pow_stats)?;
            sequence = sequence.saturating_add(1);
            if main.hash == side.hash {
                return Err("parallel generation produced identical block hashes".into());
            }

            let main_state = materialize_after_block(&state, &identity, &main)?;
            let mut pre_anchor = main_state;
            commit_ghostdag_v1_metadata_for_activated_v2(&side, &mut pre_anchor, &identity)
                .map_err(|error| error.to_string())?;

            let anchor = finalized_block(
                &pre_anchor,
                &identity,
                vec![main.hash.clone(), side.hash.clone()],
                sequence,
                &mut pow_stats,
            )?;
            sequence = sequence.saturating_add(1);
            state = materialize_after_block(&pre_anchor, &identity, &anchor)?;

            blocks.push(main);
            blocks.push(side);
            blocks.push(anchor);
            parallel_pairs = parallel_pairs.saturating_add(1);
            joins = joins.saturating_add(1);
        } else {
            let block = finalized_block(&state, &identity, vec![parent], sequence, &mut pow_stats)?;
            sequence = sequence.saturating_add(1);
            state = materialize_after_block(&state, &identity, &block)?;
            blocks.push(block);
        }
    }

    Ok((blocks, state, pow_stats, parallel_pairs, joins))
}

fn corpus_digest(blocks: &[Block]) -> String {
    digest_strings(
        "PulseDAG:task39-phase2d-production-valid-corpus:v1",
        blocks.iter().enumerate().map(|(index, block)| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                index,
                block.hash,
                block.header.height,
                block.header.timestamp,
                block.header.parents.join(","),
                block.header.state_root,
                block.header.difficulty,
                block
                    .transactions
                    .iter()
                    .map(|transaction| transaction.txid.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }),
    )
}

fn canonical_order(len: usize) -> Vec<usize> {
    (0..len).collect()
}

fn alternate_order(blocks: &[Block]) -> Vec<usize> {
    let mut order = Vec::with_capacity(blocks.len());
    let mut index = 0usize;
    while index < blocks.len() {
        if index + 2 < blocks.len()
            && blocks[index].header.height == blocks[index + 1].header.height
            && blocks[index + 2].header.parents.len() >= 2
            && blocks[index + 2]
                .header
                .parents
                .contains(&blocks[index].hash)
            && blocks[index + 2]
                .header
                .parents
                .contains(&blocks[index + 1].hash)
        {
            order.push(index + 2);
            order.push(index + 1);
            order.push(index);
            index += 3;
        } else {
            order.push(index);
            index += 1;
        }
    }
    order
}

fn record_outcome(outcome: &ActivatedV2P2pRuntimeOutcome, stats: &mut OutcomeStats) {
    match outcome {
        ActivatedV2P2pRuntimeOutcome::Accepted { .. } => {
            stats.accepted_events = stats.accepted_events.saturating_add(1)
        }
        ActivatedV2P2pRuntimeOutcome::Staged { .. } => {
            stats.staged_events = stats.staged_events.saturating_add(1)
        }
        ActivatedV2P2pRuntimeOutcome::Promoted {
            promoted_hashes, ..
        } => {
            stats.promoted_events = stats.promoted_events.saturating_add(1);
            stats.promoted_blocks = stats.promoted_blocks.saturating_add(promoted_hashes.len());
        }
        ActivatedV2P2pRuntimeOutcome::MissingParents { .. } => {
            stats.missing_parent_events = stats.missing_parent_events.saturating_add(1)
        }
        ActivatedV2P2pRuntimeOutcome::Duplicate { .. } => {
            stats.duplicate_events = stats.duplicate_events.saturating_add(1)
        }
        ActivatedV2P2pRuntimeOutcome::Rejected { .. } => {
            stats.rejected_events = stats.rejected_events.saturating_add(1)
        }
    }
}

fn observe(state: &ChainState) -> Result<Observation, String> {
    let replay = rebuild_authoritative_state_v2(state).map_err(|error| error.to_string())?;
    let state_root = state
        .utxo
        .compute_state_root()
        .map_err(|error| error.to_string())?;
    if state_root != replay.diagnostics.state_root {
        return Err(format!(
            "live/replayed state-root mismatch: {} != {}",
            state_root, replay.diagnostics.state_root
        ));
    }
    if state.dag.ordered_dag != replay.ordered_dag.blocks {
        return Err("live ordered DAG differs from authoritative replay".into());
    }
    let selected_chain_digest = selected_chain_digest(state);
    let selection = selection_digest(state);
    let merge_set = merge_set_digest(state);
    let ordered = ordered_dag_digest(state);
    let transaction_outcomes =
        transaction_outcomes_digest(state, replay.diagnostics.skipped_conflicting_transactions);
    let state_commitment = state_digest(state).map_err(|error| error.to_string())?;
    let selected_tip = state.dag.selected_chain.last().cloned();
    let canonical_bundle_digest = digest_strings(
        "PulseDAG:task39-phase2d-canonical-bundle:v1",
        [
            selected_tip.clone().unwrap_or_default(),
            selected_chain_digest.clone(),
            selection.clone(),
            merge_set.clone(),
            ordered.clone(),
            transaction_outcomes.clone(),
            replay.diagnostics.applied_transactions.to_string(),
            replay
                .diagnostics
                .skipped_conflicting_transactions
                .to_string(),
            state_root.clone(),
            state_commitment.clone(),
            state.utxo.utxos.len().to_string(),
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
        transaction_outcomes_digest: transaction_outcomes,
        applied_transactions: replay.diagnostics.applied_transactions,
        skipped_conflicting_transactions: replay.diagnostics.skipped_conflicting_transactions,
        utxo_entries: state.utxo.utxos.len(),
        state_root,
        state_digest: state_commitment,
        chain_state_generation: state.chain_state_generation,
        canonical_bundle_digest,
    })
}

fn replay(blocks: &[Block], order: &[usize], name: &'static str) -> Result<ReplayRun, String> {
    let started = Instant::now();
    let mut state = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    let identity = identity(&state);
    let mut runtime = ActivatedV2P2pRuntime::default();
    let mut stats = OutcomeStats::default();

    for index in order {
        let block = blocks
            .get(*index)
            .cloned()
            .ok_or_else(|| format!("replay {name} index {index} outside corpus"))?;
        let result = drive_activated_v2_p2p_block_atomically(
            block,
            &mut state,
            &mut runtime,
            &identity,
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .map_err(|error| format!("replay {name} runtime error at index {index}: {error}"))?;
        record_outcome(&result.primary, &mut stats);
        for outcome in &result.retried {
            stats.retried_events = stats.retried_events.saturating_add(1);
            record_outcome(outcome, &mut stats);
        }
    }

    let finalized_blocks = blocks
        .iter()
        .filter(|block| state.dag.blocks.contains_key(&block.hash))
        .count();
    if runtime.pending_len() != 0 || !runtime.staging().is_empty() {
        return Err(format!(
            "replay {name} left pending={} staged={}",
            runtime.pending_len(),
            runtime.staging().len()
        ));
    }
    if finalized_blocks != blocks.len() {
        return Err(format!(
            "replay {name} finalized {finalized_blocks}/{} corpus blocks",
            blocks.len()
        ));
    }
    if stats.rejected_events != 0 || stats.duplicate_events != 0 {
        return Err(format!(
            "replay {name} saw rejected={} duplicate={} events",
            stats.rejected_events, stats.duplicate_events
        ));
    }
    if stats.accepted_events.saturating_add(stats.promoted_blocks) != blocks.len() {
        return Err(format!(
            "replay {name} authoritative event accounting mismatch: accepted={} promoted_blocks={} corpus={}",
            stats.accepted_events,
            stats.promoted_blocks,
            blocks.len()
        ));
    }

    let observation = observe(&state)?;
    Ok(ReplayRun {
        name,
        elapsed_ms: started.elapsed().as_millis(),
        accepted_events: stats.accepted_events,
        staged_events: stats.staged_events,
        promoted_events: stats.promoted_events,
        promoted_blocks: stats.promoted_blocks,
        missing_parent_events: stats.missing_parent_events,
        retried_events: stats.retried_events,
        duplicate_events: stats.duplicate_events,
        rejected_events: stats.rejected_events,
        final_pending_count: runtime.pending_len(),
        final_staged_count: runtime.staging().len(),
        finalized_blocks,
        observation,
    })
}

fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn run(args: Args) -> Result<Manifest, String> {
    let blocks_requested = if args.self_test {
        SELF_TEST_BLOCKS
    } else {
        args.blocks
    };
    let generation_started = Instant::now();
    let (blocks, generated_state, pow_stats, parallel_pairs, joins) =
        generate_corpus(blocks_requested)?;
    let corpus_generation_elapsed_ms = generation_started.elapsed().as_millis();
    let digest = corpus_digest(&blocks);

    let generated_observation = observe(&generated_state)?;
    let canonical = replay(&blocks, &canonical_order(blocks.len()), "canonical")?;
    let alternate = replay(&blocks, &alternate_order(&blocks), "anchor-first")?;
    let exact_equivalence = canonical.observation == alternate.observation
        && canonical.observation.canonical_bundle_digest
            == generated_observation.canonical_bundle_digest;

    let production_valid = exact_equivalence
        && parallel_pairs > 0
        && joins == parallel_pairs
        && canonical.finalized_blocks == blocks.len()
        && alternate.finalized_blocks == blocks.len()
        && canonical.rejected_events == 0
        && alternate.rejected_events == 0
        && canonical.observation.skipped_conflicting_transactions == 0
        && alternate.observation.skipped_conflicting_transactions == 0
        && alternate.missing_parent_events > 0
        && alternate.retried_events > 0;

    let mut missing = vec!["restart_snapshot_prune_same_parallel_corpus"];
    if blocks.len() < MILLION {
        missing.push("million_block_exact_candidate_run");
    }
    let mut fail_reasons = Vec::new();
    if !exact_equivalence {
        fail_reasons
            .push("canonical and alternate arrival histories did not converge exactly".into());
    }
    if parallel_pairs == 0 || joins != parallel_pairs {
        fail_reasons.push("parallel fork/join coverage was not exercised".into());
    }
    if canonical.finalized_blocks != blocks.len() || alternate.finalized_blocks != blocks.len() {
        fail_reasons.push("not every corpus block finalized in both replay histories".into());
    }
    if canonical.rejected_events != 0 || alternate.rejected_events != 0 {
        fail_reasons.push("at least one production runtime replay rejected a corpus block".into());
    }
    if canonical.observation.skipped_conflicting_transactions != 0
        || alternate.observation.skipped_conflicting_transactions != 0
    {
        fail_reasons.push("coinbase-only corpus produced unexpected ordered-DAG conflicts".into());
    }
    if alternate.missing_parent_events == 0 || alternate.retried_events == 0 {
        fail_reasons.push("alternate replay did not exercise missing-parent recovery".into());
    }

    Ok(Manifest {
        schema: SCHEMA,
        candidate_sha: args.candidate_sha,
        candidate_tree_sha: args.candidate_tree,
        requested_blocks: blocks_requested,
        generated_blocks: blocks.len(),
        corpus_digest: digest,
        corpus_generation_elapsed_ms,
        pow_attempts_total: pow_stats.attempts_total,
        pow_attempts_max: pow_stats.attempts_max,
        pow_failures: pow_stats.failures,
        topology: Topology {
            fork_interval: FORK_INTERVAL,
            parallel_pairs,
            multi_parent_joins: joins,
            includes_parallel_blocks: parallel_pairs > 0,
            includes_multi_parent_joins: joins > 0,
            alternate_order_forces_missing_parent_recovery: true,
        },
        corpus_contract: CorpusContract {
            activated_v2_context_validation: true,
            activated_v2_runtime_staging_and_promotion: true,
            production_pow_validation: true,
            authoritative_ordered_dag_state_replay: true,
            same_corpus_two_arrival_orders: true,
            deterministic_timestamps_and_nonce_search: true,
        },
        runs: vec![canonical, alternate],
        exact_equivalence,
        production_valid_parallel_dag_same_corpus: production_valid,
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
    })
}

fn main() -> Result<(), String> {
    let args = parse_args()?;
    let out = args.out.clone();
    let manifest = run(args)?;
    write_manifest(&out, &manifest)?;
    eprintln!(
        "task39 phase2d parallel-valid replay: blocks={} forks={} joins={} exact={} production_valid={} result={} out={}",
        manifest.generated_blocks,
        manifest.topology.parallel_pairs,
        manifest.topology.multi_parent_joins,
        manifest.exact_equivalence,
        manifest.production_valid_parallel_dag_same_corpus,
        manifest.runtime_gate_result,
        out.display()
    );
    if manifest.runtime_gate_result != "PASS" {
        return Err("phase2d production-valid parallel replay gate failed".into());
    }
    Ok(())
}
