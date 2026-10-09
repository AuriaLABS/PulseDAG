//! Unactivated v3 subsecond difficulty arithmetic.
//!
//! This is a consensus-design building block, NOT a mining/P2P activation.
//! V1/v2 headers encode Unix seconds. Future v3 consensus must first freeze a
//! distinct versioned header with nanosecond timestamps and apply this same
//! policy in templates, mined and P2P admission, replay, and persistence.
//! Never feed v1/v2 second timestamps into this nanosecond-only API.

use crate::{
    monetary_v3::PRODUCTION_CADENCE_TARGET_INTERVAL_NS_V3,
    pow::{bits_from_target, target_from_bits},
    retarget::{
        consensus_min_target, consensus_pow_limit_target,
        consensus_target_multiplier_bps_from_work_multiplier, scale_target_ratio,
    },
};

pub const PRODUCTION_V3_RETARGET_WINDOW: usize = 20;
pub const PRODUCTION_V3_MAX_FUTURE_DRIFT_NS: u64 = PRODUCTION_CADENCE_TARGET_INTERVAL_NS_V3 * 2;
const BASIS_POINTS: u64 = 10_000;
const DEADBAND_BPS: u64 = 800;
const MIN_WORK_MULTIPLIER_BPS: u64 = 8_000;
const MAX_WORK_MULTIPLIER_BPS: u64 = 12_500;
const DAMPING_DIVISOR: i128 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V3RetargetSample {
    /// Unix time in nanoseconds, not the v1/v2 BlockHeader timestamp in seconds.
    pub timestamp_ns: u64,
    /// Canonical compact target bits for the observed block.
    pub bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V3RetargetDecision {
    pub expected_bits: u32,
    pub observed_intervals: usize,
    pub average_interval_ns: u64,
    pub work_multiplier_bps: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum V3SubsecondConsensusError {
    #[error("v3 retarget requires at least one parent-chain sample")]
    EmptyWindow,
    #[error("v3 retarget window exceeds the frozen limit of 20 samples")]
    WindowTooLarge,
    #[error("v3 nanosecond timestamp must be non-zero")]
    ZeroTimestamp,
    #[error("v3 parent-chain timestamps must be strictly decreasing newest-first")]
    NonMonotonicWindow,
    #[error("v3 candidate timestamp must be strictly newer than its newest parent")]
    StaleCandidateTimestamp,
    #[error("v3 timestamp arithmetic overflows u64 nanoseconds")]
    TimestampOverflow,
    #[error("v3 candidate timestamp exceeds the frozen future-drift limit")]
    FutureTimestamp,
}

fn retarget_work_multiplier_bps(average_interval_ns: u64) -> u64 {
    // The nonzero average is checked in the caller. u128 intermediate keeps
    // every u64 nanosecond input deterministic without saturating products.
    let raw = u128::from(PRODUCTION_CADENCE_TARGET_INTERVAL_NS_V3) * u128::from(BASIS_POINTS)
        / u128::from(average_interval_ns.max(1));
    let lower = u128::from(BASIS_POINTS - DEADBAND_BPS);
    let upper = u128::from(BASIS_POINTS + DEADBAND_BPS);
    if (lower..=upper).contains(&raw) {
        return BASIS_POINTS;
    }
    let damped =
        i128::from(BASIS_POINTS) + ((raw as i128 - i128::from(BASIS_POINTS)) / DAMPING_DIVISOR);
    (damped as u64).clamp(MIN_WORK_MULTIPLIER_BPS, MAX_WORK_MULTIPLIER_BPS)
}

/// Determine compact PoW bits from a newest-first v3 parent-chain window.
///
/// This function has no access to ChainState, protocol activation or wall time:
/// the caller must supply a verified, selected-parent-scoped, versioned
/// nanosecond sample. It does not enable v3 startup or acceptance.
pub fn expected_difficulty_for_v3_window_ns(
    newest_first: &[V3RetargetSample],
) -> Result<V3RetargetDecision, V3SubsecondConsensusError> {
    if newest_first.is_empty() {
        return Err(V3SubsecondConsensusError::EmptyWindow);
    }
    if newest_first.len() > PRODUCTION_V3_RETARGET_WINDOW {
        return Err(V3SubsecondConsensusError::WindowTooLarge);
    }
    if newest_first.iter().any(|sample| sample.timestamp_ns == 0) {
        return Err(V3SubsecondConsensusError::ZeroTimestamp);
    }
    let mut total_interval_ns: u128 = 0;
    for pair in newest_first.windows(2) {
        let delta_ns = pair[0]
            .timestamp_ns
            .checked_sub(pair[1].timestamp_ns)
            .filter(|delta| *delta > 0)
            .ok_or(V3SubsecondConsensusError::NonMonotonicWindow)?;
        total_interval_ns += u128::from(delta_ns);
    }
    let intervals = newest_first.len() - 1;
    let average_interval_ns = if intervals == 0 {
        PRODUCTION_CADENCE_TARGET_INTERVAL_NS_V3
    } else {
        // The average is bounded by a single u64 timestamp difference, even
        // when summing up to 19 differences in a u128 accumulator.
        (total_interval_ns / intervals as u128) as u64
    };
    let work_multiplier_bps = retarget_work_multiplier_bps(average_interval_ns);
    let current_target = target_from_bits(newest_first[0].bits);
    let pow_limit = consensus_pow_limit_target();
    let min_target = consensus_min_target();
    let bounded_current = current_target.clamp(min_target, pow_limit);
    let target_multiplier_bps =
        consensus_target_multiplier_bps_from_work_multiplier(work_multiplier_bps);
    let (scaled, overflow) =
        scale_target_ratio(&bounded_current, target_multiplier_bps, BASIS_POINTS);
    let bounded_target = if overflow || scaled > pow_limit {
        pow_limit
    } else if scaled < min_target {
        min_target
    } else {
        scaled
    };
    Ok(V3RetargetDecision {
        expected_bits: bits_from_target(&bounded_target),
        observed_intervals: intervals,
        average_interval_ns,
        work_multiplier_bps,
    })
}

/// Pure candidate timestamp check in explicitly versioned Unix nanoseconds.
///
/// Future consensus must call this from all acceptance paths and use the same
/// timestamp units in its header hash and PoW commitments. Until then the
/// pre-storage startup gate for monetary_v3 must remain enabled.
pub fn validate_candidate_timestamp_v3_ns(
    timestamp_ns: u64,
    newest_parent_timestamp_ns: u64,
    now_ns: u64,
) -> Result<(), V3SubsecondConsensusError> {
    if timestamp_ns == 0 || newest_parent_timestamp_ns == 0 || now_ns == 0 {
        return Err(V3SubsecondConsensusError::ZeroTimestamp);
    }
    if timestamp_ns <= newest_parent_timestamp_ns {
        return Err(V3SubsecondConsensusError::StaleCandidateTimestamp);
    }
    let upper = now_ns
        .checked_add(PRODUCTION_V3_MAX_FUTURE_DRIFT_NS)
        .ok_or(V3SubsecondConsensusError::TimestampOverflow)?;
    if timestamp_ns > upper {
        return Err(V3SubsecondConsensusError::FutureTimestamp);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retarget::CONSENSUS_POW_LIMIT_BITS;

    const BASE: u64 = 1_800_000_000_000_000_000;
    const FIXED_BITS: u32 = 0x1e0f_ffff;

    fn sample(timestamp_ns: u64) -> V3RetargetSample {
        V3RetargetSample {
            timestamp_ns,
            bits: FIXED_BITS,
        }
    }

    #[test]
    fn stable_five_hundred_milliseconds_preserves_compact_bits() {
        let decision = expected_difficulty_for_v3_window_ns(&[
            sample(BASE + 1_000_000_000),
            sample(BASE + 500_000_000),
            sample(BASE),
        ])
        .unwrap();
        assert_eq!(decision.observed_intervals, 2);
        assert_eq!(decision.average_interval_ns, 500_000_000);
        assert_eq!(decision.work_multiplier_bps, BASIS_POINTS);
        assert_eq!(decision.expected_bits, FIXED_BITS);
    }

    #[test]
    fn twice_as_fast_hardens_and_twice_as_slow_relaxes_work() {
        let fast = expected_difficulty_for_v3_window_ns(&[
            sample(BASE + 500_000_000),
            sample(BASE + 250_000_000),
            sample(BASE),
        ])
        .unwrap();
        let slow = expected_difficulty_for_v3_window_ns(&[
            sample(BASE + 2_000_000_000),
            sample(BASE + 1_000_000_000),
            sample(BASE),
        ])
        .unwrap();
        let original_target = target_from_bits(FIXED_BITS);
        assert_eq!(fast.average_interval_ns, 250_000_000);
        assert_eq!(slow.average_interval_ns, 1_000_000_000);
        assert_eq!(fast.work_multiplier_bps, MAX_WORK_MULTIPLIER_BPS);
        assert_eq!(slow.work_multiplier_bps, MIN_WORK_MULTIPLIER_BPS);
        assert!(target_from_bits(fast.expected_bits) < original_target);
        assert!(target_from_bits(slow.expected_bits) > original_target);
    }

    #[test]
    fn two_blocks_in_one_second_keep_subsecond_signal() {
        let decision = expected_difficulty_for_v3_window_ns(&[
            sample(BASE + 850_000_000),
            sample(BASE + 600_000_000),
            sample(BASE + 100_000_000),
        ])
        .unwrap();
        assert_eq!(decision.average_interval_ns, 375_000_000);
        assert!(decision.work_multiplier_bps > BASIS_POINTS);
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&[
                sample(BASE + 850_000_000),
                sample(BASE + 600_000_000),
                sample(BASE + 100_000_000)
            ])
            .unwrap(),
            decision
        );
    }

    #[test]
    fn crossing_unix_second_boundary_preserves_exact_500ms() {
        let decision = expected_difficulty_for_v3_window_ns(&[
            sample(BASE + 1_250_000_000),
            sample(BASE + 750_000_000),
            sample(BASE + 250_000_000),
        ])
        .unwrap();
        assert_eq!(decision.average_interval_ns, 500_000_000);
        assert_eq!(decision.work_multiplier_bps, BASIS_POINTS);
    }

    #[test]
    fn first_sample_uses_frozen_cadence_not_legacy_sixty_seconds() {
        let initial = expected_difficulty_for_v3_window_ns(&[V3RetargetSample {
            timestamp_ns: BASE,
            bits: CONSENSUS_POW_LIMIT_BITS,
        }])
        .unwrap();
        assert_eq!(initial.observed_intervals, 0);
        assert_eq!(initial.average_interval_ns, 500_000_000);
        assert_eq!(initial.expected_bits, CONSENSUS_POW_LIMIT_BITS);
        assert_eq!(crate::retarget::CONSENSUS_TARGET_BLOCK_INTERVAL_SECS, 60);
    }

    #[test]
    fn invalid_windows_fail_without_silent_saturation() {
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&[]),
            Err(V3SubsecondConsensusError::EmptyWindow)
        );
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&[sample(BASE), sample(BASE)]),
            Err(V3SubsecondConsensusError::NonMonotonicWindow)
        );
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&[sample(BASE), sample(BASE + 1)]),
            Err(V3SubsecondConsensusError::NonMonotonicWindow)
        );
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&[sample(0)]),
            Err(V3SubsecondConsensusError::ZeroTimestamp)
        );
        assert_eq!(
            expected_difficulty_for_v3_window_ns(&vec![
                sample(BASE);
                PRODUCTION_V3_RETARGET_WINDOW + 1
            ]),
            Err(V3SubsecondConsensusError::WindowTooLarge)
        );
    }

    #[test]
    fn future_drift_and_strict_parent_order_are_fail_closed() {
        assert!(validate_candidate_timestamp_v3_ns(BASE + 500_000_000, BASE, BASE).is_ok());
        assert_eq!(
            validate_candidate_timestamp_v3_ns(BASE, BASE, BASE),
            Err(V3SubsecondConsensusError::StaleCandidateTimestamp)
        );
        assert_eq!(
            validate_candidate_timestamp_v3_ns(BASE + 1_000_000_001, BASE, BASE),
            Err(V3SubsecondConsensusError::FutureTimestamp)
        );
        assert_eq!(
            validate_candidate_timestamp_v3_ns(u64::MAX, BASE, u64::MAX),
            Err(V3SubsecondConsensusError::TimestampOverflow)
        );
    }
}
