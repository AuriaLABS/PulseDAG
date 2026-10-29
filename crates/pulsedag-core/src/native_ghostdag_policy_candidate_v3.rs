//! Dormant native-v3 GHOSTDAG policy *candidate*, for reproducible study ONLY.
//!
//! Consumes a fully genesis-bound, KHeavyHash-authenticated parent-DAG and
//! derives selected-parent, greedy K-cluster blue/red, inclusive blue-work and
//! score from immutable headers. Rules are PROPOSED (#1309), not frozen.
//! No mining, P2P, rewards, UTXO, contract execution or activation calls this.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use crate::{
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_parent_dag_context_v3::MAX_DORMANT_V3_DAG_HEADERS,
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    pow::{target_from_bits, PowTarget},
    pow_v3::validate_v3_pow_target,
    types::BlockId,
};

fn invalid(reason: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED dormant v3 GHOSTDAG policy: {}",
        reason.into()
    ))
}

/// Checked 512-bit unsigned accumulator; little-endian 64-bit limbs.
/// The 512-bit width itself remains a candidate, not a frozen consensus rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CandidateWork512(pub [u64; 8]);

impl CandidateWork512 {
    fn compare(&self, other: &Self) -> Ordering {
        for i in (0..8).rev() {
            match self.0[i].cmp(&other.0[i]) {
                Ordering::Equal => {}
                x => return x,
            }
        }
        Ordering::Equal
    }

    fn checked_add(self, other: Self) -> Result<Self, PulseError> {
        let mut out = [0u64; 8];
        let mut carry = 0u128;
        for (i, dst) in out.iter_mut().enumerate() {
            let sum = u128::from(self.0[i]) + u128::from(other.0[i]) + carry;
            *dst = sum as u64;
            carry = sum >> 64;
        }
        if carry != 0 {
            return Err(invalid("512-bit cumulative work overflow"));
        }
        Ok(Self(out))
    }

    fn subtract(self, other: Self) -> Self {
        let mut out = [0u64; 8];
        let mut borrow = false;
        for (i, dst) in out.iter_mut().enumerate() {
            let (first, b1) = self.0[i].overflowing_sub(other.0[i]);
            let (second, b2) = first.overflowing_sub(u64::from(borrow));
            *dst = second;
            borrow = b1 || b2;
        }
        debug_assert!(!borrow);
        Self(out)
    }

    fn shift_left_one(&mut self) {
        let mut carry = 0u64;
        for limb in &mut self.0 {
            let next = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        debug_assert_eq!(carry, 0);
    }

    /// Human-readable exact arithmetic: avoid decimals through f64/u128.
    pub fn decimal(self) -> String {
        if self.0 == [0; 8] {
            return "0".into();
        }
        let mut rest = self.0;
        let mut digits = Vec::new();
        while rest.iter().any(|v| *v != 0) {
            let mut carry = 0u128;
            for limb in rest.iter_mut().rev() {
                let value = (carry << 64) | u128::from(*limb);
                *limb = (value / 10) as u64;
                carry = value % 10;
            }
            digits.push(char::from(b'0' + carry as u8));
        }
        digits.iter().rev().collect()
    }
}

/// Independently compute floor(2^256 / (T+1)) from canonical 256-bit target.
/// Bitwise *integer* long division; no f64, saturation, u128 truncation or
/// dependency on legacy v1 blue_work. Called only after PoW authentication.
fn candidate_work(bits: u32) -> Result<CandidateWork512, PulseError> {
    validate_v3_pow_target(bits)?;
    let target: PowTarget = target_from_bits(bits);
    let mut denominator = CandidateWork512::default();
    for (i, chunk) in target.chunks_exact(8).rev().enumerate() {
        denominator.0[i] =
            u64::from_be_bytes(chunk.try_into().map_err(|_| invalid("target width"))?);
    }
    denominator = denominator.checked_add(CandidateWork512([1, 0, 0, 0, 0, 0, 0, 0]))?;
    let mut remainder = CandidateWork512::default();
    let mut quotient = CandidateWork512::default();
    for bit in (0..=256usize).rev() {
        remainder.shift_left_one();
        if bit == 256 {
            remainder.0[0] |= 1; // numerator = 2^256
        }
        if remainder.compare(&denominator) != Ordering::Less {
            remainder = remainder.subtract(denominator);
            quotient.0[bit / 64] |= 1u64 << (bit % 64);
        }
    }
    if quotient.0 == [0; 8] || quotient.0[4..].iter().any(|x| *x != 0) {
        return Err(invalid("target work must fit positive u256"));
    }
    Ok(quotient)
}

/// Explicitly proposed K, NOT a network parameter. Candidate metadata is
/// fully re-derived on each call and is not part of P2P/consensus authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateBlockMetadata {
    pub selected_parent: Option<BlockId>,
    pub blue_score: u64,
    pub blue_work: CandidateWork512,
    pub merge_blues: Vec<BlockId>,
    pub merge_reds: Vec<BlockId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateGhostdagEvaluation {
    pub blocks: BTreeMap<BlockId, CandidateBlockMetadata>,
    pub k_trial: usize,
}

/// A preview of the SELECTED-PARENT SPINE only, never a monetary reorg.
///
/// The full DAG merge-set order can also contain blue/red side blocks; these
/// lists MUST NOT be used as UTXO disconnect/connect or coinbase replay lists.
/// Recomputes no consensus policy and mutates no block, chain or account state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantCandidateSelectedChainDelta {
    pub common_selected_ancestor: BlockId,
    /// Selected spine, from old tip down to (but not including) common ancestor.
    pub detach_tip_first: Vec<BlockId>,
    /// Selected spine, from common ancestor's child through new tip.
    pub attach_ancestor_first: Vec<BlockId>,
    pub old_tip_blue_work: CandidateWork512,
    pub new_tip_blue_work: CandidateWork512,
}

