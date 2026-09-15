//! Versioned, fail-closed programmability activation contract for #1041.
//!
//! This module freezes the data and validation boundary that a *future*
//! separately authorized protocol upgrade must satisfy before executable
//! contracts can be considered active. It does not wire contract execution
//! into node admission/apply paths and it does not activate programmability.
//!
//! `contracts_gate` remains only a compile/runtime precondition. A local
//! runtime flag, even with `executable-contracts` compiled in, is never
//! sufficient consensus authorization.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    contract_state_v1::{
        CONTRACT_STATE_TRANSITION_VERSION_V1, PROOF_EVIDENCE_POLICY_VERSION_V1,
    },
    contract_v3::{
        ContractV3CompatibilityMetadataV1, ContractV3Error, ProgrammabilityActivationIdentity,
        CONTRACT_TRANSACTION_VERSION_V3,
    },
    protocol::ProtocolActivationIdentity,
};

use super::contracts_compile_time_executable;

pub const PROGRAMMABILITY_ACTIVATION_CONTRACT_VERSION_V1: u16 = 1;
pub const INACTIVE_PROTOCOL_UPGRADE_AUTHORIZATION_COMMITMENT: [u8; 32] = [0; 32];

const PROGRAMMABILITY_ACTIVATION_CONTRACT_DOMAIN_V1: &[u8] =
    b"PulseDAG:programmability-activation-contract:v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u16)]
pub enum ProgrammabilityActivationModeV1 {
    Inactive = 0,
    ProtocolUpgrade = 1,
}

