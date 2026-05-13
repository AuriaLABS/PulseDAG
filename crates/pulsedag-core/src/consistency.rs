use std::collections::BTreeSet;

use crate::{
    selection::{preferred_tip_hash, sorted_tip_hashes},
    state::ChainState,
};

/// Audit the local DAG invariants used by PulseDAG v2 replay and selection.
///
/// These invariants are intentionally local safety checks. They document the
/// deterministic DAG behavior required before v3 network replay work, but they
/// do not implement full GHOSTDAG and do not claim Kaspa consensus
/// compatibility.
///
/// Enforced invariants:
/// - accepted non-genesis blocks have at least one parent;
/// - parents are unique within a block header;
/// - every accepted parent is known, otherwise the block must remain an orphan;
/// - `height == max(parent.height) + 1` for accepted non-genesis blocks;
/// - every child index entry references known blocks and removes its parent from
///   the tip set;
/// - every tip references a known block, and the selected tip comes from the
///   deterministic sorted tip order;
/// - `best_height` equals the maximum accepted block height.
pub fn dag_consistency_issues(state: &ChainState) -> Vec<String> {
    let mut issues = Vec::new();

    if !state.dag.blocks.contains_key(&state.dag.genesis_hash) {
        issues.push(format!(
            "genesis {} is missing from block store",
            state.dag.genesis_hash
        ));
    }

    let max_height = state
        .dag
        .blocks
        .values()
        .map(|b| b.header.height)
        .max()
        .unwrap_or(0);
    if state.dag.best_height != max_height {
        issues.push(format!(
            "best_height {} does not match max block height {}",
            state.dag.best_height, max_height
        ));
    }

    for tip in &state.dag.tips {
        if !state.dag.blocks.contains_key(tip) {
            issues.push(format!("tip {} is missing from block store", tip));
        }
    }

    let sorted_tips = sorted_tip_hashes(state);
    if sorted_tips.len() != state.dag.tips.len() {
        issues.push(format!(
            "sorted tip count {} does not match tip set count {}",
            sorted_tips.len(),
            state.dag.tips.len()
        ));
    }
    if let Some(selected_tip) = preferred_tip_hash(state) {
        if !state.dag.tips.contains(&selected_tip) {
            issues.push(format!(
                "selected_tip {} is not present in tip set",
                selected_tip
            ));
        }
        if sorted_tips.first() != Some(&selected_tip) {
            issues.push(format!(
                "selected_tip {} does not match first sorted tip {:?}",
                selected_tip,
                sorted_tips.first()
            ));
        }
    } else if !state.dag.blocks.is_empty() {
        issues.push("no selected_tip could be derived while blocks exist".to_string());
    }

    for (hash, block) in &state.dag.blocks {
        if hash != &block.hash {
            issues.push(format!(
                "block map key {} does not match embedded hash {}",
                hash, block.hash
            ));
        }
        if hash == &state.dag.genesis_hash {
            continue;
        }
        if block.header.parents.is_empty() {
            issues.push(format!(
                "block {} at height {} has no parents",
                block.hash, block.header.height
            ));
        }

        let mut seen_parents = BTreeSet::new();
        let mut expected_height = 0u64;
        for parent in &block.header.parents {
            if !seen_parents.insert(parent) {
                issues.push(format!("block {} repeats parent {}", block.hash, parent));
            }
            match state.dag.blocks.get(parent) {
                Some(parent_block) => {
                    expected_height =
                        expected_height.max(parent_block.header.height.saturating_add(1));
                }
                None => {
                    issues.push(format!(
                        "block {} references missing parent {}",
                        block.hash, parent
                    ));
                }
            }
        }
        if expected_height > 0 && block.header.height != expected_height {
            issues.push(format!(
                "block {} height {} does not match max parent height + 1 ({})",
                block.hash, block.header.height, expected_height
            ));
        }
    }

    for (parent, children) in &state.dag.children {
        if !state.dag.blocks.contains_key(parent) {
            issues.push(format!("children map contains unknown parent {parent}"));
        }
        let mut seen_children = BTreeSet::new();
        for child in children {
            if !seen_children.insert(child) {
                issues.push(format!(
                    "children map repeats child {} for parent {}",
                    child, parent
                ));
            }
            match state.dag.blocks.get(child) {
                Some(child_block) => {
                    if !child_block.header.parents.iter().any(|p| p == parent) {
                        issues.push(format!(
                            "children map lists {} under non-parent {}",
                            child, parent
                        ));
                    }
                }
                None => {
                    issues.push(format!(
                        "children map references unknown child {} for parent {}",
                        child, parent
                    ));
                }
            }
        }
        if !children.is_empty() && state.dag.tips.contains(parent) {
            issues.push(format!("block {parent} has children but remains a tip"));
        }
    }

    issues.sort();
    issues.dedup();
    issues
}

/// Panic-on-failure helper for tests that need a concise DAG audit assertion.
pub fn assert_dag_consistent_for_tests(state: &ChainState) {
    let issues = dag_consistency_issues(state);
    assert!(issues.is_empty(), "DAG invariant violations: {issues:#?}");
}