/// Preview a selected-spine switch within one ALREADY fully authenticated
/// trial evaluation. This is not a policy choice between tips; the caller
/// supplies both. Missing tips/cycles/genesis incompatibility fail closed.
pub fn preview_dormant_v3_candidate_selected_chain_delta(
    trial: &CandidateGhostdagEvaluation,
    old_tip: &str,
    new_tip: &str,
) -> Result<DormantCandidateSelectedChainDelta, PulseError> {
    fn selected_to_genesis(
        trial: &CandidateGhostdagEvaluation,
        tip: &str,
    ) -> Result<Vec<BlockId>, PulseError> {
        let mut backwards = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = Some(tip.to_owned());
        while let Some(hash) = current {
            if !seen.insert(hash.clone()) || seen.len() > trial.blocks.len() {
                return Err(invalid("cycle / unbounded selected-parent spine"));
            }
            let node = trial
                .blocks
                .get(&hash)
                .ok_or_else(|| invalid("missing tip or selected-parent metadata"))?;
            current = node.selected_parent.clone();
            backwards.push(hash);
        }
        backwards.reverse();
        Ok(backwards)
    }

    let old_metadata = trial
        .blocks
        .get(old_tip)
        .ok_or_else(|| invalid("missing old tip in authenticated trial"))?;
    let new_metadata = trial
        .blocks
        .get(new_tip)
        .ok_or_else(|| invalid("missing new tip in authenticated trial"))?;
    let old_spine = selected_to_genesis(trial, old_tip)?;
    let new_spine = selected_to_genesis(trial, new_tip)?;
    let prefix = old_spine
        .iter()
        .zip(new_spine.iter())
        .take_while(|(a, b)| a == b)
        .count();
    if prefix == 0 {
        return Err(invalid("selected tips have incompatible genesis ancestry"));
    }
    Ok(DormantCandidateSelectedChainDelta {
        common_selected_ancestor: old_spine[prefix - 1].clone(),
        detach_tip_first: old_spine[prefix..].iter().rev().cloned().collect(),
        attach_ancestor_first: new_spine[prefix..].to_vec(),
        old_tip_blue_work: old_metadata.blue_work,
        new_tip_blue_work: new_metadata.blue_work,
    })
}

fn is_incomparable(a: &str, b: &str, past: &BTreeMap<BlockId, BTreeSet<BlockId>>) -> bool {
    !past[a].contains(b) && !past[b].contains(a)
}

