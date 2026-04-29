use crate::{selection::preferred_tip_hash, state::ChainState};

pub fn dag_consistency_issues(state: &ChainState) -> Vec<String> {
    let mut issues = Vec::new();

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

    if let Some(selected_tip) = preferred_tip_hash(state) {
        if !state.dag.tips.contains(&selected_tip) {
            issues.push(format!(
                "selected_tip {} is not present in tip set",
                selected_tip
            ));
        }
    } else if !state.dag.blocks.is_empty() {
        issues.push("no selected_tip could be derived while blocks exist".to_string());
    }

    for block in state.dag.blocks.values() {
        if block.header.height == 0 {
            continue;
        }
        if block.header.parents.is_empty() {
            issues.push(format!(
                "block {} at height {} has no parents",
                block.hash, block.header.height
            ));
        }
        for parent in &block.header.parents {
            if !state.dag.blocks.contains_key(parent) {
                issues.push(format!(
                    "block {} references missing parent {}",
                    block.hash, parent
                ));
            }
        }
    }

    issues.sort();
    issues.dedup();
    issues
}
