//! Strictly bounded, DORMANT native monetary replay for a LINEAR prefix.
//!
//! Actual native envelopes, authenticated GD08 order, zero-funded genesis,
//! signed spends, maturity at end of score, block-bound reward outpoints and
//! every committed UTXO root. No legacy Block or OrderedDagV2. Deliberately
//! reject forks/conflicts until the native merge-set economic policy is frozen.
//! The cadence/K inputs are trial inputs, not authorization or finality proof.
//! No production caller, persistence writer, checkpoint or network activation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    block_envelope_v3::{validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3},
    errors::PulseError,
    genesis_v3::verify_dormant_native_genesis_freeze_v3_ns,
    monetary_v3::{
        economic_maturity_reached, monetary_cadence_fingerprint_v3, subsidy_atoms_for_score,
        total_supply_atoms_for_score, MonetaryCadenceSegment,
    },
    native_gd08_order_kernel_v3::derive_dormant_native_gd08_order_from_ghostdag_trial,
    native_gd08_signed_utxo_trial_v3::verify_full_signed_spend,
    reward_settlement_v3::{settlement_outpoint_v3, validate_reward_claim_transaction_v3},
    state::UtxoState,
    types::{BlockId, OutPoint, Utxo},
};

const MAX_TRIAL_BLOCKS: usize = 128;
const MAX_TRIAL_CADENCE_SEGMENTS: usize = 128;
const MAX_TRIAL_ORDINARY_ITEMS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeLinearMonetaryReportV3 {
    pub order_digest: String,
    /// Binds the entire validated trial schedule, including future segments.
    pub cadence_fingerprint: String,
    pub state_root: String,
    pub monetary_score: u64,
    pub total_subsidy_atoms: u64,
    pub pending_reward_atoms: u64,
    pub spendable_atoms: u64,
    pub mature_rewards: usize,
    pub applied_transfers: usize,
}

fn invalid(why: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED native linear monetary trial: {}",
        why.into()
    ))
}

fn insert_coin(state: &mut UtxoState, coin: Utxo) -> Result<(), PulseError> {
    if state.utxos.contains_key(&coin.outpoint) {
        return Err(invalid("duplicate settlement or transfer outpoint"));
    }
    state
        .address_index
        .entry(coin.address.clone())
        .or_default()
        .push(coin.outpoint.clone());
    state.utxos.insert(coin.outpoint.clone(), coin);
    Ok(())
}

fn apply_verified_transfer(
    state: &mut UtxoState,
    created: &mut BTreeSet<OutPoint>,
    tx: &crate::types::Transaction,
    chain_id: &str,
    height: u64,
) -> Result<(), PulseError> {
    let mut view = BTreeMap::new();
    for input in &tx.inputs {
        let coin = state
            .utxos
            .get(&input.previous_output)
            .ok_or_else(|| invalid("missing, spent or immature input"))?;
        view.insert(input.previous_output.clone(), coin.clone());
    }
    verify_full_signed_spend(tx, chain_id, &view)?;
    for input in &tx.inputs {
        let coin = state
            .utxos
            .remove(&input.previous_output)
            .ok_or_else(|| invalid("duplicate input"))?;
        if let Some(index) = state.address_index.get_mut(&coin.address) {
            index.retain(|outpoint| outpoint != &coin.outpoint);
        }
    }
    for (i, output) in tx.outputs.iter().enumerate() {
        let outpoint = OutPoint {
            txid: tx.txid.clone(),
            index: u32::try_from(i).map_err(|_| invalid("output index overflow"))?,
        };
        if !created.insert(outpoint.clone()) {
            return Err(invalid("repeated output history"));
        }
        insert_coin(
            state,
            Utxo {
                outpoint,
                address: output.address.clone(),
                amount: output.amount,
                coinbase: false,
                height,
            },
        )?;
    }
    Ok(())
}

/// Rebuild from the full pinned genesis; no caller-supplied balances.
/// Outputs are diagnostics only. Reject any unsupported nonlinear history,
/// missing/immature input, duplicate spend, bad signature or root mismatch.
/// Apply ordinary spends before end-of-score maturity; rewards mature only
/// under the explicit trial economic schedule, never wall time or header height.
pub fn inspect_dormant_native_linear_monetary_trial(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
) -> Result<DormantNativeLinearMonetaryReportV3, PulseError> {
    Ok(replay_linear_state(
        chain_id,
        genesis_ns,
        trusted_genesis,
        tip,
        envelopes,
        cadence,
    )?
    .report)
}

struct LinearReplayState {
    report: DormantNativeLinearMonetaryReportV3,
    state: UtxoState,
    created: BTreeSet<OutPoint>,
    pending: VecDeque<(u64, Utxo)>,
}

