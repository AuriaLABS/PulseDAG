use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Approved v3.0.0 mainnet monetary constants.
pub const MAX_SUPPLY_ATOMS: u64 = 100_000_000_000_000_000;
pub const YEAR1_MINING_BUDGET_ATOMS: u64 = 50_000_000_000_000_000;
pub const ATOMS_PER_COIN: u64 = 100_000_000;
pub const ECONOMIC_YEAR_SECONDS: u64 = 31_536_000;
pub const ECONOMIC_YEAR_NS: u128 = ECONOMIC_YEAR_SECONDS as u128 * 1_000_000_000;
pub const COINBASE_MATURITY_SECONDS: u64 = 3_600;
pub const COINBASE_MATURITY_NS: u128 = COINBASE_MATURITY_SECONDS as u128 * 1_000_000_000;

/// At the start of economic year 57 the mathematical geometric remainder is
/// below one atomic unit. v3 settles that final residual atom at the boundary,
/// reaches MAX_SUPPLY_ATOMS exactly, and permanently switches subsidy to zero.
pub const TERMINAL_ECONOMIC_YEAR: u128 = 57;

/// A consensus cadence segment maps canonical monetary-score steps to economic
/// time. `activation_score` is the score at which this interval becomes active
/// for the *next* score transition. Therefore a segment starting at score S
/// governs S -> S+1, S+1 -> S+2, ... until the next segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonetaryCadenceSegment {
    pub activation_score: u64,
    pub target_interval_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MonetaryV3Error {
    #[error("monetary cadence schedule is empty")]
    EmptyCadenceSchedule,
    #[error("first monetary cadence segment must activate at score 0")]
    FirstCadenceMustStartAtZero,
    #[error("monetary cadence target interval must be non-zero")]
    ZeroCadenceInterval,
    #[error("monetary cadence activation scores must be strictly increasing")]
    NonIncreasingCadenceActivation,
    #[error("monetary arithmetic overflow")]
    ArithmeticOverflow,
}

fn validate_cadence_segments(segments: &[MonetaryCadenceSegment]) -> Result<(), MonetaryV3Error> {
    let Some(first) = segments.first() else {
        return Err(MonetaryV3Error::EmptyCadenceSchedule);
    };
    if first.activation_score != 0 {
        return Err(MonetaryV3Error::FirstCadenceMustStartAtZero);
    }
    if first.target_interval_ns == 0 {
        return Err(MonetaryV3Error::ZeroCadenceInterval);
    }

    for pair in segments.windows(2) {
        if pair[1].activation_score <= pair[0].activation_score {
            return Err(MonetaryV3Error::NonIncreasingCadenceActivation);
        }
        if pair[1].target_interval_ns == 0 {
            return Err(MonetaryV3Error::ZeroCadenceInterval);
        }
    }
    Ok(())
}

