use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Frozen v3.0.0 smooth-emission monetary-policy version.
pub const MONETARY_POLICY_VERSION_V3: &str = "pulsedag-monetary-v3.0.0-smooth-v1";
pub const MONETARY_CADENCE_FINGERPRINT_DOMAIN_V3: &[u8] = b"PulseDAG:monetary-cadence:v3.0.0";

/// PulseDAG v3.0.0 monetary constants.
///
/// Supply is fair-launch and bounded: no spendable genesis issuance, premine,
/// protocol treasury, consensus burn, or permanent tail emission.
pub const MAX_SUPPLY_ATOMS: u64 = 100_000_000_000_000_000;
pub const GENESIS_ISSUANCE_ATOMS: u64 = 0;
pub const ATOMS_PER_COIN: u64 = 100_000_000;

pub const ECONOMIC_YEAR_SECONDS: u64 = 31_536_000;
pub const ECONOMIC_YEAR_NS: u128 = ECONOMIC_YEAR_SECONDS as u128 * 1_000_000_000;

/// Smooth emission uses a three-economic-year half-life evaluated at six-hour
/// fixed-point quanta. The Q64 factor is the frozen integer approximation of
/// 2^(-1/4380), because three 365-day economic years contain 4380 six-hour quanta.
pub const HALF_LIFE_SECONDS: u64 = 3 * ECONOMIC_YEAR_SECONDS;
pub const EMISSION_QUANTUM_SECONDS: u64 = 21_600;
pub const EMISSION_QUANTUM_NS: u128 = EMISSION_QUANTUM_SECONDS as u128 * 1_000_000_000;
pub const HALF_LIFE_QUANTA: u64 = HALF_LIFE_SECONDS / EMISSION_QUANTUM_SECONDS;
pub const Q64_ONE: u128 = 1_u128 << 64;
pub const DECAY_FACTOR_Q64: u128 = 18_443_825_056_137_834_748;

/// Exact year-one cumulative issuance produced by the frozen Q64 curve.
/// This is a golden-vector constant, not an independent annual budget.
pub const YEAR1_TARGET_ISSUANCE_ATOMS: u64 = 20_629_947_401_590_029;

/// Q64 truncation leaves exactly one atom immediately before this quantum and
/// zero remaining atoms at this quantum. v3 settles that final atom here and
/// permanently switches subsidy to zero, so there is no perpetual tail.
pub const TERMINAL_EMISSION_QUANTUM: u64 = 247_333;
pub const TERMINAL_EMISSION_SECONDS: u64 =
    TERMINAL_EMISSION_QUANTUM * EMISSION_QUANTUM_SECONDS;

pub const COINBASE_MATURITY_SECONDS: u64 = 3_600;
pub const COINBASE_MATURITY_NS: u128 = COINBASE_MATURITY_SECONDS as u128 * 1_000_000_000;
pub const ORDINARY_FEE_RECIPIENT_BPS: u16 = 10_000;
pub const CONSENSUS_BURN_BPS: u16 = 0;
pub const TAIL_EMISSION_ATOMS: u64 = 0;

/// Canonical policy bytes. This digest binds economic semantics only; cadence,
/// protocol, finality, source, network and genesis identities are bound
/// separately by their own fingerprints/evidence.
pub const MONETARY_POLICY_CANONICAL_V3: &[u8] = br#"PulseDAG:monetary-policy:v3.0.0-smooth-v1
max_supply_atoms=100000000000000000
atoms_per_coin=100000000
genesis_issuance_atoms=0
economic_year_seconds=31536000
half_life_seconds=94608000
emission_quantum_seconds=21600
decay_factor_q64=18443825056137834748
terminal_emission_quantum=247333
coinbase_maturity_seconds=3600
ordinary_fee_recipient_bps=10000
consensus_burn_bps=0
tail_emission_atoms=0
score_basis=ordered-dag-ordinal-v1
emission_model=q64-exponential-decay-linear-within-quantum-v1
programmable_resource_fees=inactive-unreachable-v3.0.0
"#;

pub const MONETARY_POLICY_FINGERPRINT_V3: &str =
    "1c1cdf61e46a59418d10315dd0fa705d7e7fd4dbe4d1bb0f855a808148838e36";

pub fn monetary_policy_fingerprint_v3() -> String {
    hex::encode(Sha256::digest(MONETARY_POLICY_CANONICAL_V3))
}

/// A consensus cadence segment maps canonical monetary-score steps to economic
/// time. activation_score is the score at which this interval becomes active
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
        MONETARY_CADENCE_FINGERPRINT_DOMAIN_V3.len() + 4 + segments.len().saturating_mul(16),
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

fn q64_mul(a: u128, b: u128) -> Result<u128, MonetaryV3Error> {
    debug_assert!(a <= Q64_ONE);
    debug_assert!(b <= Q64_ONE);
    a.checked_mul(b)
        .map(|product| product >> 64)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

fn q64_pow(mut exponent: u64) -> Result<u128, MonetaryV3Error> {
    let mut base = DECAY_FACTOR_Q64;
    let mut result = Q64_ONE;

    while exponent != 0 {
        if exponent & 1 == 1 {
            result = q64_mul(result, base)?;
        }
        exponent >>= 1;
        if exponent != 0 {
            base = q64_mul(base, base)?;
        }
    }
    Ok(result)
}

/// Cumulative scheduled issuance at an exact six-hour economic quantum.
fn target_issuance_atoms_at_quantum(quantum: u64) -> Result<u64, MonetaryV3Error> {
    if quantum >= TERMINAL_EMISSION_QUANTUM {
        return Ok(MAX_SUPPLY_ATOMS);
    }

    let remaining_multiplier = q64_pow(quantum)?;
    let remaining_atoms = u128::from(MAX_SUPPLY_ATOMS)
        .checked_mul(remaining_multiplier)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)?
        >> 64;
    let remaining_atoms =
        u64::try_from(remaining_atoms).map_err(|_| MonetaryV3Error::ArithmeticOverflow)?;

    MAX_SUPPLY_ATOMS
        .checked_sub(remaining_atoms)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)
}

