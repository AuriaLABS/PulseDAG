//! Observational PulseClock v1.
//!
//! Computes a DAG-derived pulse tuple from selected-chain metadata.
//! This module does not participate in consensus validation, PoW, GHOSTDAG
//! selection, covenant execution, or `contracts_enabled`.
//!
//! Spec: `docs/PULSECLOCK_V1.md`. Domain: `PulseDAG:pulse:v1`.

use crate::{
    selection::{preferred_tip_hash, rebuild_selected_chain_from_tip},
    state::ChainState,
    types::Hash,
};

/// Canonical signing / API domain. Statements that later bind a pulse MUST
/// include this domain, `chain_id`, pulse version `1`, and `selected_tip`.
pub const PULSE_DOMAIN_V1: &str = "PulseDAG:pulse:v1";
pub const PULSE_VERSION_V1: u32 = 1;
pub const PULSE_WINDOW_K_V1: u32 = 11;
pub const PULSE_UNCERTAINTY_POLICY_MAX_SECS_V1: u32 = 600;

/// Unpublished operational envelope: do not claim a measured finality depth.
/// `finality_lag` is therefore `pulse_height` (nothing after genesis pulse 0
/// is treated as envelope-final).
pub const PULSE_FINALITY_DEPTH_UNPUBLISHED_V1: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseObservationV1 {
    pub pulse_version: u32,
    pub domain: &'static str,
    pub chain_id: String,
    pub selected_tip: Hash,
    pub pulse_height: u64,
    pub pulse_time: i64,
    pub window_k: u32,
    pub sample_count: u32,
    pub uncertainty_secs: u32,
    pub finality_lag: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PulseClockV1Error {
    SelectedTipUnavailable,
    SelectedTipBlockMissing { hash: Hash },
}

impl std::fmt::Display for PulseClockV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelectedTipUnavailable => {
                write!(f, "selected tip is unavailable; PulseClock fails closed")
            }
            Self::SelectedTipBlockMissing { hash } => {
                write!(
                    f,
                    "selected tip {hash} is missing from DAG blocks; PulseClock fails closed"
                )
            }
        }
    }
}

impl std::error::Error for PulseClockV1Error {}

pub fn observe_pulse_v1(state: &ChainState) -> Result<PulseObservationV1, PulseClockV1Error> {
    let selected_tip =
        preferred_tip_hash(state).ok_or(PulseClockV1Error::SelectedTipUnavailable)?;
    observe_pulse_v1_at(state, &selected_tip)
}

pub fn observe_pulse_v1_at(
    state: &ChainState,
    selected_tip: &Hash,
) -> Result<PulseObservationV1, PulseClockV1Error> {
    let tip_block = state.dag.blocks.get(selected_tip).ok_or_else(|| {
        PulseClockV1Error::SelectedTipBlockMissing {
            hash: selected_tip.clone(),
        }
    })?;

    let pulse_height = tip_block.header.blue_score;
    let samples = collect_blue_window_timestamps(state, selected_tip, PULSE_WINDOW_K_V1);
    let (pulse_time, uncertainty_secs) = robust_pulse_time(&samples);
    let finality_lag = pulse_height.saturating_sub(PULSE_FINALITY_DEPTH_UNPUBLISHED_V1);

    Ok(PulseObservationV1 {
        pulse_version: PULSE_VERSION_V1,
        domain: PULSE_DOMAIN_V1,
        chain_id: state.chain_id.clone(),
        selected_tip: selected_tip.clone(),
        pulse_height,
        pulse_time,
        window_k: PULSE_WINDOW_K_V1,
        sample_count: samples.len() as u32,
        uncertainty_secs,
        finality_lag,
    })
}