/// Convert a canonical v3 monetary score into deterministic economic nanoseconds.
///
/// Monetary score 0 is genesis and has zero economic time. Score 1 is the first
/// non-genesis reward-settlement position in the canonical ordered DAG. The
/// cadence schedule is consensus-versioned, so changing BPS changes only the
/// number of score steps per economic second, never the emission curve itself.
pub fn economic_time_ns_for_score(
    score: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<u128, MonetaryV3Error> {
    validate_cadence_segments(segments)?;

    let mut economic_time_ns = 0u128;
    for (index, segment) in segments.iter().enumerate() {
        if score <= segment.activation_score {
            break;
        }
        let next_activation = segments
            .get(index + 1)
            .map(|next| next.activation_score)
            .unwrap_or(score);
        let end_score = score.min(next_activation);
        if end_score <= segment.activation_score {
            continue;
        }
        let steps = end_score - segment.activation_score;
        let contribution = u128::from(steps)
            .checked_mul(u128::from(segment.target_interval_ns))
            .ok_or(MonetaryV3Error::ArithmeticOverflow)?;
        economic_time_ns = economic_time_ns
            .checked_add(contribution)
            .ok_or(MonetaryV3Error::ArithmeticOverflow)?;
        if end_score == score {
            break;
        }
    }
    Ok(economic_time_ns)
}

fn ceil_div_u128(numerator: u128, denominator: u128) -> u128 {
    let quotient = numerator / denominator;
    quotient + u128::from(numerator % denominator != 0)
}

/// Exact cumulative v3 mining issuance scheduled by economic time.
///
/// The curve is linear inside each economic year and halves its annual budget at
/// every 365-day boundary. This computes the exact rational schedule with u128
/// intermediates and rounds cumulative issuance down to atomic units. No
/// floating-point arithmetic is used.
pub fn target_issuance_atoms(economic_time_ns: u128) -> Result<u64, MonetaryV3Error> {
    let epoch = economic_time_ns / ECONOMIC_YEAR_NS;
    if epoch >= TERMINAL_ECONOMIC_YEAR {
        return Ok(MAX_SUPPLY_ATOMS);
    }

    let within_year_ns = economic_time_ns % ECONOMIC_YEAR_NS;

    // Remaining exact supply at time t in epoch e is:
    // MAX * (2*YEAR - within) / (2^(e+1) * YEAR).
    // target issuance = MAX - ceil(remaining), which is exactly floor of the
    // mathematical cumulative issuance while preserving the hard cap.
    let remaining_numerator = u128::from(MAX_SUPPLY_ATOMS)
        .checked_mul(
            ECONOMIC_YEAR_NS
                .checked_mul(2)
                .and_then(|twice_year| twice_year.checked_sub(within_year_ns))
                .ok_or(MonetaryV3Error::ArithmeticOverflow)?,
        )
        .ok_or(MonetaryV3Error::ArithmeticOverflow)?;
    let shift = u32::try_from(epoch + 1).map_err(|_| MonetaryV3Error::ArithmeticOverflow)?;
    let power_of_two = 1u128
        .checked_shl(shift)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)?;
    let remaining_denominator = power_of_two
        .checked_mul(ECONOMIC_YEAR_NS)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)?;
    let remaining_atoms = ceil_div_u128(remaining_numerator, remaining_denominator);
    let remaining_atoms = u64::try_from(remaining_atoms)
        .map_err(|_| MonetaryV3Error::ArithmeticOverflow)?;

    MAX_SUPPLY_ATOMS
        .checked_sub(remaining_atoms)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Subsidy assigned to one canonical reward-settlement position.