fn replay_linear_state(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
) -> Result<LinearReplayState, PulseError> {
    if envelopes.is_empty() || envelopes.len() > MAX_TRIAL_BLOCKS {
        return Err(invalid("unsupported bounded prefix length"));
    }
    if cadence.len() > MAX_TRIAL_CADENCE_SEGMENTS {
        return Err(invalid("trial cadence budget exceeded"));
    }
    // Validate even a genesis-only prefix; the replay loop may never execute.
    let cadence_fingerprint =
        monetary_cadence_fingerprint_v3(cadence).map_err(|e| invalid(e.to_string()))?;
    let genesis = envelopes
        .get(trusted_genesis)
        .ok_or_else(|| invalid("missing frozen genesis"))?;
    verify_dormant_native_genesis_freeze_v3_ns(genesis, chain_id, genesis_ns, trusted_genesis)?;
    let mut headers = BTreeMap::new();
    let mut item_count = 0usize;
    for (hash, envelope) in envelopes {
        if hash != &envelope.hash {
            return Err(invalid("incorrect envelope map key"));
        }
        validate_dormant_native_v3_monetary_body_preflight(envelope, chain_id)?;
        if hash != trusted_genesis && envelope.header.parents.len() != 1 {
            return Err(invalid("multiparent monetary policy not frozen"));
        }
        for tx in envelope.transactions.iter().skip(1) {
            item_count = item_count
                .checked_add(1)
                .and_then(|n| n.checked_add(tx.inputs.len()))
                .and_then(|n| n.checked_add(tx.outputs.len()))
                .ok_or_else(|| invalid("transaction budget overflow"))?;
            if item_count > MAX_TRIAL_ORDINARY_ITEMS {
                return Err(invalid("transaction budget exceeded"));
            }
        }
        headers.insert(hash.clone(), envelope.header.clone());
    }
    // K=0 here has no anticone effect: only one parent per reachable block,
    // and the ordered set must cover the entire supplied corpus.
    let order = derive_dormant_native_gd08_order_from_ghostdag_trial(
        chain_id,
        genesis_ns,
        trusted_genesis,
        tip,
        &headers,
        0,
    )?;
    if order.ordered.len() != envelopes.len() {
        return Err(invalid("unrelated or competing blocks in linear trial"));
    }
    let mut state = UtxoState::default();
    if state.compute_state_root()? != genesis.header.state_root {
        return Err(invalid("genesis must have zero issuance"));
    }
    let mut created = BTreeSet::new();
    let mut pending: VecDeque<(u64, Utxo)> = VecDeque::new();
    let mut mature_rewards = 0usize;
    let mut applied_transfers = 0usize;
    let mut issued = 0_u64;
    for (index, hash) in order.ordered.iter().enumerate().skip(1) {
        let score = u64::try_from(index).map_err(|_| invalid("score overflow"))?;
        let block = &envelopes[hash];
        if block.header.parents.as_slice() != [order.ordered[index - 1].clone()] {
            return Err(invalid("nonlinear canonical prefix"));
        }
        let claim = block
            .transactions
            .first()
            .ok_or_else(|| invalid("missing reward claim"))?;
        validate_reward_claim_transaction_v3(claim, chain_id)
            .map_err(|e| invalid(e.to_string()))?;
        let mut fees = 0_u64;
        for tx in block.transactions.iter().skip(1) {
            apply_verified_transfer(&mut state, &mut created, tx, chain_id, block.header.height)?;
            fees = fees
                .checked_add(tx.fee)
                .ok_or_else(|| invalid("fee overflow"))?;
            applied_transfers += 1;
        }
        let subsidy =
            subsidy_atoms_for_score(score, cadence).map_err(|e| invalid(e.to_string()))?;
        issued = issued
            .checked_add(subsidy)
            .ok_or_else(|| invalid("issuance overflow"))?;
        let amount = subsidy
            .checked_add(fees)
            .ok_or_else(|| invalid("reward overflow"))?;
        let outpoint = settlement_outpoint_v3(chain_id, hash, &claim.txid);
        if !created.insert(outpoint.clone()) {
            return Err(invalid("repeated reward outpoint"));
        }
        pending.push_back((
            score,
            Utxo {
                outpoint,
                address: claim.outputs[0].address.clone(),
                amount,
                coinbase: true,
                height: block.header.height,
            },
        ));
        while let Some((reward_score, _)) = pending.front() {
            if !economic_maturity_reached(*reward_score, score, cadence)
                .map_err(|e| invalid(e.to_string()))?
            {
                break;
            }
            let (_, coin) = pending.pop_front().expect("checked nonempty queue");
            if coin.amount != 0 {
                insert_coin(&mut state, coin)?;
            }
            mature_rewards += 1;
        }
        let spendable: u128 = state
            .utxos
            .values()
            .map(|coin| u128::from(coin.amount))
            .sum();
        let reserved: u128 = pending
            .iter()
            .map(|(_, coin)| u128::from(coin.amount))
            .sum();
        let scheduled =
            total_supply_atoms_for_score(score, cadence).map_err(|e| invalid(e.to_string()))?;
        if issued != scheduled || spendable + reserved != u128::from(scheduled) {
            return Err(invalid(
                "issued supply differs from UTXO plus pending rewards",
            ));
        }
        if state.compute_state_root()? != block.header.state_root {
            return Err(invalid("native committed state root mismatch"));
        }
    }
    let pending_reward_atoms = pending.iter().map(|(_, coin)| coin.amount).sum();
    let spendable_atoms = state.utxos.values().map(|coin| coin.amount).sum();
    let report = DormantNativeLinearMonetaryReportV3 {
        order_digest: order.digest,
        cadence_fingerprint,
        state_root: state.compute_state_root()?,
        monetary_score: (order.ordered.len() - 1) as u64,
        total_subsidy_atoms: issued,
        pending_reward_atoms,
        spendable_atoms,
        mature_rewards,
        applied_transfers,
    };
    Ok(LinearReplayState {
        report,
        state,
        created,
        pending,
    })
}

/// Offline commitment to the complete bounded replay state, not a snapshot
/// encoding, signature, approved policy, or permission to discard history.
/// In particular, equal spendable roots alone do not establish equivalence.
pub fn inspect_dormant_native_linear_checkpoint_commitment(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
) -> Result<String, PulseError> {
    let replay = replay_linear_state(
        chain_id,
        genesis_ns,
        trusted_genesis,
        tip,
        envelopes,
        cadence,
    )?;
    Ok(linear_checkpoint_commitment(
        chain_id,
        genesis_ns,
        trusted_genesis,
        tip,
        &replay,
    ))
}

fn linear_checkpoint_commitment(
    chain_id: &str,
    genesis_ns: u64,
    genesis: &str,
    tip: &str,
    replay: &LinearReplayState,
) -> String {
    use sha2::{Digest, Sha256};

    fn field(hash: &mut Sha256, value: &str) {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }

    fn outpoint(hash: &mut Sha256, value: &OutPoint) {
        field(hash, &value.txid);
        hash.update(value.index.to_le_bytes());
    }

    let mut hash = Sha256::new();
    hash.update(b"PulseDAG:UNAPPROVED:linear-checkpoint-monetary:v1");
    field(&mut hash, chain_id);
    hash.update(genesis_ns.to_le_bytes());
    field(&mut hash, genesis);
    field(&mut hash, tip);
    field(
        &mut hash,
        crate::monetary_v3::MONETARY_POLICY_FINGERPRINT_V3,
    );
    field(&mut hash, &replay.report.cadence_fingerprint);
    field(&mut hash, &replay.report.order_digest);
    field(&mut hash, &replay.report.state_root);
    for value in [
        replay.report.monetary_score,
        replay.report.total_subsidy_atoms,
        replay.report.spendable_atoms,
        replay.report.pending_reward_atoms,
        replay.report.mature_rewards as u64,
        replay.report.applied_transfers as u64,
    ] {
        hash.update(value.to_le_bytes());
    }
    hash.update((replay.created.len() as u64).to_le_bytes());
    for created in &replay.created {
        outpoint(&mut hash, created);
    }
    hash.update((replay.pending.len() as u64).to_le_bytes());
    for (score, coin) in &replay.pending {
        hash.update(score.to_le_bytes());
        outpoint(&mut hash, &coin.outpoint);
        field(&mut hash, &coin.address);
        hash.update(coin.amount.to_le_bytes());
        hash.update([u8::from(coin.coinbase)]);
        hash.update(coin.height.to_le_bytes());
    }
    hex::encode(hash.finalize())
}

/// Evidence for the individual direct parents of a single candidate merge.
/// Each report is branch-local, NOT an aggregate balance or merge settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeMergeParentReportV3 {
    pub candidate_hash: BlockId,
    pub order_digest: String,
    pub ordered_blocks: Vec<BlockId>,
    pub parent_reports: BTreeMap<BlockId, DormantNativeLinearMonetaryReportV3>,
    /// Distinct authenticated transaction IDs spending the same outpoint in
    /// separately verified parent histories. No winner or fee is authorized.
    pub competing_spends: BTreeMap<OutPoint, BTreeSet<String>>,
}