fn collect_blue_window_timestamps(
    state: &ChainState,
    selected_tip: &Hash,
    window_k: u32,
) -> Vec<i64> {
    let mut chain = if state.dag.selected_chain.last() == Some(selected_tip)
        && !state.dag.selected_chain.is_empty()
    {
        state.dag.selected_chain.clone()
    } else {
        rebuild_selected_chain_from_tip(state, Some(selected_tip.clone()))
    };
    if chain.last() != Some(selected_tip) {
        chain = rebuild_selected_chain_from_tip(state, Some(selected_tip.clone()));
    }

    let mut samples = Vec::new();
    for hash in chain.iter().rev() {
        if samples.len() >= window_k as usize {
            break;
        }
        let Some(block) = state.dag.blocks.get(hash) else {
            continue;
        };
        if !is_selected_chain_blue_sample(state, hash) {
            continue;
        }
        if let Some(ts) = admitted_unix_timestamp(state, hash, block.header.timestamp) {
            samples.push(ts);
        }
    }
    samples
}

fn is_selected_chain_blue_sample(state: &ChainState, hash: &Hash) -> bool {
    // Selected-parent walk is the blue spine. A block listed in any merge-set
    // red set of a descendant on that spine is not a blue sample.
    !state
        .dag
        .merge_set_reds
        .values()
        .any(|reds| reds.iter().any(|red| red == hash))
}

fn admitted_unix_timestamp(state: &ChainState, hash: &Hash, timestamp: u64) -> Option<i64> {
    if timestamp == 0 && hash != &state.dag.genesis_hash {
        return None;
    }
    i64::try_from(timestamp).ok()
}