///
/// This is a cumulative-difference rule, so rounding remainders are distributed
/// deterministically across score positions and the sum can never exceed the
/// approved hard cap.
pub fn subsidy_atoms_for_score(
    score: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<u64, MonetaryV3Error> {
    if score == 0 {
        return Ok(0);
    }
    let previous_time = economic_time_ns_for_score(score - 1, segments)?;
    let current_time = economic_time_ns_for_score(score, segments)?;
    let previous_issuance = target_issuance_atoms(previous_time)?;
    let current_issuance = target_issuance_atoms(current_time)?;
    current_issuance
        .checked_sub(previous_issuance)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Economic-time portion of coinbase maturity. Consensus settlement must also
/// require the reward's ordered-DAG position to be final under the frozen v3
/// finality policy before the reward UTXO becomes spendable.
pub fn economic_maturity_reached(
    reward_score: u64,
    current_score: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<bool, MonetaryV3Error> {
    if current_score < reward_score {
        return Ok(false);
    }
    let reward_time = economic_time_ns_for_score(reward_score, segments)?;
    let current_time = economic_time_ns_for_score(current_score, segments)?;
    Ok(current_time.saturating_sub(reward_time) >= COINBASE_MATURITY_NS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BPS1: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];
    const BPS2: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 500_000_000,
    }];
    const BPS4: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 250_000_000,
    }];

    #[test]
    fn approved_supply_checkpoints_are_exact() {
        assert_eq!(target_issuance_atoms(0).unwrap(), 0);
        assert_eq!(
            target_issuance_atoms(ECONOMIC_YEAR_NS).unwrap(),
            YEAR1_MINING_BUDGET_ATOMS
        );
        assert_eq!(
            target_issuance_atoms(2 * ECONOMIC_YEAR_NS).unwrap(),
            75_000_000_000_000_000
        );
        assert_eq!(
            target_issuance_atoms(10 * ECONOMIC_YEAR_NS).unwrap(),
            99_902_343_750_000_000
        );
    }

    #[test]
    fn one_two_and_four_bps_have_identical_one_second_issuance() {
        let one_second_1bps = target_issuance_atoms(economic_time_ns_for_score(1, &BPS1).unwrap())
            .unwrap();
        let one_second_2bps = target_issuance_atoms(economic_time_ns_for_score(2, &BPS2).unwrap())
            .unwrap();
        let one_second_4bps = target_issuance_atoms(economic_time_ns_for_score(4, &BPS4).unwrap())
            .unwrap();
        assert_eq!(one_second_1bps, 1_585_489_599);
        assert_eq!(one_second_1bps, one_second_2bps);
        assert_eq!(one_second_1bps, one_second_4bps);

        let four_rewards = (1..=4)
            .map(|score| subsidy_atoms_for_score(score, &BPS4).unwrap())
            .sum::<u64>();
        assert_eq!(four_rewards, one_second_1bps);
    }

    #[test]
    fn cadence_activation_is_continuous_and_does_not_reprice_history() {
        let schedule = [
            MonetaryCadenceSegment {
                activation_score: 0,
                target_interval_ns: 1_000_000_000,
            },
            MonetaryCadenceSegment {
                activation_score: 10,
                target_interval_ns: 500_000_000,
            },
            MonetaryCadenceSegment {
                activation_score: 20,
                target_interval_ns: 250_000_000,
            },
        ];
        assert_eq!(economic_time_ns_for_score(10, &schedule).unwrap(), 10_000_000_000);
        assert_eq!(economic_time_ns_for_score(20, &schedule).unwrap(), 15_000_000_000);
        assert_eq!(economic_time_ns_for_score(24, &schedule).unwrap(), 16_000_000_000);
    }

    #[test]
    fn year_boundary_and_terminal_residual_are_exact() {
        let year_score = ECONOMIC_YEAR_SECONDS;
        assert_eq!(
            economic_time_ns_for_score(year_score, &BPS1).unwrap(),
            ECONOMIC_YEAR_NS
        );
        assert_eq!(
            target_issuance_atoms(57 * ECONOMIC_YEAR_NS - 1).unwrap(),
            MAX_SUPPLY_ATOMS - 1
        );
        assert_eq!(
            target_issuance_atoms(57 * ECONOMIC_YEAR_NS).unwrap(),
            MAX_SUPPLY_ATOMS
        );
        assert_eq!(
            target_issuance_atoms(100 * ECONOMIC_YEAR_NS).unwrap(),
            MAX_SUPPLY_ATOMS
        );
    }

    #[test]
    fn maturity_uses_economic_time_not_raw_score_count() {
        let one_hour_at_1bps = COINBASE_MATURITY_SECONDS;
        let one_hour_at_2bps = COINBASE_MATURITY_SECONDS * 2;
        let one_hour_at_4bps = COINBASE_MATURITY_SECONDS * 4;
        assert!(economic_maturity_reached(1, 1 + one_hour_at_1bps, &BPS1).unwrap());
        assert!(economic_maturity_reached(1, 1 + one_hour_at_2bps, &BPS2).unwrap());
        assert!(economic_maturity_reached(1, 1 + one_hour_at_4bps, &BPS4).unwrap());
        assert!(!economic_maturity_reached(1, one_hour_at_1bps, &BPS1).unwrap());
    }

    #[test]
    fn malformed_cadence_schedules_fail_closed() {
        assert_eq!(
            economic_time_ns_for_score(1, &[]),
            Err(MonetaryV3Error::EmptyCadenceSchedule)
        );
        assert_eq!(
            economic_time_ns_for_score(
                1,
                &[MonetaryCadenceSegment {
                    activation_score: 1,
                    target_interval_ns: 1_000_000_000,
                }]
            ),
            Err(MonetaryV3Error::FirstCadenceMustStartAtZero)
        );
    }
}