/// Authenticate a bounded merge candidate and independently rebuild every
/// direct parent's linear past from frozen genesis. Competing spends may be
/// valid on separate branches; their balances MUST NOT be added together.
/// The merge's own committed state root, rewards and conflict winners remain
/// unverified. Reject nested merges rather than invent an economic policy.
pub fn inspect_dormant_native_merge_parent_linear_trials(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    candidate: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
    trial_k: usize,
) -> Result<DormantNativeMergeParentReportV3, PulseError> {
    const MAX_MERGE_PARENTS: usize = 8;
    if envelopes.is_empty() || envelopes.len() > MAX_TRIAL_BLOCKS {
        return Err(invalid("unsupported bounded merge corpus length"));
    }
    let merge = envelopes
        .get(candidate)
        .ok_or_else(|| invalid("missing merge candidate"))?;
    if !(2..=MAX_MERGE_PARENTS).contains(&merge.header.parents.len()) {
        return Err(invalid("unsupported direct merge parent count"));
    }
    let mut headers = BTreeMap::new();
    let mut item_count = 0usize;
    for (hash, envelope) in envelopes {
        if hash != &envelope.hash {
            return Err(invalid("incorrect envelope map key"));
        }
        for tx in envelope.transactions.iter().skip(1) {
            item_count = item_count
                .checked_add(1)
                .and_then(|n| n.checked_add(tx.inputs.len()))
                .and_then(|n| n.checked_add(tx.outputs.len()))
                .ok_or_else(|| invalid("transaction budget overflow"))?;
            if item_count > MAX_TRIAL_ORDINARY_ITEMS {
                return Err(invalid("transaction budget exceeded"));
            }
        }
        validate_dormant_native_v3_monetary_body_preflight(envelope, chain_id)?;
        headers.insert(hash.clone(), envelope.header.clone());
    }
    let order = derive_dormant_native_gd08_order_from_ghostdag_trial(
        chain_id,
        genesis_ns,
        trusted_genesis,
        candidate,
        &headers,
        trial_k,
    )?;
    if order.ordered.len() != envelopes.len() {
        return Err(invalid("unrelated blocks in merge trial"));
    }
    let mut parent_reports = BTreeMap::new();
    let mut competing_spends: BTreeMap<OutPoint, BTreeSet<String>> = BTreeMap::new();
    let mut inspected_blocks = BTreeSet::new();
    for parent in &merge.header.parents {
        let mut prefix = BTreeMap::new();
        let mut next = parent.clone();
        loop {
            let envelope = envelopes
                .get(&next)
                .ok_or_else(|| invalid("missing parent branch envelope"))?;
            if prefix.insert(next.clone(), envelope.clone()).is_some() {
                return Err(invalid("cycle in parent branch"));
            }
            if next == trusted_genesis {
                break;
            }
            if envelope.header.parents.len() != 1 {
                return Err(invalid("nested merge monetary policy not frozen"));
            }
            next = envelope.header.parents[0].clone();
        }
        let report = inspect_dormant_native_linear_monetary_trial(
            chain_id,
            genesis_ns,
            trusted_genesis,
            parent,
            &prefix,
            cadence,
        )?;
        // Collect only after successful monetary replay; shared ancestors are
        // inspected once, and repeated identical txids are not distinct spends.
        for (hash, envelope) in &prefix {
            if inspected_blocks.insert(hash.clone()) {
                for tx in envelope.transactions.iter().skip(1) {
                    for input in &tx.inputs {
                        competing_spends
                            .entry(input.previous_output.clone())
                            .or_default()
                            .insert(tx.txid.clone());
                    }
                }
            }
        }
        if parent_reports.insert(parent.clone(), report).is_some() {
            return Err(invalid("duplicate merge parent"));
        }
    }
    competing_spends.retain(|_, txids| txids.len() > 1);
    Ok(DormantNativeMergeParentReportV3 {
        candidate_hash: candidate.clone(),
        order_digest: order.digest,
        ordered_blocks: order.ordered,
        parent_reports,
        competing_spends,
    })
}

/// Hypothetical all-position reward accounting; not a settled or spendable coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeSiblingRewardPositionV3 {
    pub block_hash: BlockId,
    pub score: u64,
    pub outpoint: OutPoint,
    pub beneficiary: String,
    pub subsidy_atoms: u64,
    pub accepted_fees_atoms: u64,
    pub claim_atoms: u64,
    /// Economic time only, without finality or a state transition.
    pub economic_maturity_at_candidate: bool,
}

/// Ordinary fee attribution and hypothetical economic-time reward projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeSiblingFeeTrialV3 {
    pub order_digest: String,
    pub accepted: Vec<(BlockId, String)>,
    pub skipped: Vec<(BlockId, String)>,
    pub fees_by_block: BTreeMap<BlockId, u64>,
    pub total_fees: u64,
    pub opening_state_root: String,
    pub ordinary_only_state_root: String,
    pub opening_spendable_atoms: u64,
    pub remaining_spendable_atoms: u64,
    pub unchanged_pending_reward_atoms: u64,
    /// Explicit trial in which every canonical position (including red blocks
    /// and the merge candidate) receives the scheduled subsidy. No eligibility
    /// policy, finality, reward spendability or merge root is authorized.
    pub hypothetical_reward_positions: Vec<DormantNativeSiblingRewardPositionV3>,
    pub hypothetical_total_issued_atoms: u64,
    /// Includes prior pending amounts and all new claims, before any maturity.
    pub hypothetical_reward_reserve_before_maturity_atoms: u64,
    /// Economic-time-only projection after all ordinary spends. No finality
    /// or actual merge-header commitment is checked by this experiment.
    pub hypothetical_post_maturity_state_root: String,
    pub hypothetical_post_maturity_spendable_atoms: u64,
    pub hypothetical_post_maturity_pending_atoms: u64,
    pub hypothetical_newly_matured_rewards: usize,
}

