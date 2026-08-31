use sha2::{Digest, Sha256};

/// Canonical PulseDAG monetary policy identifier.
pub const MONETARY_POLICY_ID: &str = "pulsedag-monetary-v1";
pub const MONETARY_POLICY_FINGERPRINT_DOMAIN: &str = "PulseDAG:monetary-policy:v1";

pub const PDG_DECIMALS: u32 = 8;
pub const ATOMIC_UNITS_PER_PDG: u64 = 100_000_000;

/// Reference amount approached by the exponentially decaying main-emission phase.
/// This is not a hard cap because the permanent tail emission continues afterwards.
pub const MAIN_EMISSION_REFERENCE_ATOMIC: u128 = 100_000_000_000_000_000;

/// A monetary year is exactly 365.25 days.
pub const MONETARY_YEAR_SECS: u64 = 31_557_600;
pub const HALF_LIFE_SECS: u64 = 157_788_000;

/// The main emission uses a deterministic Q64 fixed-point decay evaluated at
/// six-hour monetary quanta, with integer linear interpolation inside a quantum.
pub const EMISSION_QUANTUM_SECS: u64 = 21_600;
pub const Q64_ONE: u128 = 1_u128 << 64;
pub const DECAY_FACTOR_Q64: u128 = 18_444_993_806_489_946_493;

/// Tail activation is aligned to an exact six-hour monetary quantum.
pub const TAIL_START_QUANTUM: u64 = 35_388;
pub const TAIL_START_SECS: u64 = 764_380_800;
pub const MAIN_SUPPLY_AT_TAIL_START_ATOMIC: u128 = 96_518_997_121_567_008;
pub const TAIL_ANNUAL_ATOMIC: u128 = 482_594_985_607_835;

/// Fair-launch monetary boundaries.
pub const GENESIS_ALLOCATION_ATOMIC: u128 = 0;
pub const BOOTSTRAP_ISSUANCE_ATOMIC: u128 = 0;
pub const TREASURY_BPS: u16 = 0;

/// Coinbase maturity is measured in consensus monetary time, not block count,
/// so changing block cadence cannot change the economic maturity horizon.
pub const COINBASE_MATURITY_SECS: u64 = 86_400;

/// Canonical fixed-parameter representation used for the monetary-policy digest.
pub const MONETARY_POLICY_CANONICAL_V1: &str = concat!(
    "PulseDAG:monetary-policy:v1\n",
    "symbol=PDG\n",
    "decimals=8\n",
    "atomic_units_per_pdg=100000000\n",
    "main_emission_reference_atomic=100000000000000000\n",
    "monetary_year_secs=31557600\n",
    "half_life_secs=157788000\n",
    "emission_quantum_secs=21600\n",
    "decay_factor_q64=18444993806489946493\n",
    "tail_start_quantum=35388\n",
    "tail_start_secs=764380800\n",
    "main_supply_at_tail_start_atomic=96518997121567008\n",
    "tail_annual_atomic=482594985607835\n",
    "genesis_allocation_atomic=0\n",
    "treasury_bps=0\n",
    "bootstrap_issuance_atomic=0\n",
    "coinbase_maturity_secs=86400\n",
    "fee_disposition=miner_100pct_no_burn_v1\n",
    "activation_clock=consensus_monetary_time_since_public_activation\n",
    "block_rate_dependency=none\n",
);

pub const MONETARY_POLICY_FINGERPRINT_V1: &str =
    "70fafa18d010c4a3f99653ced2103f54b70030ae4471c61072e3b4db90d41503";

fn q64_mul(a: u128, b: u128) -> u128 {
    debug_assert!(a <= Q64_ONE);
    debug_assert!(b <= Q64_ONE);
    (a * b) >> 64
}

fn q64_pow(mut base: u128, mut exponent: u64) -> u128 {
    let mut result = Q64_ONE;
    while exponent != 0 {
        if exponent & 1 == 1 {
            result = q64_mul(result, base);
        }
        exponent >>= 1;
        if exponent != 0 {
            base = q64_mul(base, base);
        }
    }
    result
}

/// Cumulative main-phase supply at an exact six-hour monetary quantum.
///
/// The function is integer-only and independent of wall-clock floating point,
/// block height, raw block count, or configured blocks-per-second.
pub fn main_supply_at_quantum(quantum: u64) -> u128 {
    let quantum = quantum.min(TAIL_START_QUANTUM);
    let remaining_multiplier = q64_pow(DECAY_FACTOR_Q64, quantum);
    let remaining = (MAIN_EMISSION_REFERENCE_ATOMIC * remaining_multiplier) >> 64;
    MAIN_EMISSION_REFERENCE_ATOMIC - remaining
}

