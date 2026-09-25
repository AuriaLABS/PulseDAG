//! Planning matcher for `colored_utxo_v1`.
//!
//! One color per UTXO. Units are conserved per `color_id` unless an explicit
//! burn is declared. Default evaluation is fail-closed.

use sha2::{Digest, Sha256};

pub const COLORED_TEMPLATE_ID_V1: &str = "colored_utxo_v1";
pub const COLORED_DOMAIN_V1: &str = "PulseDAG:covenant:colored:v1";
pub const COLOR_ID_DOMAIN_V1: &[u8] = b"PulseDAG:color:v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColoredV1Output {
    pub template_id: String,
    pub color_id: [u8; 32],
    pub units: u64,
    pub owner_pk: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColoredSpendPathV1 {
    Mint,
    Transfer,
    Burn,
}

impl ColoredSpendPathV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mint => "mint",
            Self::Transfer => "transfer",
            Self::Burn => "burn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColoredAdmissionV1 {
    pub covenants_enabled: bool,
    pub template_admitted: bool,
}

impl ColoredAdmissionV1 {
    pub const INACTIVE: Self = Self {
        covenants_enabled: false,
        template_admitted: false,
    };

    pub fn admitted(self) -> bool {
        self.covenants_enabled && self.template_admitted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColoredV1Error {
    TemplateDisabled,
    UnknownTemplateId {
        observed: String,
    },
    EmptyOwnerKey,
    ColorIdMismatch,
    ImplicitMintForbidden,
    UnitsNotConserved {
        color_id: [u8; 32],
        inputs: u64,
        outputs: u64,
        burned: u64,
    },
    BurnMustBeDeclared,
    BurnPathRequiresBurnedUnits,
}

impl std::fmt::Display for ColoredV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "colored_utxo_v1 is not admitted; spend fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(
                    f,
                    "unknown covenant template {observed}; not colored_utxo_v1"
                )
            }
            Self::EmptyOwnerKey => write!(f, "owner_pk must not be empty"),
            Self::ColorIdMismatch => {
                write!(f, "output color_id does not match genesis derivation")
            }
            Self::ImplicitMintForbidden => {
                write!(f, "transfer/burn cannot introduce a new color_id")
            }
            Self::UnitsNotConserved {
                inputs,
                outputs,
                burned,
                ..
            } => write!(
                f,
                "color units not conserved: inputs {inputs} outputs {outputs} burned {burned}"
            ),
            Self::BurnMustBeDeclared => {
                write!(
                    f,
                    "destroying units requires the burn path and burned_units"
                )
            }
            Self::BurnPathRequiresBurnedUnits => {
                write!(f, "burn path must declare burned_units > 0")
            }
        }
    }
}

impl std::error::Error for ColoredV1Error {}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

/// `SHA-256(PulseDAG:color:v1 || chain_id || genesis_txid || genesis_index)`.
pub fn derive_color_id_v1(chain_id: &str, genesis_txid: &str, genesis_index: u32) -> [u8; 32] {
    let mut out = Vec::new();
    encode_len_prefixed(&mut out, COLOR_ID_DOMAIN_V1);
    encode_len_prefixed(&mut out, chain_id.as_bytes());
    encode_len_prefixed(&mut out, genesis_txid.as_bytes());
    out.extend_from_slice(&genesis_index.to_le_bytes());
    Sha256::digest(&out).into()
}

pub fn validate_colored_output_v1(output: &ColoredV1Output) -> Result<(), ColoredV1Error> {
    if output.template_id != COLORED_TEMPLATE_ID_V1 {
        return Err(ColoredV1Error::UnknownTemplateId {
            observed: output.template_id.clone(),
        });
    }
    if output.owner_pk.is_empty() {
        return Err(ColoredV1Error::EmptyOwnerKey);
    }
    Ok(())
}

pub fn evaluate_colored_mint_v1(
    admission: ColoredAdmissionV1,
    output: &ColoredV1Output,
    chain_id: &str,
    genesis_txid: &str,
    genesis_index: u32,
) -> Result<(), ColoredV1Error> {
    if !admission.admitted() {
        return Err(ColoredV1Error::TemplateDisabled);
    }
    validate_colored_output_v1(output)?;
    let expected = derive_color_id_v1(chain_id, genesis_txid, genesis_index);
    if output.color_id != expected {
        return Err(ColoredV1Error::ColorIdMismatch);
    }
    Ok(())
}

