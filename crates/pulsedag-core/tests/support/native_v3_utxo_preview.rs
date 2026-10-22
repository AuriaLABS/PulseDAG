//! NON-NORMATIVE, TEST-ONLY and **in-memory** UTXO rollback/reapply simulator.
//!
//! This is NOT a production monetary engine. Reward amounts, fee-burn rules,
//! signatures, transaction IDs, maturity, native GHOSTDAG ordering and state
//! roots are NOT validated. Callers supply hypothetical already-validated
//! per-block effects. The selected-parent SPINE is not the full DAG tx order!
//! Nothing here performs database, network, wallet or miner writes.

use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::{
    native_ghostdag_policy_candidate_v3::DormantCandidateSelectedChainDelta,
    types::{BlockId, OutPoint, Utxo},
};

pub type PreviewSet = BTreeMap<OutPoint, Utxo>;

#[derive(Clone)]
pub struct HypotheticalBlockEffects {
    pub block: BlockId,
    pub spent: Vec<Utxo>,
    pub created: Vec<Utxo>,
    /// Explicit hypothetical reward; independent of *real* monetary policy.
    pub test_issuance: u64,
    /// Explicit hypothetical burn, which is NOT the live fee rule.
    pub test_fee_burn: u64,
}

fn same_coin(a: &Utxo, b: &Utxo) -> bool {
    a.outpoint == b.outpoint
        && a.address == b.address
        && a.amount == b.amount
        && a.coinbase == b.coinbase
        && a.height == b.height
}

fn check_effects(block: &HypotheticalBlockEffects) -> Result<(), String> {
    if block.block.is_empty() {
        return Err("empty preview block ID".into());
    }
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    let mut input_amount = 0u128;
    let mut output_amount = 0u128;
    let mut coinbase_amount = 0u128;
    for spent in &block.spent {
        if !inputs.insert(&spent.outpoint) || spent.amount == 0 {
            return Err("duplicate or zero-valued preview input".into());
        }
        input_amount = input_amount
            .checked_add(u128::from(spent.amount))
            .ok_or("preview input sum overflow")?;
    }
    for created in &block.created {
        if !outputs.insert(&created.outpoint)
            || inputs.contains(&created.outpoint)
            || created.amount == 0
        {
            return Err("duplicate/recycled or zero-valued preview output".into());
        }
        output_amount = output_amount
            .checked_add(u128::from(created.amount))
            .ok_or("preview output sum overflow")?;
        if created.coinbase {
            coinbase_amount = coinbase_amount
                .checked_add(u128::from(created.amount))
                .ok_or("preview coinbase sum overflow")?;
        }
    }
    if coinbase_amount != u128::from(block.test_issuance) {
        return Err("hypothetical coinbase amount != issuance".into());
    }
    if input_amount
        .checked_add(u128::from(block.test_issuance))
        .ok_or("hypothetical issuance sum overflow")?
        != output_amount
            .checked_add(u128::from(block.test_fee_burn))
            .ok_or("hypothetical fees sum overflow")?
    {
        return Err("hypothetical input+mint != output+burn".into());
    }
    Ok(())
}

pub fn apply(state: &PreviewSet, block: &HypotheticalBlockEffects) -> Result<PreviewSet, String> {
    check_effects(block)?;
    let mut next = state.clone();
    for spent in &block.spent {
        let current = next
            .get(&spent.outpoint)
            .ok_or("preview double-spend or absent input")?;
        if !same_coin(current, spent) {
            return Err("preview input does not match committed prior UTXO".into());
        }
        next.remove(&spent.outpoint);
    }
    for output in &block.created {
        if next.contains_key(&output.outpoint) {
            return Err("preview output collision".into());
        }
        next.insert(output.outpoint.clone(), output.clone());
    }
    Ok(next)
}

pub fn undo(state: &PreviewSet, block: &HypotheticalBlockEffects) -> Result<PreviewSet, String> {
    check_effects(block)?;
    let mut next = state.clone();
    for created in &block.created {
        let found = next
            .get(&created.outpoint)
            .ok_or("preview undo missing created UTXO")?;
        if !same_coin(found, created) {
            return Err("preview undo output mutated");
        }
        next.remove(&created.outpoint);
    }
    for spent in &block.spent {
        if next.insert(spent.outpoint.clone(), spent.clone()).is_some() {
            return Err("preview undo would overwrite an unspent coin".into());
        }
    }
    Ok(next)
}

/// On success returns a NEW snapshot, never mutating input. On any error the
/// input snapshot is unchanged. No DB commit, finality or accepted ordering.
pub fn apply_selected_spine_preview(
    old_snapshot: &PreviewSet,
    spine: &DormantCandidateSelectedChainDelta,
    by_block: &BTreeMap<BlockId, HypotheticalBlockEffects>,
) -> Result<PreviewSet, String> {
    if spine.common_selected_ancestor.is_empty() {
        return Err("selected-spine common ancestor missing".into());
    }
    let mut all = BTreeSet::new();
    for id in spine.detach_tip_first.iter().chain(&spine.attach_ancestor_first) {
        if !all.insert(id) || id == &spine.common_selected_ancestor {
            return Err("duplicate spine block or ancestor in delta".into());
        }
        let effects = by_block.get(id).ok_or("missing hypothetical block effects")?;
        if effects.block != *id {
            return Err("hypothetical effect ID mismatch".into());
        }
    }
    let mut next = old_snapshot.clone();
    for id in &spine.detach_tip_first {
        next = undo(&next, by_block.get(id).expect("previously checked"))?;
    }
    for id in &spine.attach_ancestor_first {
        next = apply(&next, by_block.get(id).expect("previously checked"))?;
    }
    Ok(next)
}

/// Checked total, no floating point, sum over *unspent* outputs only.
pub fn total_supply(state: &PreviewSet) -> Result<u128, String> {
    state.values().try_fold(0u128, |sum, coin| {
        sum.checked_add(u128::from(coin.amount))
            .ok_or_else(|| "preview supply total overflow".into())
    })
}

pub fn coin(name: &str, amount: u64, address: &str, coinbase: bool, height: u64) -> Utxo {
    Utxo {
        outpoint: OutPoint { txid: name.into(), index: 0 },
        amount,
        address: address.into(),
        coinbase,
        height,
    }
}