impl ProgrammabilityActivationModeV1 {
    fn canonical_id(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProgrammabilityActivationV1Error {
    #[error("invalid Contract v3 programmability identity: {0}")]
    ContractV3(#[from] ContractV3Error),
    #[error("invalid base protocol activation identity: {0}")]
    BaseProtocolIdentity(String),
    #[error("unsupported programmability activation-contract version {0}")]
    UnsupportedActivationContractVersion(u16),
    #[error("programmability activation contract {field} fingerprint must not be all zero")]
    EmptyFingerprint { field: &'static str },
    #[error(
        "unsupported frozen {field} version {actual}; activation contract requires {expected}"
    )]
    FrozenComponentVersionMismatch {
        field: &'static str,
        actual: u16,
        expected: u16,
    },
    #[error("inactive activation mode requires the exact zero authorization commitment")]
    InactiveAuthorizationMustBeZero,
    #[error("protocol-upgrade activation mode requires a non-zero authorization commitment")]
    ProtocolUpgradeAuthorizationMustBeNonzero,
    #[error("programmability identity is not bound to the expected base protocol fingerprint")]
    ProgrammabilityBaseProtocolMismatch,
    #[error("programmability activation identity fingerprint mismatch")]
    ProgrammabilityIdentityMismatch,
    #[error("base protocol activation identity fingerprint mismatch")]
    BaseProtocolIdentityMismatch,
    #[error("Contract v3 compatibility fingerprint mismatch")]
    ContractV3CompatibilityMismatch,
    #[error("programmability is explicitly inactive")]
    Inactive,
    #[error("trusted protocol-upgrade authorization commitment is missing")]
    TrustedAuthorizationMissing,
    #[error("trusted protocol-upgrade authorization commitment mismatch")]
    AuthorizationCommitmentMismatch,
    #[error("executable-contracts compile precondition is closed")]
    CompileGateClosed,
    #[error("contracts runtime precondition is closed")]
    RuntimeGateClosed,
    #[error("activation-contract canonical field length exceeds u32::MAX")]
    CanonicalLengthOverflow,
}

impl ProgrammabilityActivationV1Error {
    pub fn rejection_code(&self) -> u16 {
        match self {
            Self::ContractV3(_) => 1,
            Self::BaseProtocolIdentity(_) => 2,
            Self::UnsupportedActivationContractVersion(_) => 3,
            Self::EmptyFingerprint { .. } => 4,
            Self::FrozenComponentVersionMismatch { .. } => 5,
            Self::InactiveAuthorizationMustBeZero => 6,
            Self::ProtocolUpgradeAuthorizationMustBeNonzero => 7,
            Self::ProgrammabilityBaseProtocolMismatch => 8,
            Self::ProgrammabilityIdentityMismatch => 9,
            Self::BaseProtocolIdentityMismatch => 10,
            Self::ContractV3CompatibilityMismatch => 11,
            Self::Inactive => 12,
            Self::TrustedAuthorizationMissing => 13,
            Self::AuthorizationCommitmentMismatch => 14,
            Self::CompileGateClosed => 15,
            Self::RuntimeGateClosed => 16,
            Self::CanonicalLengthOverflow => 17,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgrammabilityActivationContractV1 {
    pub version: u16,
    pub mode: ProgrammabilityActivationModeV1,
    pub programmability_identity_fingerprint: [u8; 32],
    pub base_protocol_fingerprint: [u8; 32],
    pub contract_v3_compatibility_fingerprint: [u8; 32],
    pub contract_transaction_version: u16,
    pub contract_state_transition_version: u16,
    pub proof_evidence_policy_version: u16,
    pub protocol_upgrade_authorization_commitment: [u8; 32],
}

impl ProgrammabilityActivationContractV1 {
    pub fn inactive(
        identity: &ProgrammabilityActivationIdentity,
        base_protocol: &ProtocolActivationIdentity,
    ) -> Result<Self, ProgrammabilityActivationV1Error> {
        let bindings = FrozenActivationBindingsV1::derive(identity, base_protocol)?;
        Ok(Self {
            version: PROGRAMMABILITY_ACTIVATION_CONTRACT_VERSION_V1,
            mode: ProgrammabilityActivationModeV1::Inactive,
            programmability_identity_fingerprint: bindings.programmability_identity_fingerprint,
            base_protocol_fingerprint: bindings.base_protocol_fingerprint,
            contract_v3_compatibility_fingerprint: bindings.contract_v3_compatibility_fingerprint,
            contract_transaction_version: CONTRACT_TRANSACTION_VERSION_V3,
            contract_state_transition_version: CONTRACT_STATE_TRANSITION_VERSION_V1,
            proof_evidence_policy_version: PROOF_EVIDENCE_POLICY_VERSION_V1,
            protocol_upgrade_authorization_commitment:
                INACTIVE_PROTOCOL_UPGRADE_AUTHORIZATION_COMMITMENT,
        })
    }

    pub fn protocol_upgrade(
        identity: &ProgrammabilityActivationIdentity,
        base_protocol: &ProtocolActivationIdentity,
        protocol_upgrade_authorization_commitment: [u8; 32],
    ) -> Result<Self, ProgrammabilityActivationV1Error> {
        let bindings = FrozenActivationBindingsV1::derive(identity, base_protocol)?;
        let contract = Self {
            version: PROGRAMMABILITY_ACTIVATION_CONTRACT_VERSION_V1,
            mode: ProgrammabilityActivationModeV1::ProtocolUpgrade,
            programmability_identity_fingerprint: bindings.programmability_identity_fingerprint,
            base_protocol_fingerprint: bindings.base_protocol_fingerprint,
            contract_v3_compatibility_fingerprint: bindings.contract_v3_compatibility_fingerprint,
            contract_transaction_version: CONTRACT_TRANSACTION_VERSION_V3,
            contract_state_transition_version: CONTRACT_STATE_TRANSITION_VERSION_V1,
            proof_evidence_policy_version: PROOF_EVIDENCE_POLICY_VERSION_V1,
            protocol_upgrade_authorization_commitment,
        };
        contract.validate_shape()?;
        Ok(contract)
    }

    pub fn validate_shape(&self) -> Result<(), ProgrammabilityActivationV1Error> {
        if self.version != PROGRAMMABILITY_ACTIVATION_CONTRACT_VERSION_V1 {
            return Err(
                ProgrammabilityActivationV1Error::UnsupportedActivationContractVersion(
                    self.version,
                ),
            );
        }

        ensure_nonzero_fingerprint(
            "programmability_identity",
            &self.programmability_identity_fingerprint,
        )?;
        ensure_nonzero_fingerprint("base_protocol", &self.base_protocol_fingerprint)?;
        ensure_nonzero_fingerprint(
            "contract_v3_compatibility",
            &self.contract_v3_compatibility_fingerprint,
        )?;

        ensure_frozen_version(
            "contract_transaction",
            self.contract_transaction_version,
            CONTRACT_TRANSACTION_VERSION_V3,
        )?;
        ensure_frozen_version(
            "contract_state_transition",
            self.contract_state_transition_version,
            CONTRACT_STATE_TRANSITION_VERSION_V1,
        )?;
        ensure_frozen_version(
            "proof_evidence_policy",
            self.proof_evidence_policy_version,
            PROOF_EVIDENCE_POLICY_VERSION_V1,
        )?;

        match self.mode {
            ProgrammabilityActivationModeV1::Inactive => {
                if self.protocol_upgrade_authorization_commitment
                    != INACTIVE_PROTOCOL_UPGRADE_AUTHORIZATION_COMMITMENT
                {
                    return Err(
                        ProgrammabilityActivationV1Error::InactiveAuthorizationMustBeZero,
                    );
                }
            }
            ProgrammabilityActivationModeV1::ProtocolUpgrade => {
                if is_zero(&self.protocol_upgrade_authorization_commitment) {
                    return Err(
                        ProgrammabilityActivationV1Error::ProtocolUpgradeAuthorizationMustBeNonzero,
                    );
                }
            }
        }

        Ok(())
    }

    pub fn validate_against(
        &self,
        expected_identity: &ProgrammabilityActivationIdentity,
        expected_base_protocol: &ProtocolActivationIdentity,
    ) -> Result<(), ProgrammabilityActivationV1Error> {
        self.validate_shape()?;
        let expected = FrozenActivationBindingsV1::derive(
            expected_identity,
            expected_base_protocol,
        )?;

        if self.programmability_identity_fingerprint
            != expected.programmability_identity_fingerprint
        {
            return Err(ProgrammabilityActivationV1Error::ProgrammabilityIdentityMismatch);
        }
        if self.base_protocol_fingerprint != expected.base_protocol_fingerprint {
            return Err(ProgrammabilityActivationV1Error::BaseProtocolIdentityMismatch);
        }
        if self.contract_v3_compatibility_fingerprint
            != expected.contract_v3_compatibility_fingerprint
        {
            return Err(ProgrammabilityActivationV1Error::ContractV3CompatibilityMismatch);
        }

        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProgrammabilityActivationV1Error> {
        canonical_programmability_activation_contract_bytes_v1(self)
    }

    pub fn fingerprint(&self) -> Result<[u8; 32], ProgrammabilityActivationV1Error> {
        Ok(sha256_array(&self.canonical_bytes()?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrozenActivationBindingsV1 {
    programmability_identity_fingerprint: [u8; 32],
    base_protocol_fingerprint: [u8; 32],
    contract_v3_compatibility_fingerprint: [u8; 32],
}

impl FrozenActivationBindingsV1 {
    fn derive(
        identity: &ProgrammabilityActivationIdentity,
        base_protocol: &ProtocolActivationIdentity,
    ) -> Result<Self, ProgrammabilityActivationV1Error> {
        identity.validate()?;
        let base_protocol_fingerprint =
            protocol_activation_identity_fingerprint_bytes(base_protocol)?;

        if identity.base_protocol_fingerprint != base_protocol_fingerprint {
            return Err(
                ProgrammabilityActivationV1Error::ProgrammabilityBaseProtocolMismatch,
            );
        }

        let programmability_identity_fingerprint = identity.fingerprint()?;
        let contract_v3_compatibility_fingerprint =
            ContractV3CompatibilityMetadataV1::from_identity(identity)?.fingerprint()?;

        Ok(Self {
            programmability_identity_fingerprint,
            base_protocol_fingerprint,
            contract_v3_compatibility_fingerprint,
        })
    }
}

pub fn protocol_activation_identity_fingerprint_bytes(
    identity: &ProtocolActivationIdentity,
) -> Result<[u8; 32], ProgrammabilityActivationV1Error> {
    let bytes = identity
        .canonical_fingerprint_bytes()
        .map_err(ProgrammabilityActivationV1Error::BaseProtocolIdentity)?;
    Ok(sha256_array(&bytes))
}

pub fn canonical_programmability_activation_contract_bytes_v1(
    contract: &ProgrammabilityActivationContractV1,
) -> Result<Vec<u8>, ProgrammabilityActivationV1Error> {
    contract.validate_shape()?;

    let mut out = Vec::with_capacity(192);
    encode_len_prefixed(
        &mut out,
        PROGRAMMABILITY_ACTIVATION_CONTRACT_DOMAIN_V1,
    )?;
    out.extend_from_slice(&contract.version.to_le_bytes());
    out.extend_from_slice(&contract.mode.canonical_id().to_le_bytes());
    out.extend_from_slice(&contract.programmability_identity_fingerprint);
    out.extend_from_slice(&contract.base_protocol_fingerprint);
    out.extend_from_slice(&contract.contract_v3_compatibility_fingerprint);
    out.extend_from_slice(&contract.contract_transaction_version.to_le_bytes());
    out.extend_from_slice(&contract.contract_state_transition_version.to_le_bytes());
    out.extend_from_slice(&contract.proof_evidence_policy_version.to_le_bytes());
    out.extend_from_slice(&contract.protocol_upgrade_authorization_commitment);
    Ok(out)
}

/// Authorize the *contract activation boundary* for a future protocol upgrade.
///
/// This function is deliberately not connected to node transaction admission or
/// state application. The trusted authorization commitment must be supplied by
/// an independently activated protocol-upgrade mechanism; operator config is
/// not a source of consensus authority.
pub fn authorize_programmability_execution_v1(
    contract: &ProgrammabilityActivationContractV1,
    expected_identity: &ProgrammabilityActivationIdentity,
    expected_base_protocol: &ProtocolActivationIdentity,
    runtime_enabled: bool,
    trusted_protocol_upgrade_authorization_commitment: [u8; 32],
) -> Result<(), ProgrammabilityActivationV1Error> {
    authorize_with_gates(
        contract,
        expected_identity,
        expected_base_protocol,
        contracts_compile_time_executable(),
        runtime_enabled,
        trusted_protocol_upgrade_authorization_commitment,
    )
}

fn authorize_with_gates(
    contract: &ProgrammabilityActivationContractV1,
    expected_identity: &ProgrammabilityActivationIdentity,
    expected_base_protocol: &ProtocolActivationIdentity,
    compile_gate_open: bool,
    runtime_enabled: bool,
    trusted_protocol_upgrade_authorization_commitment: [u8; 32],
) -> Result<(), ProgrammabilityActivationV1Error> {
    contract.validate_against(expected_identity, expected_base_protocol)?;

    if contract.mode == ProgrammabilityActivationModeV1::Inactive {
        return Err(ProgrammabilityActivationV1Error::Inactive);
    }

    if is_zero(&trusted_protocol_upgrade_authorization_commitment) {
        return Err(ProgrammabilityActivationV1Error::TrustedAuthorizationMissing);
    }
    if trusted_protocol_upgrade_authorization_commitment
        != contract.protocol_upgrade_authorization_commitment
    {
        return Err(ProgrammabilityActivationV1Error::AuthorizationCommitmentMismatch);
    }
    if !compile_gate_open {
        return Err(ProgrammabilityActivationV1Error::CompileGateClosed);
    }
    if !runtime_enabled {
        return Err(ProgrammabilityActivationV1Error::RuntimeGateClosed);
    }

    Ok(())
}

fn ensure_frozen_version(
    field: &'static str,
    actual: u16,
    expected: u16,
) -> Result<(), ProgrammabilityActivationV1Error> {
    if actual != expected {
        return Err(
            ProgrammabilityActivationV1Error::FrozenComponentVersionMismatch {
                field,
                actual,
                expected,
            },
        );
    }
    Ok(())
}

fn ensure_nonzero_fingerprint(
    field: &'static str,
    value: &[u8; 32],
) -> Result<(), ProgrammabilityActivationV1Error> {
    if is_zero(value) {
        return Err(ProgrammabilityActivationV1Error::EmptyFingerprint { field });
    }
    Ok(())
}

fn is_zero(value: &[u8; 32]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

fn encode_len_prefixed(
    out: &mut Vec<u8>,
    bytes: &[u8],
) -> Result<(), ProgrammabilityActivationV1Error> {
    let len = u32::try_from(bytes.len())
        .map_err(|_| ProgrammabilityActivationV1Error::CanonicalLengthOverflow)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn sha256_array(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_v3::ProgrammabilityDomain;

    fn base_protocol() -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            "ghostdag-order-v1",
        )
    }

    fn programmability_identity() -> ProgrammabilityActivationIdentity {
        let base_protocol = base_protocol();
        ProgrammabilityActivationIdentity {
            version: 1,
            domain: ProgrammabilityDomain::Testnet,
            chain_id: "pulsedag-testnet-v2".to_string(),
            genesis_hash: [0x11; 32],
            base_protocol_fingerprint:
                protocol_activation_identity_fingerprint_bytes(&base_protocol).unwrap(),
        }
    }

    fn inactive_contract() -> ProgrammabilityActivationContractV1 {
        ProgrammabilityActivationContractV1::inactive(
            &programmability_identity(),
            &base_protocol(),
        )
        .unwrap()
    }

    #[test]
    fn protocol_binary_fingerprint_matches_existing_hex_identity() {
        let protocol = base_protocol();
        assert_eq!(
            hex::encode(protocol_activation_identity_fingerprint_bytes(&protocol).unwrap()),
            protocol.fingerprint().unwrap()
        );
        assert_eq!(
            protocol.fingerprint().unwrap(),
            "793cc7ba13c579514ef08f33a79160906e9a2cedd610ea958344d3d8e2c26209"
        );
    }

    #[test]
    fn inactive_contract_has_frozen_golden_fingerprint() {
        let contract = inactive_contract();
        assert_eq!(
            hex::encode(contract.fingerprint().unwrap()),
            "8911363d2572773ffa180e8b0818bac26c70cdef82fb973d7377c1c05fee521e"
        );
        assert_eq!(
            contract.protocol_upgrade_authorization_commitment,
            INACTIVE_PROTOCOL_UPGRADE_AUTHORIZATION_COMMITMENT
        );
    }

    #[test]
    fn inactive_never_authorizes_even_when_compile_and_runtime_are_open() {
        let identity = programmability_identity();
        let protocol = base_protocol();
        let contract =
            ProgrammabilityActivationContractV1::inactive(&identity, &protocol).unwrap();

        assert_eq!(
            authorize_with_gates(
                &contract,
                &identity,
                &protocol,
                true,
                true,
                [0x44; 32],
            ),
            Err(ProgrammabilityActivationV1Error::Inactive)
        );
    }

    #[test]
    fn protocol_upgrade_requires_independent_trusted_authorization() {
        let identity = programmability_identity();
        let protocol = base_protocol();
        let authorization = [0x44; 32];
        let contract = ProgrammabilityActivationContractV1::protocol_upgrade(
            &identity,
            &protocol,
            authorization,
        )
        .unwrap();

        assert!(authorize_with_gates(
            &contract,
            &identity,
            &protocol,
            true,
            true,
            authorization,
        )
        .is_ok());
        assert_eq!(
            authorize_with_gates(
                &contract,
                &identity,
                &protocol,
                true,
                true,
                [0; 32],
            ),
            Err(ProgrammabilityActivationV1Error::TrustedAuthorizationMissing)
        );
        assert_eq!(
            authorize_with_gates(
                &contract,
                &identity,
                &protocol,
                true,
                true,
                [0x45; 32],
            ),
            Err(ProgrammabilityActivationV1Error::AuthorizationCommitmentMismatch)
        );
    }

    #[test]
    fn compile_and_runtime_gates_are_both_required_but_not_sufficient() {
        let identity = programmability_identity();
        let protocol = base_protocol();
        let authorization = [0x44; 32];
        let contract = ProgrammabilityActivationContractV1::protocol_upgrade(
            &identity,
            &protocol,
            authorization,
        )
        .unwrap();

        assert_eq!(
            authorize_with_gates(
                &contract,
                &identity,
                &protocol,
                false,
                true,
                authorization,
            ),
            Err(ProgrammabilityActivationV1Error::CompileGateClosed)
        );
        assert_eq!(
            authorize_with_gates(
                &contract,
                &identity,
                &protocol,
                true,
                false,
                authorization,
            ),
            Err(ProgrammabilityActivationV1Error::RuntimeGateClosed)
        );
        assert_eq!(
            authorize_with_gates(
                &contract,
                &identity,
                &protocol,
                true,
                true,
                [0; 32],
            ),
            Err(ProgrammabilityActivationV1Error::TrustedAuthorizationMissing)
        );
    }

    #[test]
    fn programmability_identity_substitution_fails_closed() {
        let protocol = base_protocol();
        let identity = programmability_identity();
        let contract =
            ProgrammabilityActivationContractV1::inactive(&identity, &protocol).unwrap();

        let mut substituted = identity.clone();
        substituted.chain_id.push_str("-other");

        assert_eq!(
            contract.validate_against(&substituted, &protocol),
            Err(ProgrammabilityActivationV1Error::ProgrammabilityIdentityMismatch)
        );
    }

    #[test]
    fn base_protocol_substitution_fails_closed() {
        let protocol = base_protocol();
        let identity = programmability_identity();
        let contract =
            ProgrammabilityActivationContractV1::inactive(&identity, &protocol).unwrap();

        let substituted = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2-other",
            "genesis-v2",
            "ghostdag-order-v1",
        );

        assert_eq!(
            contract.validate_against(&identity, &substituted),
            Err(ProgrammabilityActivationV1Error::ProgrammabilityBaseProtocolMismatch)
        );
    }

    #[test]
    fn frozen_component_version_substitution_fails_closed() {
        let mut contract = inactive_contract();
        contract.contract_transaction_version += 1;
        assert!(matches!(
            contract.validate_shape(),
            Err(ProgrammabilityActivationV1Error::FrozenComponentVersionMismatch {
                field: "contract_transaction",
                ..
            })
        ));

        let mut contract = inactive_contract();
        contract.contract_state_transition_version += 1;
        assert!(matches!(
            contract.validate_shape(),
            Err(ProgrammabilityActivationV1Error::FrozenComponentVersionMismatch {
                field: "contract_state_transition",
                ..
            })
        ));

        let mut contract = inactive_contract();
        contract.proof_evidence_policy_version += 1;
        assert!(matches!(
            contract.validate_shape(),
            Err(ProgrammabilityActivationV1Error::FrozenComponentVersionMismatch {
                field: "proof_evidence_policy",
                ..
            })
        ));
    }

    #[test]
    fn inactive_and_protocol_upgrade_authorization_shapes_are_strict() {
        let mut inactive = inactive_contract();
        inactive.protocol_upgrade_authorization_commitment = [0x22; 32];
        assert_eq!(
            inactive.validate_shape(),
            Err(ProgrammabilityActivationV1Error::InactiveAuthorizationMustBeZero)
        );

        let identity = programmability_identity();
        let protocol = base_protocol();
        assert_eq!(
            ProgrammabilityActivationContractV1::protocol_upgrade(
                &identity,
                &protocol,
                [0; 32],
            ),
            Err(
                ProgrammabilityActivationV1Error::ProtocolUpgradeAuthorizationMustBeNonzero
            )
        );
    }

    #[test]
    fn canonical_replay_and_strict_serde_are_stable() {
        let contract = inactive_contract();
        let encoded = serde_json::to_string(&contract).unwrap();
        let decoded: ProgrammabilityActivationContractV1 =
            serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, contract);
        assert_eq!(decoded.canonical_bytes().unwrap(), contract.canonical_bytes().unwrap());
        assert_eq!(decoded.fingerprint().unwrap(), contract.fingerprint().unwrap());

        let mut unknown_field = serde_json::to_value(&contract).unwrap();
        unknown_field["future_field"] = serde_json::json!(1);
        assert!(
            serde_json::from_value::<ProgrammabilityActivationContractV1>(unknown_field)
                .is_err()
        );

        let mut unknown_mode = serde_json::to_value(&contract).unwrap();
        unknown_mode["mode"] = serde_json::json!("future_mode");
        assert!(
            serde_json::from_value::<ProgrammabilityActivationContractV1>(unknown_mode)
                .is_err()
        );

        let mut unknown_version = contract.clone();
        unknown_version.version += 1;
        assert!(matches!(
            unknown_version.validate_shape(),
            Err(
                ProgrammabilityActivationV1Error::UnsupportedActivationContractVersion(_)
            )
        ));
    }

    #[test]
    fn consensus_fields_are_fingerprint_bound_or_fail_closed() {
        let base = inactive_contract();
        let base_fingerprint = base.fingerprint().unwrap();

        let mut fingerprint_variants = Vec::new();

        let mut value = base.clone();
        value.programmability_identity_fingerprint[0] ^= 1;
        fingerprint_variants.push(value);

        let mut value = base.clone();
        value.base_protocol_fingerprint[0] ^= 1;
        fingerprint_variants.push(value);

        let mut value = base.clone();
        value.contract_v3_compatibility_fingerprint[0] ^= 1;
        fingerprint_variants.push(value);

        for variant in fingerprint_variants {
            assert_ne!(variant.fingerprint().unwrap(), base_fingerprint);
        }

        let identity = programmability_identity();
        let protocol = base_protocol();
        let upgrade_a = ProgrammabilityActivationContractV1::protocol_upgrade(
            &identity,
            &protocol,
            [0x44; 32],
        )
        .unwrap();
        let upgrade_b = ProgrammabilityActivationContractV1::protocol_upgrade(
            &identity,
            &protocol,
            [0x45; 32],
        )
        .unwrap();

        assert_ne!(upgrade_a.fingerprint().unwrap(), base_fingerprint);
        assert_ne!(upgrade_b.fingerprint().unwrap(), upgrade_a.fingerprint().unwrap());

        let mut invalid = base.clone();
        invalid.version += 1;
        assert!(invalid.fingerprint().is_err());

        let mut invalid = base.clone();
        invalid.contract_transaction_version += 1;
        assert!(invalid.fingerprint().is_err());

        let mut invalid = base.clone();
        invalid.contract_state_transition_version += 1;
        assert!(invalid.fingerprint().is_err());

        let mut invalid = base;
        invalid.proof_evidence_policy_version += 1;
        assert!(invalid.fingerprint().is_err());
    }

    #[test]
    fn rejection_codes_are_stable() {
        assert_eq!(
            ProgrammabilityActivationV1Error::Inactive.rejection_code(),
            12
        );
        assert_eq!(
            ProgrammabilityActivationV1Error::AuthorizationCommitmentMismatch
                .rejection_code(),
            14
        );
        assert_eq!(
            ProgrammabilityActivationV1Error::RuntimeGateClosed.rejection_code(),
            16
        );
    }
}