pub fn evaluate_colored_units_v1(
    admission: ColoredAdmissionV1,
    path: ColoredSpendPathV1,
    color_id: [u8; 32],
    input_units: u64,
    output_units: u64,
    burned_units: u64,
) -> Result<(), ColoredV1Error> {
    if !admission.admitted() {
        return Err(ColoredV1Error::TemplateDisabled);
    }
    match path {
        ColoredSpendPathV1::Mint => {
            if input_units != 0 {
                return Err(ColoredV1Error::UnitsNotConserved {
                    color_id,
                    inputs: input_units,
                    outputs: output_units,
                    burned: burned_units,
                });
            }
            if burned_units != 0 {
                return Err(ColoredV1Error::UnitsNotConserved {
                    color_id,
                    inputs: input_units,
                    outputs: output_units,
                    burned: burned_units,
                });
            }
            Ok(())
        }
        ColoredSpendPathV1::Transfer => {
            if input_units == 0 {
                return Err(ColoredV1Error::ImplicitMintForbidden);
            }
            if burned_units != 0 {
                return Err(ColoredV1Error::BurnMustBeDeclared);
            }
            if input_units != output_units {
                return Err(ColoredV1Error::UnitsNotConserved {
                    color_id,
                    inputs: input_units,
                    outputs: output_units,
                    burned: 0,
                });
            }
            Ok(())
        }
        ColoredSpendPathV1::Burn => {
            if input_units == 0 {
                return Err(ColoredV1Error::ImplicitMintForbidden);
            }
            if burned_units == 0 {
                return Err(ColoredV1Error::BurnPathRequiresBurnedUnits);
            }
            let accounted = output_units.saturating_add(burned_units);
            if input_units != accounted {
                return Err(ColoredV1Error::UnitsNotConserved {
                    color_id,
                    inputs: input_units,
                    outputs: output_units,
                    burned: burned_units,
                });
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admitted() -> ColoredAdmissionV1 {
        ColoredAdmissionV1 {
            covenants_enabled: true,
            template_admitted: true,
        }
    }

    fn output(units: u64, color: [u8; 32]) -> ColoredV1Output {
        ColoredV1Output {
            template_id: COLORED_TEMPLATE_ID_V1.into(),
            color_id: color,
            units,
            owner_pk: "owner".into(),
            amount: 1,
        }
    }

    #[test]
    fn inactive_admission_rejects_mint_and_transfer() {
        let color = derive_color_id_v1("c", "tx", 0);
        let out = output(10, color);
        assert_eq!(
            evaluate_colored_mint_v1(ColoredAdmissionV1::INACTIVE, &out, "c", "tx", 0),
            Err(ColoredV1Error::TemplateDisabled)
        );
        assert_eq!(
            evaluate_colored_units_v1(
                ColoredAdmissionV1::INACTIVE,
                ColoredSpendPathV1::Transfer,
                color,
                10,
                10,
                0
            ),
            Err(ColoredV1Error::TemplateDisabled)
        );
    }

    #[test]
    fn mint_binds_color_id_to_genesis_outpoint() {
        let color = derive_color_id_v1("pulsedag-testnet", "aa".repeat(32).as_str(), 1);
        let other = derive_color_id_v1("pulsedag-private", "aa".repeat(32).as_str(), 1);
        assert_ne!(color, other);
        let out = output(100, color);
        evaluate_colored_mint_v1(admitted(), &out, "pulsedag-testnet", &"aa".repeat(32), 1)
            .unwrap();
        assert_eq!(
            evaluate_colored_mint_v1(admitted(), &out, "pulsedag-private", &"aa".repeat(32), 1),
            Err(ColoredV1Error::ColorIdMismatch)
        );
    }

    #[test]
    fn transfer_conserves_units() {
        let color = derive_color_id_v1("c", "tx", 0);
        evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Transfer, color, 50, 50, 0)
            .unwrap();
        assert!(matches!(
            evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Transfer, color, 50, 49, 0),
            Err(ColoredV1Error::UnitsNotConserved { .. })
        ));
        assert_eq!(
            evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Transfer, color, 50, 40, 10),
            Err(ColoredV1Error::BurnMustBeDeclared)
        );
    }

    #[test]
    fn burn_must_account_for_destroyed_units() {
        let color = derive_color_id_v1("c", "tx", 0);
        evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Burn, color, 50, 20, 30).unwrap();
        assert_eq!(
            evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Burn, color, 50, 50, 0),
            Err(ColoredV1Error::BurnPathRequiresBurnedUnits)
        );
        assert!(matches!(
            evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Burn, color, 50, 20, 20),
            Err(ColoredV1Error::UnitsNotConserved { .. })
        ));
    }

    #[test]
    fn transfer_cannot_mint_implicitly() {
        let color = derive_color_id_v1("c", "tx", 0);
        assert_eq!(
            evaluate_colored_units_v1(admitted(), ColoredSpendPathV1::Transfer, color, 0, 10, 0),
            Err(ColoredV1Error::ImplicitMintForbidden)
        );
    }
}