fn robust_pulse_time(samples: &[i64]) -> (i64, u32) {
    if samples.is_empty() {
        return (0, PULSE_UNCERTAINTY_POLICY_MAX_SECS_V1);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    if sorted.len() < 3 {
        return (
            median_sorted(&sorted),
            PULSE_UNCERTAINTY_POLICY_MAX_SECS_V1,
        );
    }
    let clipped = &sorted[1..sorted.len() - 1];
    let min = *clipped.first().expect("clipped window non-empty");
    let max = *clipped.last().expect("clipped window non-empty");
    let range = max.saturating_sub(min) as u64;
    let uncertainty = ((range + 1) / 2).clamp(1, u64::from(PULSE_UNCERTAINTY_POLICY_MAX_SECS_V1));
    (
        median_sorted(clipped),
        uncertainty as u32,
    )
}

fn median_sorted(sorted: &[i64]) -> i64 {
    match sorted.len() {
        0 => 0,
        n if n % 2 == 1 => sorted[n / 2],
        n => {
            let lo = sorted[n / 2 - 1];
            let hi = sorted[n / 2];
            lo + (hi - lo) / 2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        selection::refresh_selected_chain,
        types::{Block, BlockHeader},
    };

    fn block(hash: &str, parent: &str, height: u64, blue_score: u64, timestamp: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 2,
                parents: vec![parent.to_string()],
                timestamp,
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("merkle-{hash}"),
                state_root: format!("state-{hash}"),
                blue_score,
                height,
            },
            transactions: vec![],
        }
    }

    fn with_genesis_time() -> crate::ChainState {
        let mut state = init_chain_state("pulse-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        if let Some(block) = state.dag.blocks.get_mut(&genesis) {
            block.header.timestamp = 1_700_000_000;
        }
        state
    }

    fn extend_selected(state: &mut crate::ChainState, block: Block) {
        let hash = block.hash.clone();
        let parent = block.header.parents.first().cloned();
        state.dag.tips.remove(parent.as_deref().unwrap_or(""));
        state.dag.tips.insert(hash.clone());
        if let Some(parent) = parent {
            state
                .dag
                .selected_parents
                .insert(hash.clone(), Some(parent));
        }
        state.dag.best_height = state.dag.best_height.max(block.header.height);
        state.dag.blocks.insert(hash, block);
        refresh_selected_chain(state);
    }

    #[test]
    fn genesis_only_reports_blue_score_and_policy_max_uncertainty() {
        let state = init_chain_state("pulse-test".to_string());
        let pulse = observe_pulse_v1(&state).expect("genesis tip is available");
        assert_eq!(pulse.domain, PULSE_DOMAIN_V1);
        assert_eq!(pulse.pulse_version, 1);
        assert_eq!(pulse.chain_id, "pulse-test");
        assert_eq!(pulse.selected_tip, state.dag.genesis_hash);
        assert_eq!(pulse.pulse_height, 0);
        assert_eq!(pulse.pulse_time, 0);
        assert_eq!(pulse.window_k, 11);
        assert_eq!(pulse.sample_count, 1);
        assert_eq!(pulse.uncertainty_secs, 600);
        assert_eq!(pulse.finality_lag, 0);
    }

    #[test]
    fn same_selected_dag_is_deterministic_regardless_of_insert_order_metadata() {
        let mut a = with_genesis_time();
        let genesis = a.dag.genesis_hash.clone();
        extend_selected(&mut a, block("b1", &genesis, 1, 1, 1_700_000_010));
        extend_selected(&mut a, block("b2", "b1", 2, 2, 1_700_000_020));
        extend_selected(&mut a, block("b3", "b2", 3, 3, 1_700_000_040));
        extend_selected(&mut a, block("b4", "b3", 4, 4, 1_700_000_050));
        extend_selected(&mut a, block("b5", "b4", 5, 5, 1_700_000_080));

        let first = observe_pulse_v1(&a).unwrap();
        let second = observe_pulse_v1_at(&a, &first.selected_tip).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.selected_tip, "b5");
        assert_eq!(first.pulse_height, 5);
        // 6 samples (genesis+5): clip 1_700_000_000 and 080; remaining 010,020,040,050.
        // median 30; uncertainty ceil((50-10)/2)=20.
        assert_eq!(first.pulse_time, 1_700_000_030);
        assert_eq!(first.uncertainty_secs, 20);
    }

    #[test]
    fn clipped_skew_widens_uncertainty_and_keeps_median() {
        let mut state = with_genesis_time();
        let genesis = state.dag.genesis_hash.clone();
        extend_selected(&mut state, block("b1", &genesis, 1, 1, 1_700_000_100));
        extend_selected(&mut state, block("b2", "b1", 2, 2, 1_700_000_110));
        extend_selected(&mut state, block("b3", "b2", 3, 3, 1_700_000_120));
        extend_selected(&mut state, block("b4", "b3", 4, 4, 1_700_000_130));
        extend_selected(&mut state, block("b5", "b4", 5, 5, 1_700_010_000));

        let pulse = observe_pulse_v1(&state).unwrap();
        // clip genesis and 10_000 outlier; remaining 100..130. median 115. range 30/2=15.
        assert_eq!(pulse.pulse_time, 1_700_000_115);
        assert_eq!(pulse.uncertainty_secs, 15);
    }

    #[test]
    fn zero_non_genesis_timestamp_is_dropped_not_replaced() {
        let mut state = init_chain_state("pulse-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        extend_selected(&mut state, block("b1", &genesis, 1, 1, 0));
        extend_selected(&mut state, block("b2", "b1", 2, 2, 50));
        let pulse = observe_pulse_v1(&state).unwrap();
        assert_eq!(pulse.sample_count, 2); // genesis + b2
        assert_eq!(pulse.uncertainty_secs, 600);
        assert_eq!(pulse.pulse_time, 25);
    }

    #[test]
    fn missing_tip_fails_closed() {
        let state = init_chain_state("pulse-test".to_string());
        let err = observe_pulse_v1_at(&state, &"missing".to_string()).unwrap_err();
        assert_eq!(
            err,
            PulseClockV1Error::SelectedTipBlockMissing {
                hash: "missing".to_string()
            }
        );
    }

    #[test]
    fn merge_set_red_blocks_are_not_sampled() {
        let mut state = init_chain_state("pulse-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        extend_selected(&mut state, block("b1", &genesis, 1, 1, 10));
        extend_selected(&mut state, block("b2", "b1", 2, 2, 20));
        state
            .dag
            .merge_set_reds
            .insert("b2".to_string(), vec!["b1".to_string()]);
        let pulse = observe_pulse_v1(&state).unwrap();
        // genesis ts 0 + b2 ts 20; b1 dropped as red.
        assert_eq!(pulse.sample_count, 2);
        assert_eq!(pulse.pulse_time, 10);
        assert_eq!(pulse.uncertainty_secs, 600);
    }
}