fn priority(
    a: &str,
    b: &str,
    derived: &BTreeMap<BlockId, CandidateBlockMetadata>,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Ordering {
    let left = &derived[a];
    let right = &derived[b];
    left.blue_work
        .compare(&right.blue_work)
        .then_with(|| left.blue_score.cmp(&right.blue_score))
        .then_with(|| headers[a].height.cmp(&headers[b].height))
        .then_with(|| b.cmp(a)) // smaller canonical hash wins final tie
}

/// Authenticates COMPLETE native v3 ancestry BEFORE touching candidate K work.
/// Fail-closed for missing/invalid side-PoW, cycles, redundant direct parents,
/// incorrect committed score, 512-bit overflow and expired local graph budget.
///
/// This deliberately has no production authority and no default K. The
/// verified graph budget (currently 128 headers) is only a test resource cap.
pub fn derive_dormant_native_v3_ghostdag_trial(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    candidate_tips: &[BlockId],
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    trial_k: usize,
) -> Result<CandidateGhostdagEvaluation, PulseError> {
    if trial_k > MAX_DORMANT_V3_DAG_HEADERS {
        return Err(invalid("trial K exceeds local diagnostic budget"));
    }
    let topo = derive_dormant_native_v3_topological_evaluation(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        candidate_tips,
        headers,
    )?;
    let mut past: BTreeMap<BlockId, BTreeSet<BlockId>> = BTreeMap::new();
    let mut blue_past: BTreeMap<BlockId, BTreeSet<BlockId>> = BTreeMap::new();
    // Exact incomparable-blue counts per authenticated blue past. Carry these
    // across selected-parent ancestry instead of re-scanning every pair for
    // every pending merge candidate. This is a dormant local-only cache.
    let mut blue_anticone_counts: BTreeMap<BlockId, BTreeMap<BlockId, usize>> = BTreeMap::new();
    let mut derived: BTreeMap<BlockId, CandidateBlockMetadata> = BTreeMap::new();

    for block in topo.ordered_reachable_headers {
        let h = headers
            .get(&block)
            .ok_or_else(|| invalid("missing authenticated block"))?;
        let mut ancestors = BTreeSet::from([block.clone()]);
        for parent in &h.parents {
            ancestors.extend(
                past.get(parent)
                    .ok_or_else(|| invalid("missing parent past"))?
                    .iter()
                    .cloned(),
            );
        }
        if ancestors.len() > MAX_DORMANT_V3_DAG_HEADERS {
            return Err(invalid("inclusive past exceeds local graph budget"));
        }
        if block == frozen_genesis_hash {
            if !h.parents.is_empty() || h.blue_score != 0 {
                return Err(invalid("invalid genesis score or parents"));
            }
            past.insert(block.clone(), ancestors);
            blue_past.insert(block.clone(), BTreeSet::from([block.clone()]));
            blue_anticone_counts.insert(block.clone(), BTreeMap::from([(block.clone(), 0)]));
            derived.insert(
                block,
                CandidateBlockMetadata {
                    selected_parent: None,
                    blue_score: 0,
                    blue_work: CandidateWork512::default(),
                    merge_blues: vec![],
                    merge_reds: vec![],
                },
            );
            continue;
        }

        // Do not silently accept redundant/non-antichain parent references in
        // this *candidate* policy, irrespective of header's wider shape limit.
        for (i, a) in h.parents.iter().enumerate() {
            for b in h.parents.iter().skip(i + 1) {
                if past[a].contains(b) || past[b].contains(a) {
                    return Err(invalid("direct parents do not form an antichain"));
                }
            }
        }
        let selected = h
            .parents
            .iter()
            .max_by(|a, b| priority(a, b, &derived, headers))
            .ok_or_else(|| invalid("non-genesis block lacks parents"))?
            .clone();
        let mut remaining = ancestors
            .difference(&past[&selected])
            .cloned()
            .collect::<BTreeSet<_>>();
        remaining.remove(&block); // candidate H itself is NOT part of its merge set
        let mut blues = vec![];
        let mut reds = vec![];
        let mut accepted_blue_past = blue_past[&selected].clone();
        let mut anticone_counts = blue_anticone_counts
            .get(&selected)
            .ok_or_else(|| invalid("missing selected-parent blue anticone counters"))?
            .clone();
        if accepted_blue_past.len() != anticone_counts.len() {
            return Err(invalid("selected-parent anticone cache is incomplete"));
        }

        while !remaining.is_empty() {
            // Kahn-ready: all parents must have left the unprocessed merge set.
            let next = remaining
                .iter()
                .filter(|v| {
                    headers[*v]
                        .parents
                        .iter()
                        .all(|parent| !remaining.contains(parent))
                })
                .max_by(|a, b| priority(a, b, &derived, headers))
                .ok_or_else(|| invalid("merge-set ordering has missing predecessors"))?
                .clone();
            remaining.remove(&next);
            // A candidate can increment only the counts of blues which are
            // incomparable to it. Cache the preceding blue-past's exact counts
            // instead of re-counting O(B²) pairs for EVERY merge candidate.
            // This is algebraically identical to BOTH proposed K-cluster tests:
            // (1) number of incomparable old blues <= K; and (2) no old blue
            // with an increment already has K incomparable blues.
            let incomparable_blues = accepted_blue_past
                .iter()
                .filter(|existing| is_incomparable(&next, existing, &past))
                .cloned()
                .collect::<Vec<_>>();
            let all_prior_stay_bounded = incomparable_blues.iter().all(|existing| {
                anticone_counts
                    .get(existing)
                    .is_some_and(|count| *count < trial_k)
            });
            if incomparable_blues.len() <= trial_k && all_prior_stay_bounded {
                for existing in &incomparable_blues {
                    let count = anticone_counts
                        .get_mut(existing)
                        .ok_or_else(|| invalid("missing existing blue anticone count"))?;
                    *count = count
                        .checked_add(1)
                        .ok_or_else(|| invalid("blue anticone count overflow"))?;
                }
                anticone_counts.insert(next.clone(), incomparable_blues.len());
                accepted_blue_past.insert(next.clone());
                blues.push(next);
            } else {
                reds.push(next);
            }
        }

        let parent = &derived[&selected];
        let score = parent
            .blue_score
            .checked_add(1)
            .and_then(|v| v.checked_add(blues.len() as u64))
            .ok_or_else(|| invalid("blue score overflow"))?;
        if h.blue_score != score {
            return Err(invalid(format!(
                "committed blue score differs from independently derived candidate at {block}"
            )));
        }
        let mut work = parent
            .blue_work
            .checked_add(candidate_work(h.difficulty)?)?;
        for blue in &blues {
            work = work.checked_add(candidate_work(headers[blue].difficulty)?)?;
        }
        past.insert(block.clone(), ancestors);
        // The current block descends from every accepted blue in its inclusive
        // authenticated past, so its own anticone count is always zero.
        accepted_blue_past.insert(block.clone());
        anticone_counts.insert(block.clone(), 0);
        if accepted_blue_past.len() != anticone_counts.len() {
            return Err(invalid("derived blue anticone cache is incomplete"));
        }
        blue_anticone_counts.insert(block.clone(), anticone_counts);
        blue_past.insert(block.clone(), accepted_blue_past);
        derived.insert(
            block,
            CandidateBlockMetadata {
                selected_parent: Some(selected),
                blue_score: score,
                blue_work: work,
                merge_blues: blues,
                merge_reds: reds,
            },
        );
    }
    Ok(CandidateGhostdagEvaluation {
        blocks: derived,
        k_trial: trial_k,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_256_bit_work_and_checked_512_bit_math() {
        assert_eq!(candidate_work(0x207f_ffff).unwrap().decimal(), "2");
        assert_eq!(candidate_work(0x1f7f_ffff).unwrap().decimal(), "512");
        assert_eq!(candidate_work(0x1e7f_ffff).unwrap().decimal(), "131072");
        assert_eq!(candidate_work(0x1d00_ffff).unwrap().decimal(), "4295032833");
        assert_eq!(
            candidate_work(0x0101_0000).unwrap().decimal(),
            "57896044618658097711785492504343953926634992332820282019728792003956564819968"
        );
        let high = candidate_work(0x0101_0000).unwrap();
        let doubled = high.checked_add(high).unwrap();
        assert_eq!(
            doubled.decimal(),
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        );
        assert!(candidate_work(0).is_err());
        assert!(CandidateWork512([u64::MAX; 8]).checked_add(high).is_err());
        assert_eq!(CandidateWork512::default().decimal(), "0");
    }
}