/// Restricted experiment: all direct parents are siblings of the same linear
/// prefix, each has at most one ordinary transfer, and the merge itself has
/// none. Independently verify every parent before applying first-in-GD08-order
/// conflict selection. All siblings (including reds) participate in THIS
/// trial; this is not an approved GD08 color, reward or finality policy.
/// A losing transaction consumes no inputs and earns no fees. Never use this
/// result as monetary admission or as a verified merge UTXO root.
pub fn inspect_dormant_native_sibling_fee_trial(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    candidate: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
    trial_k: usize,
) -> Result<DormantNativeSiblingFeeTrialV3, PulseError> {
    let verified = inspect_dormant_native_merge_parent_linear_trials(
        chain_id,
        genesis_ns,
        trusted_genesis,
        candidate,
        envelopes,
        cadence,
        trial_k,
    )?;
    let merge = &envelopes[candidate];
    if merge.transactions.len() != 1 {
        return Err(invalid("merge transfers outside sibling fee trial"));
    }
    let mut common_parent = None;
    for hash in &merge.header.parents {
        let sibling = &envelopes[hash];
        if sibling.header.parents.len() != 1 || sibling.transactions.len() > 2 {
            return Err(invalid("unsupported sibling fee trial shape"));
        }
        let parent = &sibling.header.parents[0];
        if common_parent.is_some_and(|previous| previous != parent) {
            return Err(invalid("siblings must share one verified parent"));
        }
        common_parent = Some(parent);
    }

    let common_parent = common_parent.ok_or_else(|| invalid("missing common parent"))?;
    let prefix = envelopes
        .iter()
        .filter(|(hash, _)| *hash != candidate && !verified.parent_reports.contains_key(*hash))
        .map(|(hash, body)| (hash.clone(), body.clone()))
        .collect();
    let mut opening = replay_linear_state(
        chain_id,
        genesis_ns,
        trusted_genesis,
        common_parent,
        &prefix,
        cadence,
    )?;

    // Each sibling's only ordinary transfer has already been verified against
    // the identical mature opening view of that common parent. No sibling
    // output, new reward or end-of-score maturity can fund another transfer
    // in this restricted experiment.
    let mut accepted = Vec::new();
    let mut skipped = Vec::new();
    let mut consumed = BTreeSet::new();
    let mut txids = BTreeSet::new();
    let mut fees_by_block = BTreeMap::new();
    let mut total_fees = 0_u64;
    for hash in &verified.ordered_blocks {
        if !verified.parent_reports.contains_key(hash) {
            continue;
        }
        let mut fee = 0;
        if let Some(tx) = envelopes[hash].transactions.get(1) {
            if txids.contains(&tx.txid)
                || tx
                    .inputs
                    .iter()
                    .any(|input| consumed.contains(&input.previous_output))
            {
                skipped.push((hash.clone(), tx.txid.clone()));
            } else {
                apply_verified_transfer(
                    &mut opening.state,
                    &mut opening.created,
                    tx,
                    chain_id,
                    envelopes[hash].header.height,
                )?;
                consumed.extend(tx.inputs.iter().map(|input| input.previous_output.clone()));
                txids.insert(tx.txid.clone());
                accepted.push((hash.clone(), tx.txid.clone()));
                fee = tx.fee;
                total_fees = total_fees
                    .checked_add(fee)
                    .ok_or_else(|| invalid("sibling trial fee overflow"))?;
            }
        }
        fees_by_block.insert(hash.clone(), fee);
    }
    let remaining_spendable_atoms = opening.state.utxos.values().try_fold(0_u64, |sum, coin| {
        sum.checked_add(coin.amount)
            .ok_or_else(|| invalid("ordinary trial balance overflow"))
    })?;
    if remaining_spendable_atoms.checked_add(total_fees) != Some(opening.report.spendable_atoms) {
        return Err(invalid("ordinary trial spendable value plus fees mismatch"));
    }
    // Plan hypothetical claims for all canonical positions beyond the common
    // prefix. This is deliberately the same all-sibling experiment as the
    // ordinary fee trial, not an approved blue/red economic policy.
    let candidate_score = u64::try_from(verified.ordered_blocks.len() - 1)
        .map_err(|_| invalid("candidate score overflow"))?;
    let mut hypothetical_reward_positions = Vec::new();
    let mut hypothetical_total_issued_atoms = opening.report.total_subsidy_atoms;
    let mut hypothetical_reward_reserve_before_maturity_atoms = opening.report.pending_reward_atoms;
    let mut previous_score = opening.report.monetary_score;
    for (index, hash) in verified.ordered_blocks.iter().enumerate() {
        if hash != candidate && !verified.parent_reports.contains_key(hash) {
            continue;
        }
        let score = u64::try_from(index).map_err(|_| invalid("reward score overflow"))?;
        if previous_score.checked_add(1) != Some(score) {
            return Err(invalid("noncontiguous sibling reward plan"));
        }
        previous_score = score;
        let accepted_fees_atoms = if hash == candidate {
            0
        } else {
            *fees_by_block
                .get(hash)
                .ok_or_else(|| invalid("missing sibling fee attribution"))?
        };
        let claim = &envelopes[hash].transactions[0];
        let outpoint = settlement_outpoint_v3(chain_id, hash, &claim.txid);
        if !opening.created.insert(outpoint.clone()) {
            return Err(invalid("repeated hypothetical reward outpoint"));
        }
        let subsidy_atoms =
            subsidy_atoms_for_score(score, cadence).map_err(|e| invalid(e.to_string()))?;
        let claim_atoms = subsidy_atoms
            .checked_add(accepted_fees_atoms)
            .ok_or_else(|| invalid("hypothetical reward overflow"))?;
        hypothetical_total_issued_atoms = hypothetical_total_issued_atoms
            .checked_add(subsidy_atoms)
            .ok_or_else(|| invalid("hypothetical issuance overflow"))?;
        hypothetical_reward_reserve_before_maturity_atoms =
            hypothetical_reward_reserve_before_maturity_atoms
                .checked_add(claim_atoms)
                .ok_or_else(|| invalid("hypothetical reward reserve overflow"))?;
        hypothetical_reward_positions.push(DormantNativeSiblingRewardPositionV3 {
            block_hash: hash.clone(),
            score,
            outpoint,
            beneficiary: claim.outputs[0].address.clone(),
            subsidy_atoms,
            accepted_fees_atoms,
            claim_atoms,
            economic_maturity_at_candidate: economic_maturity_reached(
                score,
                candidate_score,
                cadence,
            )
            .map_err(|e| invalid(e.to_string()))?,
        });
    }
    let scheduled = total_supply_atoms_for_score(candidate_score, cadence)
        .map_err(|e| invalid(e.to_string()))?;
    if previous_score != candidate_score
        || hypothetical_reward_positions.last().map(|p| &p.block_hash) != Some(candidate)
        || hypothetical_total_issued_atoms != scheduled
        || remaining_spendable_atoms.checked_add(hypothetical_reward_reserve_before_maturity_atoms)
            != Some(scheduled)
    {
        return Err(invalid("hypothetical reward plan does not conserve supply"));
    }
    let ordinary_only_state_root = opening.state.compute_state_root()?;
    // Materialize the hypothetical reward queue only AFTER ordinary spends.
    // Preserve the independently replayed prefix's pending claims rather
    // than deriving them from caller-supplied balances or branch-local sums.
    for position in &hypothetical_reward_positions {
        opening.pending.push_back((
            position.score,
            Utxo {
                outpoint: position.outpoint.clone(),
                address: position.beneficiary.clone(),
                amount: position.claim_atoms,
                coinbase: true,
                height: envelopes[&position.block_hash].header.height,
            },
        ));
    }
    let mut hypothetical_newly_matured_rewards = 0;
    while let Some((reward_score, _)) = opening.pending.front() {
        if !economic_maturity_reached(*reward_score, candidate_score, cadence)
            .map_err(|e| invalid(e.to_string()))?
        {
            break;
        }
        let (_, coin) = opening.pending.pop_front().expect("checked nonempty queue");
        if coin.amount != 0 {
            insert_coin(&mut opening.state, coin)?;
        }
        hypothetical_newly_matured_rewards += 1;
    }
    let hypothetical_post_maturity_spendable_atoms =
        opening.state.utxos.values().try_fold(0_u64, |sum, coin| {
            sum.checked_add(coin.amount)
                .ok_or_else(|| invalid("hypothetical mature balance overflow"))
        })?;
    let hypothetical_post_maturity_pending_atoms =
        opening.pending.iter().try_fold(0_u64, |sum, (_, coin)| {
            sum.checked_add(coin.amount)
                .ok_or_else(|| invalid("hypothetical pending balance overflow"))
        })?;
    if hypothetical_post_maturity_spendable_atoms
        .checked_add(hypothetical_post_maturity_pending_atoms)
        != Some(scheduled)
    {
        return Err(invalid("hypothetical post-maturity supply mismatch"));
    }
    Ok(DormantNativeSiblingFeeTrialV3 {
        order_digest: verified.order_digest,
        accepted,
        skipped,
        fees_by_block,
        total_fees,
        opening_state_root: opening.report.state_root,
        ordinary_only_state_root,
        opening_spendable_atoms: opening.report.spendable_atoms,
        remaining_spendable_atoms,
        unchanged_pending_reward_atoms: opening.report.pending_reward_atoms,
        hypothetical_reward_positions,
        hypothetical_total_issued_atoms,
        hypothetical_reward_reserve_before_maturity_atoms,
        hypothetical_post_maturity_state_root: opening.state.compute_state_root()?,
        hypothetical_post_maturity_spendable_atoms,
        hypothetical_post_maturity_pending_atoms,
        hypothetical_newly_matured_rewards,
    })
}

