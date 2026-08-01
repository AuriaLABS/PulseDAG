#!/usr/bin/env python3
"""One-shot migration for the v2.4.0 consensus candidate.

This script is intentionally removed by the workflow that executes it.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1))


def regex_once(path: str, pattern: str, replacement: str) -> None:
    file = Path(path)
    text = file.read_text()
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE | re.DOTALL)
    if count != 1:
        raise SystemExit(f"{path}: regex expected one match, found {count}: {pattern[:120]!r}")
    file.write_text(updated)


def replace_test(path: str, name: str, body: str) -> None:
    file = Path(path)
    text = file.read_text()
    pattern = re.compile(rf"(?ms)^    #\[test\]\n    fn {re.escape(name)}\(\) \{{.*?^    \}}\n")
    updated, count = pattern.subn(body.rstrip() + "\n", text, count=1)
    if count != 1:
        raise SystemExit(f"{path}: test function not found: {name}")
    file.write_text(updated)


def extract_embedded_python(commit: str, workflow_path: str) -> str:
    workflow = subprocess.check_output(["git", "show", f"{commit}:{workflow_path}"], text=True)
    marker = "          python3 - <<'PY'\n"
    start = workflow.index(marker) + len(marker)
    end = workflow.index("\n          PY\n", start)
    embedded = workflow[start:end]
    return "\n".join(
        line[10:] if line.startswith("          ") else line
        for line in embedded.splitlines()
    )


script = extract_embedded_python(
    "4e4484c3086ade4f42a11edcedee078c822e3d0c",
    ".github/workflows/one_shot_v2_4_fixture_fix.yml",
)
script = script.replace(
    "build_candidate_block, build_coinbase_transaction, dev_mine_header,\\n            refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,\\n",
    "build_candidate_block, build_coinbase_transaction,\\n            refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,\\n",
)
script = script.replace(
    "let (header, mined, _, _) = dev_mine_header(",
    "let (header, mined, _, _) = crate::dev_mine_header(",
)
exec(compile(script, "first_pass_fixture_migration.py", "exec"), {})

accept = "crates/pulsedag-core/src/accept.rs"
replace_test(
    accept,
    "accepts_block_with_valid_pow",
    '''    #[test]
    fn accepts_block_with_valid_pow() {
        let mut state = init_chain_state("test".to_string());
        let block = valid_acceptance_block(&state, "valid-pow", 1);

        assert!(accept_block(block, &mut state, AcceptSource::P2p).is_ok());
    }
''',
)
replace_test(
    accept,
    "duplicate_block_returns_duplicate_outcome",
    '''    #[test]
    fn duplicate_block_returns_duplicate_outcome() {
        let mut state = init_chain_state("test".to_string());
        let block = valid_acceptance_block(&state, "duplicate", 1);
        assert!(accept_block(block.clone(), &mut state, AcceptSource::P2p).is_ok());
        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::Duplicate);
    }
''',
)
replace_test(
    accept,
    "invalid_transaction_in_peer_block_returns_invalid_transaction_outcome",
    '''    #[test]
    fn invalid_transaction_in_peer_block_returns_invalid_transaction_outcome() {
        let mut state = init_chain_state("test".to_string());
        let block = invalid_transaction_acceptance_block(&state);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::InvalidTransaction);
    }
''',
)
replace_test(
    accept,
    "peer_block_and_mining_submit_share_canonical_acceptance_outcomes",
    '''    #[test]
    fn peer_block_and_mining_submit_share_canonical_acceptance_outcomes() {
        let mut peer_state = init_chain_state("agreement".to_string());
        let mut mining_state = init_chain_state("agreement".to_string());
        let block = valid_acceptance_block(&peer_state, "agreement", 1);

        let peer_outcome =
            accept_block_with_result(block.clone(), &mut peer_state, AcceptSource::P2p);
        let mining_outcome =
            accept_block_with_result(block, &mut mining_state, AcceptSource::LocalMining);

        assert_eq!(peer_outcome, BlockAcceptanceResult::Accepted);
        assert_eq!(peer_outcome, mining_outcome);
        assert!(peer_state
            .dag
            .blocks
            .contains_key(&peer_state.dag.tips.iter().next().unwrap().clone()));
        assert!(mining_state
            .dag
            .blocks
            .contains_key(&mining_state.dag.tips.iter().next().unwrap().clone()));
    }
''',
)
replace_test(
    accept,
    "mutated_block_returns_invalid_structure",
    '''    #[test]
    fn mutated_block_returns_invalid_structure() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut spend = txs[0].clone();
        spend.txid = "mutated".to_string();
        txs.push(spend);
        let difficulty = crate::expected_difficulty(&state);
        let mut block = build_candidate_block(parents, 1, difficulty, txs);
        let (header, mined, _, _) = crate::dev_mine_header(block.header.clone(), 200_000);
        assert!(mined, "expected malformed fixture to satisfy consensus PoW");
        block.header = header;
        block.hash = "mutated-block".to_string();
        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::Malformed);
    }
''',
)

validation = "crates/pulsedag-core/src/validation.rs"
regex_once(
    validation,
    r"build_candidate_block as raw_build_candidate_block,\s*build_coinbase_transaction,\s*refresh_block_consensus_ids,\s*refresh_block_consensus_ids_with_state,",
    """build_candidate_block as raw_build_candidate_block, build_coinbase_transaction,
            refresh_block_consensus_ids as raw_refresh_block_consensus_ids,
            refresh_block_consensus_ids_with_state as raw_refresh_block_consensus_ids_with_state,""",
)
replace_once(
    validation,
    '''    fn coinbase(nonce: u64) -> Transaction {
        build_coinbase_transaction("miner1", 50, nonce)
    }

''',
    '''    fn coinbase(nonce: u64) -> Transaction {
        build_coinbase_transaction("miner1", 50, nonce)
    }

    fn mine_current_header(block: &mut Block) {
        let (header, mined, _, _) = crate::dev_mine_header(block.header.clone(), 200_000);
        assert!(mined, "expected validation fixture to satisfy consensus PoW");
        block.header = header;
        raw_refresh_block_consensus_ids(block);
    }

    fn refresh_block_consensus_ids(block: &mut Block) {
        raw_refresh_block_consensus_ids(block);
        mine_current_header(block);
    }

    fn refresh_block_consensus_ids_with_state(
        block: &mut Block,
        state: &ChainState,
    ) -> Result<(), PulseError> {
        raw_refresh_block_consensus_ids_with_state(block, state)?;
        mine_current_header(block);
        Ok(())
    }

''',
)
vtext = Path(validation).read_text()
for old, new in [
    (
        "block.header.difficulty = 1;\n        refresh_block_consensus_ids(&mut block);",
        "block.header.difficulty = 1;\n        raw_refresh_block_consensus_ids(&mut block);",
    ),
    (
        "block.header.difficulty = stale_template_difficulty;\n        refresh_block_consensus_ids(&mut block);",
        "block.header.difficulty = stale_template_difficulty;\n        raw_refresh_block_consensus_ids(&mut block);",
    ),
    (
        "block.header.difficulty = 0x0100_0000;\n        refresh_block_consensus_ids(&mut block);",
        "block.header.difficulty = 0x0100_0000;\n        raw_refresh_block_consensus_ids(&mut block);",
    ),
]:
    if old not in vtext:
        raise SystemExit(f"validation invalid-difficulty marker missing: {old}")
    vtext = vtext.replace(old, new, 1)
Path(validation).write_text(vtext)

storage = "crates/pulsedag-storage/src/lib.rs"
regex_once(
    storage,
    r"refresh_block_consensus_ids,\s*refresh_block_consensus_ids_with_state,",
    """refresh_block_consensus_ids as raw_refresh_block_consensus_ids,
        refresh_block_consensus_ids_with_state as raw_refresh_block_consensus_ids_with_state,""",
)
sfile = Path(storage)
stext = sfile.read_text()
marker = '''    fn build_candidate_block(
        parents: Vec<Hash>,
        height: u64,
        difficulty: u32,
        transactions: Vec<pulsedag_core::types::Transaction>,
    ) -> Block {
        let difficulty = if difficulty == 1 {
            pulsedag_core::retarget::CONSENSUS_POW_LIMIT_BITS
        } else {
            difficulty
        };
        raw_build_candidate_block(parents, height, difficulty, transactions)
    }

'''
wrappers = marker + '''    fn mine_current_header(block: &mut Block) {
        let (header, mined, _, _) = dev_mine_header(block.header.clone(), 200_000);
        assert!(mined, "expected storage fixture to satisfy consensus PoW");
        block.header = header;
        raw_refresh_block_consensus_ids(block);
    }

    fn refresh_block_consensus_ids(block: &mut Block) {
        raw_refresh_block_consensus_ids(block);
        mine_current_header(block);
    }

    fn refresh_block_consensus_ids_with_state(
        block: &mut Block,
        state: &pulsedag_core::ChainState,
    ) -> Result<(), PulseError> {
        raw_refresh_block_consensus_ids_with_state(block, state)?;
        mine_current_header(block);
        Ok(())
    }

'''
if stext.count(marker) != 1:
    raise SystemExit("storage build wrapper marker not found exactly once")
sfile.write_text(stext.replace(marker, wrappers, 1))

retarget = "crates/pulsedag-core/src/retarget.rs"
replace_once(
    retarget,
    '''    let avg_block_interval_secs = if interval == 0 {
        policy.target_block_interval_secs
    } else {
        interval
    };
''',
    '''    let observed_intervals = observed_block_count.saturating_sub(1);
    let avg_block_interval_secs = if observed_intervals == 0 {
        policy.target_block_interval_secs
    } else {
        interval.max(1)
    };
''',
)
replace_once(
    retarget,
    "    let observed_intervals = observed_block_count.saturating_sub(1);\n    let raw_multiplier_bps = policy\n",
    "    let raw_multiplier_bps = policy\n",
)
replace_once(
    retarget,
    '''    #[test]
    fn legacy_difficulty_one_is_not_an_absorbing_state() {
''',
    '''    #[test]
    fn zero_second_intervals_harden_instead_of_falling_back_to_target() {
        let state = state_with_fixed_interval_tip(CONSENSUS_POW_LIMIT_BITS, 0, 25);
        let snapshot = consensus_difficulty_snapshot(&state);

        assert_eq!(snapshot.avg_block_interval_secs, 1);
        assert_eq!(snapshot.retarget_multiplier_bps, CONSENSUS_RETARGET_MAX_BPS);
        assert_eq!(snapshot.target_multiplier_bps, 8_000);
        assert!(snapshot.expected_bits != CONSENSUS_POW_LIMIT_BITS);
        assert!(target_from_bits(snapshot.expected_bits) < consensus_pow_limit_target());
    }

    #[test]
    fn legacy_difficulty_one_is_not_an_absorbing_state() {
''',
)

pow_policy = "crates/pulsedag-rpc/src/handlers/pow_policy.rs"
ptext = Path(pow_policy).read_text()
ptext = ptext.replace(
    "let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);",
    "let snapshot = pulsedag_core::consensus_difficulty_snapshot(&chain);",
    1,
)
ptext = ptext.replace("algorithm: snapshot.algorithm.to_string(),", "algorithm: pulsedag_core::selected_pow_name().to_string(),", 1)
ptext = ptext.replace("current_dev_difficulty: snapshot.current_difficulty,", "current_dev_difficulty: u64::from(snapshot.current_bits),", 1)
ptext = ptext.replace("recommended_dev_difficulty: snapshot.suggested_difficulty,", "recommended_dev_difficulty: u64::from(snapshot.expected_bits),", 1)
ptext = ptext.replace("suggested_difficulty: snapshot.suggested_difficulty,", "suggested_difficulty: u64::from(snapshot.expected_bits),", 1)
ptext = ptext.replace("target_u64: snapshot.target_u64,", "target_u64: snapshot.expected_target_u64,", 1)
ptext = ptext.replace(
    '"This is a development difficulty policy".to_string(),',
    '"This endpoint reports the canonical consensus difficulty policy".to_string(),',
    1,
)
Path(pow_policy).write_text(ptext)

pow_dashboard = "crates/pulsedag-rpc/src/handlers/pow_dashboard.rs"
dtext = Path(pow_dashboard).read_text()
dtext = dtext.replace(
    "let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);",
    "let snapshot = pulsedag_core::consensus_difficulty_snapshot(&chain);",
    1,
)
dtext = dtext.replace("let suggested_difficulty = snapshot.suggested_difficulty;", "let suggested_difficulty = u64::from(snapshot.expected_bits);", 1)
dtext = dtext.replace("let target_u64 = snapshot.target_u64;", "let target_u64 = snapshot.expected_target_u64;", 1)
dtext = dtext.replace(
    "let retarget_multiplier_bps =\n        pulsedag_core::dev_retarget_multiplier_bps(avg_block_interval_secs);",
    "let retarget_multiplier_bps = snapshot.retarget_multiplier_bps;",
    1,
)
dtext = dtext.replace("algorithm: snapshot.algorithm.to_string(),", "algorithm: pulsedag_core::selected_pow_name().to_string(),", 1)
Path(pow_dashboard).write_text(dtext)

pow_capture = "crates/pulsedag-rpc/src/handlers/pow_metrics_capture.rs"
ctext = Path(pow_capture).read_text()
ctext = ctext.replace(
    "let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);",
    "let snapshot = pulsedag_core::consensus_difficulty_snapshot(&chain);",
    1,
)
ctext = ctext.replace("let suggested_difficulty = snapshot.suggested_difficulty;", "let suggested_difficulty = u64::from(snapshot.expected_bits);", 1)
ctext = ctext.replace("let target_u64 = snapshot.target_u64;", "let target_u64 = snapshot.expected_target_u64;", 1)
ctext = ctext.replace("algorithm: snapshot.algorithm.to_string(),", "algorithm: pulsedag_core::selected_pow_name().to_string(),", 1)
Path(pow_capture).write_text(ctext)

pow_metrics = "crates/pulsedag-rpc/src/handlers/pow_metrics.rs"
mtext = Path(pow_metrics).read_text()
mtext = mtext.replace(
    "/// Canonical consensus diagnostics returned by the dedicated `/pow` endpoint.",
    "/// Canonical consensus diagnostics returned by the `/pow/metrics` endpoint.",
    1,
)
Path(pow_metrics).write_text(mtext)