/// Authorized cumulative PDG supply, in atomic units, measured from public
/// permissionless monetary activation.
///
/// Private/bootstrap blocks before activation authorize exactly zero issuance.
/// Callers must pass elapsed consensus monetary time since the frozen public
/// activation boundary, never a raw unbounded block timestamp.
pub fn authorized_supply_since_public_activation(elapsed_secs: u64) -> u128 {
    if elapsed_secs >= TAIL_START_SECS {
        let tail_elapsed = u128::from(elapsed_secs - TAIL_START_SECS);
        let tail_minted = TAIL_ANNUAL_ATOMIC * tail_elapsed / u128::from(MONETARY_YEAR_SECS);
        return MAIN_SUPPLY_AT_TAIL_START_ATOMIC + tail_minted;
    }

    let quantum = elapsed_secs / EMISSION_QUANTUM_SECS;
    let within_quantum = elapsed_secs % EMISSION_QUANTUM_SECS;
    let start = main_supply_at_quantum(quantum);
    if within_quantum == 0 {
        return start;
    }
    let end = main_supply_at_quantum(quantum + 1);
    start + (end - start) * u128::from(within_quantum) / u128::from(EMISSION_QUANTUM_SECS)
}

/// Authorized gross issuance between two consensus monetary times.
pub fn authorized_issuance_between(start_secs: u64, end_secs: u64) -> Option<u128> {
    if end_secs < start_secs {
        return None;
    }
    Some(
        authorized_supply_since_public_activation(end_secs)
            - authorized_supply_since_public_activation(start_secs),
    )
}

/// Stable SHA-256 digest of the fixed monetary-policy parameters.
pub fn monetary_policy_fingerprint_v1() -> String {
    hex::encode(Sha256::digest(MONETARY_POLICY_CANONICAL_V1.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_fingerprint_is_frozen() {
        assert_eq!(
            monetary_policy_fingerprint_v1(),
            MONETARY_POLICY_FINGERPRINT_V1
        );
    }

    #[test]
    fn fair_launch_boundaries_are_zero() {
        assert_eq!(GENESIS_ALLOCATION_ATOMIC, 0);
        assert_eq!(BOOTSTRAP_ISSUANCE_ATOMIC, 0);
        assert_eq!(TREASURY_BPS, 0);
        assert_eq!(authorized_supply_since_public_activation(0), 0);
    }

    #[test]
    fn main_emission_golden_vectors_are_stable() {
        assert_eq!(
            authorized_supply_since_public_activation(MONETARY_YEAR_SECS),
            12_944_943_670_387_591
        );
        assert_eq!(
            authorized_supply_since_public_activation(5 * MONETARY_YEAR_SECS),
            50_000_000_000_000_012
        );
        assert_eq!(
            authorized_supply_since_public_activation(10 * MONETARY_YEAR_SECS),
            75_000_000_000_000_012
        );
        assert_eq!(
            authorized_supply_since_public_activation(20 * MONETARY_YEAR_SECS),
            93_750_000_000_000_006
        );
        assert_eq!(
            authorized_supply_since_public_activation(TAIL_START_SECS),
            MAIN_SUPPLY_AT_TAIL_START_ATOMIC
        );
    }

    #[test]
    fn tail_emission_golden_vectors_are_stable() {
        assert_eq!(
            authorized_supply_since_public_activation(30 * MONETARY_YEAR_SECS),
            99_307_543_917_255_812
        );
        assert_eq!(
            authorized_supply_since_public_activation(50 * MONETARY_YEAR_SECS),
            108_959_443_629_412_512
        );
        assert_eq!(
            authorized_supply_since_public_activation(100 * MONETARY_YEAR_SECS),
            133_089_192_909_804_262
        );
    }

    #[test]
    fn issuance_is_monotonic_and_block_rate_independent() {
        let a = authorized_supply_since_public_activation(123_456);
        let b = authorized_supply_since_public_activation(123_457);
        assert!(b >= a);
        assert_eq!(authorized_issuance_between(123_456, 123_457), Some(b - a));
        assert_eq!(authorized_issuance_between(10, 9), None);
    }
}
