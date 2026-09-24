use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Frozen v3.0.0 mainnet monetary-policy version.
pub const MONETARY_POLICY_VERSION_V3: &str = "pulsedag-monetary-v3.0.0";
pub const MONETARY_CADENCE_FINGERPRINT_DOMAIN_V3: &[u8] =
    b"PulseDAG:monetary-cadence:v3.0.0";

/// Approved v3.0.0 mainnet monetary constants.
pub const MAX_SUPPLY_ATOMS: u64 = 100_000_000_000_000_000;
pub const YEAR1_MINING_BUDGET_ATOMS: u64 = 50_000_000_000_000_000;
pub const GENESIS_ISSUANCE_ATOMS: u64 = 0;
pub const ATOMS_PER_COIN: u64 = 100_000_000;
pub const ECONOMIC_YEAR_SECONDS: u64 = 31_536_000;
pub const ECONOMIC_YEAR_NS: u128 = ECONOMIC_YEAR_SECONDS as u128 * 1_000_000_000;
pub const COINBASE_MATURITY_SECONDS: u64 = 3_600;
pub const COINBASE_MATURITY_NS: u128 = COINBASE_MATURITY_SECONDS as u128 * 1_000_000_000;
pub const ORDINARY_FEE_RECIPIENT_BPS: u16 = 10_000;
pub const CONSENSUS_BURN_BPS: u16 = 0;
pub const TAIL_EMISSION_ATOMS: u64 = 0;

/// At the start of economic year 57 the remaining geometric amount is below
/// one atomic unit. v3 settles the final residual atom at this boundary, reaches
/// MAX_SUPPLY_ATOMS exactly, and permanently switches subsidy to zero.
pub const TERMINAL_ECONOMIC_YEAR: u128 = 57;

/// Canonical policy bytes. This is intentionally independent of release source
/// SHA and network/genesis identity; those are bound separately by launch
/// evidence. Programmable resource fees are explicitly unreachable while smart
/// contracts remain inactive on v3.0.0 mainnet.
pub const MONETARY_POLICY_CANONICAL_V3: &[u8] = b"PulseDAG:monetary-policy:v3.0.0\n\
max_supply_atoms=100000000000000000\n\
atoms_per_coin=100000000\n\
genesis_issuance_atoms=0\n\
year1_mining_budget_atoms=50000000000000000\n\
economic_year_seconds=31536000\n\
coinbase_maturity_seconds=3600\n\
ordinary_fee_recipient_bps=10000\n\
consensus_burn_bps=0\n\
tail_emission_atoms=0\n\
terminal_economic_year=57\n\
score_basis=ordered-dag-ordinal-v1\n\
programmable_resource_fees=inactive-unreachable-v3.0.0\n";

pub const MONETARY_POLICY_FINGERPRINT_V3: &str =
    "14605483aa65a17d654ffc4db1571b1416eb45b3f9b56af452d88c9023311366";

pub fn monetary_policy_fingerprint_v3() -> String {
    hex::encode(Sha256::digest(MONETARY_POLICY_CANONICAL_V3))
}

/// A consensus cadence segment maps canonical monetary-score steps to economic
/// time. `activation_score` is the score at which this interval becomes active
/// for the next score transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonetaryCadenceSegment {
    pub activation_score: u64,
    pub target_interval_ns: u64,
}

pub fn canonical_monetary_cadence_bytes_v3(
    segments: &[MonetaryCadenceSegment],
) -> Result<Vec<u8>, MonetaryV3Error> {
    validate_cadence_segments(segments)?;
    let count = u32::try_from(segments.len()).map_err(|_| MonetaryV3Error::ArithmeticOverflow)?;
    let mut out = Vec::with_capacity(
        MONETARY_CADENCE_FINGERPRINT_DOMAIN_V3.len()
            + 4
            + segments.len().saturating_mul(16),
    );
    out.extend_from_slice(MONETARY_CADENCE_FINGERPRINT_DOMAIN_V3);
    out.extend_from_slice(&count.to_le_bytes());
    for segment in segments {
        out.extend_from_slice(&segment.activation_score.to_le_bytes());
        out.extend_from_slice(&segment.target_interval_ns.to_le_bytes());
    }
    Ok(out)
}