/// Exact cumulative scheduled mining issuance by economic time.
///
/// The curve is exponential at six-hour Q64 checkpoints and interpolated
/// linearly with integer arithmetic inside each checkpoint interval. This keeps
/// issuance smooth at block cadence while avoiding floating point, runtime
/// logarithms/powers, raw-height dependence, or BPS-dependent gross supply.
///
/// At the explicit terminal quantum the final residual atom is settled and
/// issuance remains exactly MAX_SUPPLY_ATOMS forever.
pub fn target_issuance_atoms(economic_time_ns: u128) -> Result<u64, MonetaryV3Error> {
    let quantum = economic_time_ns / EMISSION_QUANTUM_NS;
    if quantum >= u128::from(TERMINAL_EMISSION_QUANTUM) {
        return Ok(MAX_SUPPLY_ATOMS);
    }

    let quantum = u64::try_from(quantum).map_err(|_| MonetaryV3Error::ArithmeticOverflow)?;
    let within_quantum_ns = economic_time_ns % EMISSION_QUANTUM_NS;
    let start = target_issuance_atoms_at_quantum(quantum)?;
    if within_quantum_ns == 0 {
        return Ok(start);
    }

    let end = target_issuance_atoms_at_quantum(quantum.saturating_add(1))?;
    let delta = u128::from(
        end.checked_sub(start)
            .ok_or(MonetaryV3Error::ArithmeticOverflow)?,
    );
    let interpolated = delta
        .checked_mul(within_quantum_ns)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)?
        / EMISSION_QUANTUM_NS;
    let total = u128::from(start)
        .checked_add(interpolated)
        .ok_or(MonetaryV3Error::ArithmeticOverflow)?;

    u64::try_from(total).map_err(|_| MonetaryV3Error::ArithmeticOverflow)
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
        assert_eq!(
            monetary_policy_fingerprint_v3(),
            MONETARY_POLICY_FINGERPRINT_V3
        );
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
    fn smooth_supply_checkpoints_are_exact() {
        assert_eq!(target_issuance_atoms(0).unwrap(), GENESIS_ISSUANCE_ATOMS);
        assert_eq!(
            target_issuance_atoms(ECONOMIC_YEAR_NS).unwrap(),
            YEAR1_TARGET_ISSUANCE_ATOMS
        );
        assert_eq!(
            target_issuance_atoms(2 * ECONOMIC_YEAR_NS).unwrap(),
            37_003_947_505_256_347
        );
        assert_eq!(
            target_issuance_atoms(3 * ECONOMIC_YEAR_NS).unwrap(),
            50_000_000_000_000_006
        );
        assert_eq!(
            target_issuance_atoms(6 * ECONOMIC_YEAR_NS).unwrap(),
            75_000_000_000_000_006
        );
        assert_eq!(
            target_issuance_atoms(10 * ECONOMIC_YEAR_NS).unwrap(),
            90_078_743_425_198_757
        );
        assert_eq!(
            target_issuance_atoms(20 * ECONOMIC_YEAR_NS).unwrap(),
            99_015_686_679_769_632
        );
        assert_eq!(
            target_issuance_atoms(30 * ECONOMIC_YEAR_NS).unwrap(),
            99_902_343_750_000_001
        );
    }

    #[test]
    fn three_year_half_life_is_atomic_precision_bounded() {
        let at_half_life = target_issuance_atoms(u128::from(HALF_LIFE_SECONDS) * 1_000_000_000)
            .unwrap();
        let exact_half = MAX_SUPPLY_ATOMS / 2;
        assert!(at_half_life.abs_diff(exact_half) <= 10);
        assert_eq!(HALF_LIFE_QUANTA, 4_380);
    }

    #[test]
    fn first_quantum_vector_is_stable() {
        assert_eq!(
            target_issuance_atoms(1_000_000_000).unwrap(),
            732_593_794
        );
        assert_eq!(
            target_issuance_atoms(EMISSION_QUANTUM_NS).unwrap(),
            15_824_025_963_894
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
        assert_eq!(one_second_1bps, 732_593_794);
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
        assert_eq!(authorized_issuance_atoms(0, end, &BPS1).unwrap(), total);
        assert_eq!(
            (1..=end)
                .map(|score| subsidy_atoms_for_score(score, &BPS1).unwrap())
                .sum::<u64>(),
            total
        );
    }

    #[test]
    fn terminal_residual_atom_is_exact_and_no_tail_exists() {
        let terminal_ns = u128::from(TERMINAL_EMISSION_SECONDS) * 1_000_000_000;
        assert_eq!(
            target_issuance_atoms(terminal_ns - 1).unwrap(),
            MAX_SUPPLY_ATOMS - 1
        );
        assert_eq!(target_issuance_atoms(terminal_ns).unwrap(), MAX_SUPPLY_ATOMS);
        assert_eq!(
            target_issuance_atoms(terminal_ns + 100 * ECONOMIC_YEAR_NS).unwrap(),
            MAX_SUPPLY_ATOMS
        );
        assert_eq!(TAIL_EMISSION_ATOMS, 0);
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
