use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::monetary_v3::ATOMS_PER_COIN;

pub const PDG_SYMBOL_V3: &str = "PDG";
pub const PDG_DECIMALS_V3: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DenominationV3Error {
    #[error("PDG amount must not be empty")]
    Empty,
    #[error("PDG amount must use plain base-10 notation without surrounding whitespace")]
    InvalidSyntax,
    #[error("PDG amount supports at most 8 decimal places")]
    TooManyDecimalPlaces,
    #[error("PDG amount overflows the u64 atomic representation")]
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdgAmountV3 {
    pub atoms: u64,
    pub pdg: String,
}

impl PdgAmountV3 {
    pub fn from_atoms(atoms: u64) -> Self {
        Self {
            atoms,
            pdg: format_pdg_atoms_v3(atoms),
        }
    }

    pub fn parse(value: &str) -> Result<Self, DenominationV3Error> {
        let atoms = parse_pdg_decimal_v3(value)?;
        Ok(Self::from_atoms(atoms))
    }
}

/// Canonical human-readable PDG representation.
///
/// Consensus, storage, wallet intents and RPC amount fields remain integer
/// atoms. This formatter is presentation-only and always emits exactly eight
/// fractional decimal digits without floating-point arithmetic.
pub fn format_pdg_atoms_v3(atoms: u64) -> String {
    let whole = atoms / ATOMS_PER_COIN;
    let fraction = atoms % ATOMS_PER_COIN;
    format!("{whole}.{fraction:08}")
}

/// Parse a plain decimal PDG amount into consensus atoms without floating point.
///
/// Accepted examples include `"0"`, `"1"`, `"1.5"`, and
/// `"1.00000001"`. Signs, exponent notation, surrounding whitespace, empty
/// fractional components and more than eight decimals fail closed.
pub fn parse_pdg_decimal_v3(value: &str) -> Result<u64, DenominationV3Error> {
    if value.is_empty() {
        return Err(DenominationV3Error::Empty);
    }
    if value.trim() != value
        || value.starts_with('+')
        || value.starts_with('-')
        || value.contains('e')
        || value.contains('E')
    {
        return Err(DenominationV3Error::InvalidSyntax);
    }

    let mut parts = value.split('.');
    let whole = parts.next().ok_or(DenominationV3Error::InvalidSyntax)?;
    let fraction = parts.next();
    if parts.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DenominationV3Error::InvalidSyntax);
    }

    let whole_value = whole
        .parse::<u64>()
        .map_err(|_| DenominationV3Error::Overflow)?;
    let whole_atoms = whole_value
        .checked_mul(ATOMS_PER_COIN)
        .ok_or(DenominationV3Error::Overflow)?;

    let fraction_atoms = match fraction {
        None => 0,
        Some(fraction) => {
            if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
                return Err(DenominationV3Error::InvalidSyntax);
            }
            if fraction.len() > PDG_DECIMALS_V3 as usize {
                return Err(DenominationV3Error::TooManyDecimalPlaces);
            }
            let raw = fraction
                .parse::<u64>()
                .map_err(|_| DenominationV3Error::Overflow)?;
            let padding = PDG_DECIMALS_V3 as usize - fraction.len();
            let scale = 10u64
                .checked_pow(u32::try_from(padding).map_err(|_| DenominationV3Error::Overflow)?)
                .ok_or(DenominationV3Error::Overflow)?;
            raw.checked_mul(scale)
                .ok_or(DenominationV3Error::Overflow)?
        }
    };

    whole_atoms
        .checked_add(fraction_atoms)
        .ok_or(DenominationV3Error::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monetary_v3::MAX_SUPPLY_ATOMS;

    #[test]
    fn canonical_display_has_exactly_eight_decimals() {
        assert_eq!(format_pdg_atoms_v3(0), "0.00000000");
        assert_eq!(format_pdg_atoms_v3(1), "0.00000001");
        assert_eq!(format_pdg_atoms_v3(ATOMS_PER_COIN), "1.00000000");
        assert_eq!(format_pdg_atoms_v3(MAX_SUPPLY_ATOMS), "1000000000.00000000");
    }

    #[test]
    fn exact_decimal_parser_never_uses_float_rounding() {
        assert_eq!(parse_pdg_decimal_v3("1").unwrap(), ATOMS_PER_COIN);
        assert_eq!(parse_pdg_decimal_v3("1.5").unwrap(), 150_000_000);
        assert_eq!(parse_pdg_decimal_v3("0.00000001").unwrap(), 1);
        assert_eq!(
            parse_pdg_decimal_v3("1000000000.00000000").unwrap(),
            MAX_SUPPLY_ATOMS
        );
    }

    #[test]
    fn display_roundtrips_atomic_values() {
        for atoms in [0, 1, 99_999_999, ATOMS_PER_COIN, MAX_SUPPLY_ATOMS, u64::MAX] {
            let display = format_pdg_atoms_v3(atoms);
            assert_eq!(parse_pdg_decimal_v3(&display).unwrap(), atoms);
        }
    }

    #[test]
    fn noncanonical_or_overprecise_input_fails_closed() {
        for value in [
            "", " 1", "1 ", "+1", "-1", ".1", "1.", "1e3", "1E3", "1.2.3",
        ] {
            assert!(parse_pdg_decimal_v3(value).is_err(), "{value}");
        }
        assert_eq!(
            parse_pdg_decimal_v3("1.000000001"),
            Err(DenominationV3Error::TooManyDecimalPlaces)
        );
    }
}