pub fn monetary_cadence_fingerprint_v3(
    segments: &[MonetaryCadenceSegment],
) -> Result<String, MonetaryV3Error> {
    Ok(hex::encode(Sha256::digest(
        canonical_monetary_cadence_bytes_v3(segments)?,
    )))
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
    #[error("monetary score range must be monotonic")]
    NonMonotonicScoreRange,
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

/// Convert canonical ordered-DAG monetary score into deterministic economic ns.
///
/// Score 0 is genesis. Score 1 is the first non-genesis canonical ordered
/// position. The cadence schedule is consensus-versioned, so cadence changes
/// alter reward granularity but not the emission curve.
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

/// Exact cumulative scheduled mining issuance by economic time.
///
/// The curve is linear within each economic year and halves its exact annual
/// budget every 365 economic days. Cumulative issuance rounds down to atomic
/// units; floating-point monetary arithmetic is forbidden.
pub fn target_issuance_atoms(economic_time_ns: u128) -> Result<u64, MonetaryV3Error> {
    let epoch = economic_time_ns / ECONOMIC_YEAR_NS;
    if epoch >= TERMINAL_ECONOMIC_YEAR {
        return Ok(MAX_SUPPLY_ATOMS);
    }

    let within_year_ns = economic_time_ns % ECONOMIC_YEAR_NS;
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
    let remaining_atoms =
        u64::try_from(remaining_atoms).map_err(|_| MonetaryV3Error::ArithmeticOverflow)?;

    MAX_SUPPLY_ATOMS
        .checked_sub(remaining_atoms)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Exact total scheduled supply at an arbitrary canonical accepted score.
pub fn total_supply_atoms_for_score(
    score: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<u64, MonetaryV3Error> {
    target_issuance_atoms(economic_time_ns_for_score(score, segments)?)
}

/// Subsidy assigned to one canonical ordered-DAG monetary position.
pub fn subsidy_atoms_for_score(
    score: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<u64, MonetaryV3Error> {
    if score == 0 {
        return Ok(0);
    }
    let previous = total_supply_atoms_for_score(score - 1, segments)?;
    let current = total_supply_atoms_for_score(score, segments)?;
    current
        .checked_sub(previous)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Authorized issuance across a monotonic canonical score range.
pub fn authorized_issuance_atoms(
    from_score: u64,
    to_score: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<u64, MonetaryV3Error> {
    if to_score < from_score {
        return Err(MonetaryV3Error::NonMonotonicScoreRange);
    }
    let from = total_supply_atoms_for_score(from_score, segments)?;
    let to = total_supply_atoms_for_score(to_score, segments)?;
    to.checked_sub(from)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Maximum coinbase claim for a canonical score. Fees are transfers, not new
/// issuance, and v3.0.0 applies no consensus burn.
pub fn max_coinbase_claim_atoms(
    score: u64,
    eligible_fees_atoms: u64,
    segments: &[MonetaryCadenceSegment],
) -> Result<u64, MonetaryV3Error> {
    subsidy_atoms_for_score(score, segments)?
        .checked_add(eligible_fees_atoms)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Economic-time portion of coinbase maturity. Reward spendability must also
/// satisfy the separately frozen finality/settlement rule.
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
    fn policy_fingerprint_is_frozen() {
        assert_eq!(monetary_policy_fingerprint_v3(), MONETARY_POLICY_FINGERPRINT_V3);
    }

    #[test]
    fn cadence_fingerprint_is_frozen_for_one_bps_vector() {
        assert_eq!(
            monetary_cadence_fingerprint_v3(&BPS1).unwrap(),
            "1eac3d10a00fb226fba56f8d88079f78bdc4d18d27b0a81ac8972ede1788c626"
        );
    }

    #[test]
    fn cadence_fingerprint_changes_on_consensus_schedule_change() {
        assert_ne!(
            monetary_cadence_fingerprint_v3(&BPS1).unwrap(),
            monetary_cadence_fingerprint_v3(&BPS2).unwrap()
        );
        let staged = [
            MonetaryCadenceSegment {
                activation_score: 0,
                target_interval_ns: 1_000_000_000,
            },
            MonetaryCadenceSegment {
                activation_score: 10,
                target_interval_ns: 500_000_000,
            },
        ];
        assert_ne!(
            monetary_cadence_fingerprint_v3(&BPS1).unwrap(),
            monetary_cadence_fingerprint_v3(&staged).unwrap()
        );
    }

    #[test]
    fn approved_supply_checkpoints_are_exact() {
        assert_eq!(target_issuance_atoms(0).unwrap(), GENESIS_ISSUANCE_ATOMS);
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
        let one_second_1bps =
            target_issuance_atoms(economic_time_ns_for_score(1, &BPS1).unwrap()).unwrap();
        let one_second_2bps =
            target_issuance_atoms(economic_time_ns_for_score(2, &BPS2).unwrap()).unwrap();
        let one_second_4bps =
            target_issuance_atoms(economic_time_ns_for_score(4, &BPS4).unwrap()).unwrap();
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
        assert_eq!(
            economic_time_ns_for_score(10, &schedule).unwrap(),
            10_000_000_000
        );
        assert_eq!(
            economic_time_ns_for_score(20, &schedule).unwrap(),
            15_000_000_000
        );
        assert_eq!(
            economic_time_ns_for_score(24, &schedule).unwrap(),
            16_000_000_000
        );
    }

    #[test]
    fn authorized_issuance_telescopes_to_total_supply() {
        let end = 123_456;
        let total = total_supply_atoms_for_score(end, &BPS1).unwrap();
        assert_eq!(
            authorized_issuance_atoms(0, end, &BPS1).unwrap(),
            total
        );
        assert_eq!(
            (1..=end)
                .map(|score| subsidy_atoms_for_score(score, &BPS1).unwrap())
                .sum::<u64>(),
            total
        );
    }

    #[test]
    fn year_boundary_and_terminal_residual_are_exact() {
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
    fn malformed_inputs_fail_closed() {
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
        assert_eq!(
            authorized_issuance_atoms(2, 1, &BPS1),
            Err(MonetaryV3Error::NonMonotonicScoreRange)
        );
    }
}