/// Verify the merge header's state commitment under the restricted,
/// all-position sibling trial. This does not approve the color, reward or
/// finality policy, and must not be used for live monetary admission.
pub fn verify_dormant_native_sibling_merge_root(
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis: &str,
    candidate: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    cadence: &[MonetaryCadenceSegment],
    trial_k: usize,
) -> Result<DormantNativeSiblingFeeTrialV3, PulseError> {
    let report = inspect_dormant_native_sibling_fee_trial(
        chain_id,
        genesis_ns,
        trusted_genesis,
        candidate,
        envelopes,
        cadence,
        trial_k,
    )?;
    let merge = envelopes
        .get(candidate)
        .ok_or_else(|| invalid("missing sibling merge candidate"))?;
    if merge.header.state_root != report.hypothetical_post_maturity_state_root {
        return Err(invalid("sibling trial merge state root mismatch"));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        tx::{
            address_from_public_key, compute_txid_v2, signing_message_v2, TRANSACTION_VERSION_V2,
        },
        types::{compute_merkle_root, Transaction, TxInput, TxOutput},
    };
    use ed25519_dalek::{Signer, SigningKey};

    const CHAIN: &str = "native-linear-monetary-test-only";
    const NS: u64 = 1_800_000_000_500_000_000;
    const CADENCE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 3_600_000_000_000,
    }];

    fn mine(
        parent: &str,
        height: u64,
        beneficiary: &str,
        root: String,
        spends: Vec<Transaction>,
    ) -> BlockEnvelopeV3 {
        mine_with_parents(vec![parent.to_owned()], height, beneficiary, root, spends)
    }

    fn mine_with_parents(
        mut parents: Vec<BlockId>,
        height: u64,
        beneficiary: &str,
        root: String,
        spends: Vec<Transaction>,
    ) -> BlockEnvelopeV3 {
        parents.sort();
        let mut transactions =
            vec![build_reward_claim_transaction_v3(beneficiary, height, CHAIN).unwrap()];
        transactions.extend(spends);
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns: NS + height * 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root: root,
            blue_score: height,
            height,
        };
        header.nonce = (0..4096)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
            })
            .unwrap();
        build_block_envelope_v3(CHAIN, header, transactions).unwrap()
    }

    fn signed_spend(key: &SigningKey, reward: &OutPoint, amount: u64) -> Transaction {
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: reward.clone(),
                public_key: hex::encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".into(),
                amount: amount - 1,
            }],
            fee: 1,
            nonce: 3,
        };
        tx.inputs[0].signature = hex::encode(
            key.sign(&signing_message_v2(&tx, CHAIN).unwrap())
                .to_bytes(),
        );
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        tx
    }

    fn reward(block: &BlockEnvelopeV3, amount: u64) -> Utxo {
        let claim = &block.transactions[0];
        Utxo {
            outpoint: settlement_outpoint_v3(CHAIN, &block.hash, &claim.txid),
            address: claim.outputs[0].address.clone(),
            amount,
            coinbase: true,
            height: block.header.height,
        }
    }

    #[test]
    fn genesis_only_rejects_invalid_cadence_and_binds_valid_schedule() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let corpus = BTreeMap::from([(g.clone(), genesis)]);
        let inspect = |cadence: &[MonetaryCadenceSegment]| {
            inspect_dormant_native_linear_monetary_trial(CHAIN, NS, &g, &g, &corpus, cadence)
        };

        let first = inspect(&CADENCE).unwrap();
        assert_eq!(first.monetary_score, 0);
        assert_eq!(first.total_subsidy_atoms, 0);
        assert_eq!(first.pending_reward_atoms, 0);
        assert_eq!(first.spendable_atoms, 0);
        assert_eq!(
            first.cadence_fingerprint,
            monetary_cadence_fingerprint_v3(&CADENCE).unwrap()
        );
        let mut changed = CADENCE;
        changed[0].target_interval_ns += 1;
        let second = inspect(&changed).unwrap();
        assert_eq!(first.state_root, second.state_root);
        assert_eq!(first.order_digest, second.order_digest);
        assert_ne!(first.cadence_fingerprint, second.cadence_fingerprint);

        let mut invalid_start = CADENCE;
        invalid_start[0].activation_score = 1;
        let mut invalid_interval = CADENCE;
        invalid_interval[0].target_interval_ns = 0;
        assert!(inspect(&[]).is_err());
        assert!(inspect(&invalid_start).is_err());
        assert!(inspect(&invalid_interval).is_err());
        assert!(inspect(&[CADENCE[0], CADENCE[0]]).is_err());
        let oversized: Vec<_> = (0..=MAX_TRIAL_CADENCE_SEGMENTS)
            .map(|score| MonetaryCadenceSegment {
                activation_score: score as u64,
                target_interval_ns: CADENCE[0].target_interval_ns,
            })
            .collect();
        assert!(inspect(&oversized).is_err());
    }

    #[test]
    fn native_reward_matures_then_signed_spend_and_fee_preserve_supply() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let key = SigningKey::from_bytes(&[19u8; 32]);
        let owner = address_from_public_key(&hex::encode(key.verifying_key().to_bytes()));
        let mut expected = UtxoState::default();
        let empty = expected.compute_state_root().unwrap();
        let b1 = mine(&g, 1, &owner, empty.clone(), vec![]);
        let subsidy1 = subsidy_atoms_for_score(1, &CADENCE).unwrap();
        let r1 = reward(&b1, subsidy1);
        insert_coin(&mut expected, r1.clone()).unwrap();
        let b2 = mine(
            &b1.hash,
            2,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![],
        );
        let spend = signed_spend(&key, &r1.outpoint, subsidy1);
        // Block 2 is the maturity boundary, so the same signed spend there
        // must fail even if its PoW and transaction commitment are valid.
        let premature = mine(&b1.hash, 2, &owner, empty, vec![spend.clone()]);
        let bad = BTreeMap::from([
            (g.clone(), genesis.clone()),
            (b1.hash.clone(), b1.clone()),
            (premature.hash.clone(), premature.clone()),
        ]);
        assert!(inspect_dormant_native_linear_monetary_trial(
            CHAIN,
            NS,
            &g,
            &premature.hash,
            &bad,
            &CADENCE,
        )
        .is_err());

        expected = UtxoState::default();
        insert_coin(
            &mut expected,
            Utxo {
                outpoint: OutPoint {
                    txid: spend.txid.clone(),
                    index: 0,
                },
                address: spend.outputs[0].address.clone(),
                amount: spend.outputs[0].amount,
                coinbase: false,
                height: 3,
            },
        )
        .unwrap();
        insert_coin(
            &mut expected,
            reward(&b2, subsidy_atoms_for_score(2, &CADENCE).unwrap()),
        )
        .unwrap();
        let b3 = mine(
            &b2.hash,
            3,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![spend.clone()],
        );
        insert_coin(
            &mut expected,
            reward(&b3, subsidy_atoms_for_score(3, &CADENCE).unwrap() + 1),
        )
        .unwrap();
        let b4 = mine(
            &b3.hash,
            4,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![],
        );
        let corpus = BTreeMap::from([
            (g.clone(), genesis),
            (b1.hash.clone(), b1),
            (b2.hash.clone(), b2),
            (b3.hash.clone(), b3.clone()),
            (b4.hash.clone(), b4.clone()),
        ]);
        let observed = inspect_dormant_native_linear_monetary_trial(
            CHAIN, NS, &g, &b4.hash, &corpus, &CADENCE,
        )
        .unwrap();
        assert_eq!(observed.applied_transfers, 1);
        assert_eq!(observed.mature_rewards, 3);
        assert_eq!(observed.state_root, expected.compute_state_root().unwrap());
        assert_eq!(
            observed.pending_reward_atoms,
            subsidy_atoms_for_score(4, &CADENCE).unwrap()
        );
        assert_eq!(
            observed.total_subsidy_atoms,
            observed.spendable_atoms + observed.pending_reward_atoms
        );

        let committed = inspect_dormant_native_linear_checkpoint_commitment(
            CHAIN, NS, &g, &b4.hash, &corpus, &CADENCE,
        )
        .unwrap();
        let reversed = corpus
            .iter()
            .rev()
            .map(|(hash, body)| (hash.clone(), body.clone()))
            .collect();
        assert_eq!(
            inspect_dormant_native_linear_checkpoint_commitment(
                CHAIN, NS, &g, &b4.hash, &reversed, &CADENCE,
            )
            .unwrap(),
            committed
        );
        let mut full = replay_linear_state(CHAIN, NS, &g, &b4.hash, &corpus, &CADENCE).unwrap();
        assert_eq!(
            linear_checkpoint_commitment(CHAIN, NS, &g, &b4.hash, &full),
            committed
        );
        // Changing only the beneficiary preserves both the spendable root
        // and total pending amount but must change the checkpoint commitment.
        full.pending.front_mut().unwrap().1.address = "pulse1different".into();
        assert_eq!(full.state.compute_state_root().unwrap(), observed.state_root);
        assert_ne!(
            linear_checkpoint_commitment(CHAIN, NS, &g, &b4.hash, &full),
            committed
        );
        let mut full = replay_linear_state(CHAIN, NS, &g, &b4.hash, &corpus, &CADENCE).unwrap();
        // The already spent reward is absent from the spendable state but
        // must remain in the creation history to prevent outpoint reuse.
        assert!(full.created.remove(&r1.outpoint));
        assert_eq!(full.state.compute_state_root().unwrap(), observed.state_root);
        assert_ne!(
            linear_checkpoint_commitment(CHAIN, NS, &g, &b4.hash, &full),
            committed
        );

        let replay = mine(
            &b3.hash,
            4,
            &owner,
            b4.header.state_root.clone(),
            vec![spend],
        );
        let mut bad = corpus.clone();
        bad.remove(&b4.hash);
        bad.insert(replay.hash.clone(), replay.clone());
        assert!(inspect_dormant_native_linear_monetary_trial(
            CHAIN,
            NS,
            &g,
            &replay.hash,
            &bad,
            &CADENCE,
        )
        .is_err());
        let forged = mine(&b3.hash, 4, &owner, "f".repeat(64), vec![]);
        bad.remove(&replay.hash);
        bad.insert(forged.hash.clone(), forged.clone());
        assert!(inspect_dormant_native_linear_monetary_trial(
            CHAIN,
            NS,
            &g,
            &forged.hash,
            &bad,
            &CADENCE,
        )
        .is_err());
    }

    #[test]
    fn disjoint_sibling_spends_preserve_both_outputs_and_exact_fees() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let key = SigningKey::from_bytes(&[23u8; 32]);
        let owner = address_from_public_key(&hex::encode(key.verifying_key().to_bytes()));
        let mut opening = UtxoState::default();
        let b1 = mine(&g, 1, &owner, opening.compute_state_root().unwrap(), vec![]);
        let r1 = reward(&b1, subsidy_atoms_for_score(1, &CADENCE).unwrap());
        insert_coin(&mut opening, r1.clone()).unwrap();
        let b2 = mine(
            &b1.hash,
            2,
            &owner,
            opening.compute_state_root().unwrap(),
            vec![],
        );
        let r2 = reward(&b2, subsidy_atoms_for_score(2, &CADENCE).unwrap());
        insert_coin(&mut opening, r2.clone()).unwrap();
        let b3 = mine(
            &b2.hash,
            3,
            &owner,
            opening.compute_state_root().unwrap(),
            vec![],
        );
        let r3 = reward(&b3, subsidy_atoms_for_score(3, &CADENCE).unwrap());
        let a_tx = signed_spend(&key, &r1.outpoint, r1.amount);
        let mut b_tx = signed_spend(&key, &r2.outpoint, r2.amount);
        b_tx.fee = 3;
        b_tx.outputs[0].amount = r2.amount - 3;
        b_tx.outputs[0].address = "pulse1otherrecipient".into();
        b_tx.inputs[0].signature = hex::encode(
            key.sign(&signing_message_v2(&b_tx, CHAIN).unwrap())
                .to_bytes(),
        );
        b_tx.txid = compute_txid_v2(&b_tx, CHAIN).unwrap();

        let output = |tx: &Transaction| Utxo {
            outpoint: OutPoint {
                txid: tx.txid.clone(),
                index: 0,
            },
            address: tx.outputs[0].address.clone(),
            amount: tx.outputs[0].amount,
            coinbase: false,
            height: 4,
        };
        // Construct each branch root independently of the replay function.
        let branch_root = |tx: &Transaction, unspent: &Utxo| {
            let mut state = UtxoState::default();
            insert_coin(&mut state, unspent.clone()).unwrap();
            insert_coin(&mut state, r3.clone()).unwrap();
            insert_coin(&mut state, output(tx)).unwrap();
            state.compute_state_root().unwrap()
        };
        let a = mine(
            &b3.hash,
            4,
            &owner,
            branch_root(&a_tx, &r2),
            vec![a_tx.clone()],
        );
        let b = mine(
            &b3.hash,
            4,
            &owner,
            branch_root(&b_tx, &r1),
            vec![b_tx.clone()],
        );
        let join = mine_with_parents(
            vec![a.hash.clone(), b.hash.clone()],
            5,
            &owner,
            "ab".repeat(32),
            vec![],
        );
        let corpus = BTreeMap::from([
            (g.clone(), genesis),
            (b1.hash.clone(), b1),
            (b2.hash.clone(), b2),
            (b3.hash.clone(), b3),
            (a.hash.clone(), a.clone()),
            (b.hash.clone(), b.clone()),
            (join.hash.clone(), join.clone()),
        ]);
        let report = inspect_dormant_native_sibling_fee_trial(
            CHAIN, NS, &g, &join.hash, &corpus, &CADENCE, 0,
        )
        .unwrap();
        assert_eq!(report.accepted.len(), 2);
        assert!(report.skipped.is_empty());
        assert_eq!(
            report.fees_by_block,
            BTreeMap::from([(a.hash, 1), (b.hash, 3)])
        );
        assert_eq!(report.total_fees, 4);
        assert_eq!(report.opening_spendable_atoms, r1.amount + r2.amount);
        assert_eq!(report.remaining_spendable_atoms, r1.amount + r2.amount - 4);
        assert_eq!(report.unchanged_pending_reward_atoms, r3.amount);
        assert_eq!(
            report.opening_state_root,
            opening.compute_state_root().unwrap()
        );
        let mut expected = UtxoState::default();
        insert_coin(&mut expected, output(&a_tx)).unwrap();
        insert_coin(&mut expected, output(&b_tx)).unwrap();
        assert_eq!(
            report.ordinary_only_state_root,
            expected.compute_state_root().unwrap()
        );
    }

    #[test]
    fn competing_parent_spends_are_branch_local_and_bad_parent_roots_fail() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let key = SigningKey::from_bytes(&[19u8; 32]);
        let owner = address_from_public_key(&hex::encode(key.verifying_key().to_bytes()));
        let mut expected = UtxoState::default();
        let b1 = mine(
            &g,
            1,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![],
        );
        let subsidy = subsidy_atoms_for_score(1, &CADENCE).unwrap();
        let r1 = reward(&b1, subsidy);
        insert_coin(&mut expected, r1.clone()).unwrap();
        let b2 = mine(
            &b1.hash,
            2,
            &owner,
            expected.compute_state_root().unwrap(),
            vec![],
        );
        let spend_a = signed_spend(&key, &r1.outpoint, subsidy);
        let mut spend_b = spend_a.clone();
        spend_b.nonce += 1;
        spend_b.outputs[0].address = "pulse1otherrecipient".into();
        spend_b.inputs[0].signature = hex::encode(
            key.sign(&signing_message_v2(&spend_b, CHAIN).unwrap())
                .to_bytes(),
        );
        spend_b.txid = compute_txid_v2(&spend_b, CHAIN).unwrap();
        let branch_root = |tx: &Transaction| {
            let mut state = UtxoState::default();
            insert_coin(
                &mut state,
                Utxo {
                    outpoint: OutPoint {
                        txid: tx.txid.clone(),
                        index: 0,
                    },
                    address: tx.outputs[0].address.clone(),
                    amount: tx.outputs[0].amount,
                    coinbase: false,
                    height: 3,
                },
            )
            .unwrap();
            insert_coin(
                &mut state,
                reward(&b2, subsidy_atoms_for_score(2, &CADENCE).unwrap()),
            )
            .unwrap();
            state.compute_state_root().unwrap()
        };
        let a = mine(
            &b2.hash,
            3,
            &owner,
            branch_root(&spend_a),
            vec![spend_a.clone()],
        );
        let b = mine(
            &b2.hash,
            3,
            &owner,
            branch_root(&spend_b),
            vec![spend_b.clone()],
        );
        // Deliberately arbitrary merge root: this API verifies parents only.
        let join = mine_with_parents(
            vec![a.hash.clone(), b.hash.clone()],
            4,
            &owner,
            "ab".repeat(32),
            vec![],
        );
        let corpus = BTreeMap::from([
            (g.clone(), genesis),
            (b1.hash.clone(), b1),
            (b2.hash.clone(), b2.clone()),
            (a.hash.clone(), a.clone()),
            (b.hash.clone(), b.clone()),
            (join.hash.clone(), join.clone()),
        ]);
        let report = inspect_dormant_native_merge_parent_linear_trials(
            CHAIN, NS, &g, &join.hash, &corpus, &CADENCE, 0,
        )
        .unwrap();
        assert_eq!(report.candidate_hash, join.hash);
        assert_eq!(report.parent_reports.len(), 2);
        assert_eq!(
            report.competing_spends,
            BTreeMap::from([(
                r1.outpoint.clone(),
                BTreeSet::from([spend_a.txid.clone(), spend_b.txid.clone()]),
            )])
        );
        for parent in [&a, &b] {
            let branch = &report.parent_reports[&parent.hash];
            assert_eq!(branch.applied_transfers, 1);
            assert_eq!(branch.state_root, parent.header.state_root);
            assert_eq!(
                branch.total_subsidy_atoms,
                branch.spendable_atoms + branch.pending_reward_atoms
            );
        }
        let fees = inspect_dormant_native_sibling_fee_trial(
            CHAIN, NS, &g, &join.hash, &corpus, &CADENCE, 0,
        )
        .unwrap();
        let first = report
            .ordered_blocks
            .iter()
            .find(|hash| report.parent_reports.contains_key(*hash))
            .unwrap();
        assert_eq!(fees.accepted.len(), 1);
        assert_eq!(&fees.accepted[0].0, first);
        assert_eq!(fees.accepted[0].1, corpus[first].transactions[1].txid);
        assert_eq!(fees.skipped.len(), 1);
        assert_eq!(fees.total_fees, 1);
        assert_eq!(fees.opening_state_root, b2.header.state_root);
        assert_eq!(fees.opening_spendable_atoms, subsidy);
        assert_eq!(fees.remaining_spendable_atoms, subsidy - 1);
        assert_eq!(
            fees.unchanged_pending_reward_atoms,
            subsidy_atoms_for_score(2, &CADENCE).unwrap()
        );
        let winner = &corpus[first].transactions[1];
        let mut ordinary = UtxoState::default();
        insert_coin(
            &mut ordinary,
            Utxo {
                outpoint: OutPoint {
                    txid: winner.txid.clone(),
                    index: 0,
                },
                address: winner.outputs[0].address.clone(),
                amount: subsidy - 1,
                coinbase: false,
                height: 3,
            },
        )
        .unwrap();
        assert_eq!(
            fees.ordinary_only_state_root,
            ordinary.compute_state_root().unwrap()
        );
        assert_eq!(fees.fees_by_block[first], 1);
        assert_eq!(fees.fees_by_block[&fees.skipped[0].0], 0);
        assert_eq!(fees.order_digest, report.order_digest);
        let positions = &fees.hypothetical_reward_positions;
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0].block_hash, *first);
        assert_eq!(positions[0].accepted_fees_atoms, 1);
        assert_eq!(positions[1].block_hash, fees.skipped[0].0);
        assert_eq!(positions[1].accepted_fees_atoms, 0);
        assert_eq!(positions[2].block_hash, join.hash);
        assert_eq!(positions[2].accepted_fees_atoms, 0);
        for (position, score) in positions.iter().zip(3..=5) {
            let claim = &corpus[&position.block_hash].transactions[0];
            assert_eq!(position.score, score);
            assert_eq!(position.beneficiary, claim.outputs[0].address);
            assert_eq!(
                position.outpoint,
                settlement_outpoint_v3(CHAIN, &position.block_hash, &claim.txid)
            );
            assert_eq!(
                position.subsidy_atoms,
                subsidy_atoms_for_score(score, &CADENCE).unwrap()
            );
            assert_eq!(
                position.claim_atoms,
                position.subsidy_atoms + position.accepted_fees_atoms
            );
            assert_eq!(position.economic_maturity_at_candidate, score < 5);
        }
        assert_eq!(
            fees.hypothetical_total_issued_atoms,
            total_supply_atoms_for_score(5, &CADENCE).unwrap()
        );
        assert_eq!(
            fees.remaining_spendable_atoms + fees.hypothetical_reward_reserve_before_maturity_atoms,
            fees.hypothetical_total_issued_atoms
        );
        // The common prefix has one pending reward, then both siblings
        // mature at the merge's trial score. The merge's own claim remains
        // pending. Construct expected mature coins independently.
        let mut projected = ordinary;
        insert_coin(
            &mut projected,
            reward(&b2, subsidy_atoms_for_score(2, &CADENCE).unwrap()),
        )
        .unwrap();
        for (position, score) in positions.iter().take(2).zip(3..=4) {
            let amount = subsidy_atoms_for_score(score, &CADENCE).unwrap()
                + u64::from(&position.block_hash == first);
            insert_coin(
                &mut projected,
                reward(&corpus[&position.block_hash], amount),
            )
            .unwrap();
        }
        assert_eq!(fees.hypothetical_newly_matured_rewards, 3);
        assert_eq!(
            fees.hypothetical_post_maturity_state_root,
            projected.compute_state_root().unwrap()
        );
        // A PoW-valid merge with an arbitrary root must not pass the strict
        // trial. Re-mine the candidate with the independently derived root.
        assert!(verify_dormant_native_sibling_merge_root(
            CHAIN, NS, &g, &join.hash, &corpus, &CADENCE, 0,
        )
        .is_err());
        let bound_join = mine_with_parents(
            vec![a.hash.clone(), b.hash.clone()],
            4,
            &owner,
            projected.compute_state_root().unwrap(),
            vec![],
        );
        let mut bound_corpus = corpus.clone();
        bound_corpus.remove(&join.hash);
        bound_corpus.insert(bound_join.hash.clone(), bound_join.clone());
        let bound = verify_dormant_native_sibling_merge_root(
            CHAIN,
            NS,
            &g,
            &bound_join.hash,
            &bound_corpus,
            &CADENCE,
            0,
        )
        .unwrap();
        assert_eq!(
            bound.hypothetical_post_maturity_state_root,
            bound_join.header.state_root
        );
        assert_eq!(bound.accepted, fees.accepted);
        assert_eq!(bound.skipped, fees.skipped);
        assert_eq!(bound.total_fees, fees.total_fees);
        assert_eq!(
            bound.hypothetical_total_issued_atoms,
            fees.hypothetical_total_issued_atoms
        );
        assert_eq!(
            fees.hypothetical_post_maturity_pending_atoms,
            subsidy_atoms_for_score(5, &CADENCE).unwrap()
        );
        assert_eq!(
            fees.hypothetical_post_maturity_spendable_atoms,
            total_supply_atoms_for_score(4, &CADENCE).unwrap()
        );
        assert_ne!(
            report.parent_reports[&a.hash].state_root,
            report.parent_reports[&b.hash].state_root
        );
        // The same signed transaction included in two sibling blocks is a
        // duplicate inclusion, not two distinct competing spends.
        let duplicate = mine(
            &b2.hash,
            3,
            "pulse1otherminer",
            branch_root(&spend_a),
            vec![spend_a],
        );
        let duplicate_join = mine_with_parents(
            vec![a.hash.clone(), duplicate.hash.clone()],
            4,
            &owner,
            "ab".repeat(32),
            vec![],
        );
        let mut repeated = corpus.clone();
        repeated.remove(&b.hash);
        repeated.remove(&join.hash);
        repeated.insert(duplicate.hash.clone(), duplicate);
        repeated.insert(duplicate_join.hash.clone(), duplicate_join.clone());
        let repeated_report = inspect_dormant_native_merge_parent_linear_trials(
            CHAIN,
            NS,
            &g,
            &duplicate_join.hash,
            &repeated,
            &CADENCE,
            0,
        )
        .unwrap();
        assert!(repeated_report.competing_spends.is_empty());
        let duplicate_fees = inspect_dormant_native_sibling_fee_trial(
            CHAIN,
            NS,
            &g,
            &duplicate_join.hash,
            &repeated,
            &CADENCE,
            0,
        )
        .unwrap();
        assert_eq!(duplicate_fees.accepted.len(), 1);
        assert_eq!(duplicate_fees.skipped.len(), 1);
        assert_eq!(duplicate_fees.accepted[0].1, duplicate_fees.skipped[0].1);
        assert_eq!(duplicate_fees.total_fees, 1);
        assert_eq!(
            duplicate_fees
                .hypothetical_reward_positions
                .iter()
                .map(|position| position.accepted_fees_atoms)
                .sum::<u64>(),
            1
        );
        let forged = mine(&b2.hash, 3, &owner, "ff".repeat(32), vec![spend_b]);
        let bad_join = mine_with_parents(
            vec![a.hash.clone(), forged.hash.clone()],
            4,
            &owner,
            "ab".repeat(32),
            vec![],
        );
        let mut bad = corpus;
        bad.remove(&b.hash);
        bad.remove(&join.hash);
        bad.insert(forged.hash.clone(), forged);
        bad.insert(bad_join.hash.clone(), bad_join.clone());
        assert!(inspect_dormant_native_merge_parent_linear_trials(
            CHAIN,
            NS,
            &g,
            &bad_join.hash,
            &bad,
            &CADENCE,
            0,
        )
        .is_err());
    }
}
