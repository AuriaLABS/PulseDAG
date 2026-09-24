use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    monetary_v3::{monetary_policy_fingerprint_v3, MONETARY_POLICY_FINGERPRINT_V3},
    protocol::{ProtocolActivationIdentity, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V1},
    state::ChainState,
    tx::TRANSACTION_VERSION_V1,
};

pub const PROTOCOL_ACTIVATION_RECORD_SCHEMA_VERSION: u32 = 1;
pub const PROTOCOL_MONETARY_ACTIVATION_RECORD_SCHEMA_VERSION: u32 = 2;
pub const PROTOCOL_MONETARY_BINDING_DOMAIN_V2: &[u8] = b"PulseDAG:protocol-monetary-activation:v2";

/// Versioned persistence envelope for the protocol activation identity.
///
/// This record is intentionally independent from RocksDB/snapshot I/O. It gives
/// storage, snapshots and other persistence surfaces one canonical object to
/// serialize and verify before any activated state is published or replayed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolActivationRecordV1 {
    pub schema_version: u32,
    pub identity: ProtocolActivationIdentity,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolRestoreIdentityGate {
    LegacySchema1Compatibility,
    VerifiedRecordV1,
}

impl ProtocolRestoreIdentityGate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegacySchema1Compatibility => "legacy_schema1_compatibility",
            Self::VerifiedRecordV1 => "verified_record_v1",
        }
    }
}

impl ProtocolActivationRecordV1 {
    pub fn from_identity(identity: ProtocolActivationIdentity) -> Result<Self, String> {
        identity.validate()?;
        let fingerprint = identity.fingerprint()?;
        Ok(Self {
            schema_version: PROTOCOL_ACTIVATION_RECORD_SCHEMA_VERSION,
            identity,
            fingerprint,
        })
    }

    pub fn legacy_from_state(state: &ChainState) -> Result<Self, String> {
        Self::from_identity(ProtocolActivationIdentity::legacy_from_state(state))
    }

    /// Validate the record without comparing it to a caller expectation.
    /// Corrupt schema, identity or fingerprint data fails closed.
    pub fn validate_internal(&self) -> Result<(), String> {
        if self.schema_version != PROTOCOL_ACTIVATION_RECORD_SCHEMA_VERSION {
            return Err(format!(
                "unsupported protocol activation record schema version {}; expected {}",
                self.schema_version, PROTOCOL_ACTIVATION_RECORD_SCHEMA_VERSION
            ));
        }
        self.identity.validate()?;
        let computed = self.identity.fingerprint()?;
        if self.fingerprint != computed {
            return Err(format!(
                "protocol activation fingerprint mismatch: stored {}, computed {}",
                self.fingerprint, computed
            ));
        }
        Ok(())
    }

    /// Require an exact activation identity match before persisted state can be
    /// consumed. This deliberately rejects mixed chain, genesis, tx/header,
    /// consensus-mode or ordering identities rather than normalizing them.
    pub fn verify_expected(&self, expected: &ProtocolActivationIdentity) -> Result<(), String> {
        self.validate_internal()?;
        expected.validate()?;
        if &self.identity != expected {
            return Err(
                "persisted protocol activation identity does not match expected identity".into(),
            );
        }
        let expected_fingerprint = expected.fingerprint()?;
        if self.fingerprint != expected_fingerprint {
            return Err(format!(
                "persisted protocol activation fingerprint {} does not match expected {}",
                self.fingerprint, expected_fingerprint
            ));
        }
        Ok(())
    }
}

fn encode_monetary_binding_field(out: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).expect("protocol monetary binding field exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value);
}

/// v3 persistence identity that cryptographically binds the existing protocol
/// activation identity to the frozen monetary-policy fingerprint. This record
/// is additive: legacy/v2 restore semantics remain unchanged until an explicit
/// v3 activation path requires this schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolMonetaryActivationRecordV2 {
    pub schema_version: u32,
    pub identity: ProtocolActivationIdentity,
    pub protocol_fingerprint: String,
    pub monetary_policy_fingerprint: String,
    pub binding_fingerprint: String,
}

impl ProtocolMonetaryActivationRecordV2 {
    pub fn from_identity(identity: ProtocolActivationIdentity) -> Result<Self, String> {
        identity.validate()?;
        let protocol_fingerprint = identity.fingerprint()?;
        let monetary_policy_fingerprint = monetary_policy_fingerprint_v3();
        let binding_fingerprint =
            Self::compute_binding_fingerprint(&protocol_fingerprint, &monetary_policy_fingerprint);
        Ok(Self {
            schema_version: PROTOCOL_MONETARY_ACTIVATION_RECORD_SCHEMA_VERSION,
            identity,
            protocol_fingerprint,
            monetary_policy_fingerprint,
            binding_fingerprint,
        })
    }

    fn compute_binding_fingerprint(
        protocol_fingerprint: &str,
        monetary_policy_fingerprint: &str,
    ) -> String {
        let mut bytes = Vec::with_capacity(192);
        encode_monetary_binding_field(&mut bytes, PROTOCOL_MONETARY_BINDING_DOMAIN_V2);
        encode_monetary_binding_field(&mut bytes, protocol_fingerprint.as_bytes());
        encode_monetary_binding_field(&mut bytes, monetary_policy_fingerprint.as_bytes());
        hex::encode(Sha256::digest(bytes))
    }

    pub fn validate_internal(&self) -> Result<(), String> {
        if self.schema_version != PROTOCOL_MONETARY_ACTIVATION_RECORD_SCHEMA_VERSION {
            return Err(format!(
                "unsupported protocol monetary activation record schema version {}; expected {}",
                self.schema_version, PROTOCOL_MONETARY_ACTIVATION_RECORD_SCHEMA_VERSION
            ));
        }
        self.identity.validate()?;
        let expected_protocol = self.identity.fingerprint()?;
        if self.protocol_fingerprint != expected_protocol {
            return Err("protocol fingerprint mismatch in monetary activation record".into());
        }
        if self.monetary_policy_fingerprint != MONETARY_POLICY_FINGERPRINT_V3
            || self.monetary_policy_fingerprint != monetary_policy_fingerprint_v3()
        {
            return Err("monetary policy fingerprint mismatch in activation record".into());
        }
        let expected_binding = Self::compute_binding_fingerprint(
            &self.protocol_fingerprint,
            &self.monetary_policy_fingerprint,
        );
        if self.binding_fingerprint != expected_binding {
            return Err("protocol/monetary binding fingerprint mismatch".into());
        }
        Ok(())
    }

    pub fn verify_expected(&self, expected: &ProtocolActivationIdentity) -> Result<(), String> {
        self.validate_internal()?;
        expected.validate()?;
        if &self.identity != expected {
            return Err(
                "persisted protocol monetary activation identity does not match expected identity"
                    .into(),
            );
        }
        Ok(())
    }
}

/// Classify whether persisted state is safe to restore under an explicit
/// protocol expectation. A missing activation record is tolerated only for
/// historical v1 identities; activated or mixed identities fail closed.
pub fn verify_protocol_restore_identity(
    record: Option<&ProtocolActivationRecordV1>,
    expected: &ProtocolActivationIdentity,
) -> Result<ProtocolRestoreIdentityGate, String> {
    expected.validate()?;

    if let Some(record) = record {
        record.verify_expected(expected)?;
        return Ok(ProtocolRestoreIdentityGate::VerifiedRecordV1);
    }

    let legacy_compatible = expected.transaction_protocol_version == TRANSACTION_VERSION_V1
        && expected.block_header_protocol_version == BLOCK_HEADER_VERSION_V1
        && matches!(
            expected.consensus_mode,
            ProtocolConsensusMode::Legacy | ProtocolConsensusMode::GhostdagDev
        );

    if legacy_compatible {
        Ok(ProtocolRestoreIdentityGate::LegacySchema1Compatibility)
    } else {
        Err(
            "protocol activation record missing for non-legacy restore expectation; refusing fallback"
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        protocol::{ProtocolConsensusMode, BLOCK_HEADER_VERSION_V2},
    };

    #[test]
    fn legacy_record_is_derived_from_current_state_identity() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let record = ProtocolActivationRecordV1::legacy_from_state(&state).unwrap();
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);

        assert_eq!(
            record.schema_version,
            PROTOCOL_ACTIVATION_RECORD_SCHEMA_VERSION
        );
        assert_eq!(record.identity, expected);
        assert_eq!(record.fingerprint, expected.fingerprint().unwrap());
        assert!(record.verify_expected(&expected).is_ok());
    }

    #[test]
    fn activated_v2_record_preserves_release_identity_exactly() {
        let expected = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let record = ProtocolActivationRecordV1::from_identity(expected.clone()).unwrap();

        assert_eq!(
            record.identity.consensus_mode,
            ProtocolConsensusMode::GhostdagV1
        );
        assert_eq!(
            record.identity.block_header_protocol_version,
            BLOCK_HEADER_VERSION_V2
        );
        assert!(record.verify_expected(&expected).is_ok());
    }

    #[test]
    fn corrupted_fingerprint_fails_closed() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut record = ProtocolActivationRecordV1::legacy_from_state(&state).unwrap();
        record.fingerprint = "00".repeat(32);

        assert!(record.validate_internal().is_err());
    }

    #[test]
    fn unsupported_record_schema_fails_closed() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut record = ProtocolActivationRecordV1::legacy_from_state(&state).unwrap();
        record.schema_version += 1;

        assert!(record.validate_internal().is_err());
    }

    #[test]
    fn every_identity_mismatch_is_rejected() {
        let base = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let record = ProtocolActivationRecordV1::from_identity(base.clone()).unwrap();

        let mut variants = Vec::new();

        let mut value = base.clone();
        value.chain_id.push_str("-other");
        variants.push(value);

        let mut value = base.clone();
        value.genesis_hash.push_str("-other");
        variants.push(value);

        let mut value = base.clone();
        value.transaction_protocol_version += 1;
        variants.push(value);

        let mut value = base.clone();
        value.block_header_protocol_version += 1;
        variants.push(value);

        let mut value = base.clone();
        value.consensus_mode = ProtocolConsensusMode::Legacy;
        variants.push(value);

        let mut value = base;
        value.dag_ordering_version.push_str("-other");
        variants.push(value);

        for variant in variants {
            assert!(record.verify_expected(&variant).is_err());
        }
    }

    #[test]
    fn missing_record_is_allowed_only_for_explicit_legacy_identity() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);

        assert_eq!(
            verify_protocol_restore_identity(None, &expected).unwrap(),
            ProtocolRestoreIdentityGate::LegacySchema1Compatibility
        );
    }

    #[test]
    fn missing_record_is_rejected_for_activated_v2_identity() {
        let expected = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert!(verify_protocol_restore_identity(None, &expected).is_err());
    }

    #[test]
    fn missing_record_is_rejected_for_mixed_nonlegacy_identity() {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut expected = ProtocolActivationIdentity::legacy_from_state(&state);
        expected.block_header_protocol_version = BLOCK_HEADER_VERSION_V2;

        assert!(verify_protocol_restore_identity(None, &expected).is_err());
    }

    #[test]
    fn present_record_requires_exact_identity_and_reports_verified_gate() {
        let expected = ProtocolActivationIdentity::activated_v2(
            "pulsedag-testnet-v2",
            "genesis-v2",
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let record = ProtocolActivationRecordV1::from_identity(expected.clone()).unwrap();

        assert_eq!(
            verify_protocol_restore_identity(Some(&record), &expected).unwrap(),
            ProtocolRestoreIdentityGate::VerifiedRecordV1
        );

        let mut wrong = expected;
        wrong.chain_id.push_str("-other");
        assert!(verify_protocol_restore_identity(Some(&record), &wrong).is_err());
    }

    #[test]
    fn restore_gate_labels_are_stable() {
        assert_eq!(
            ProtocolRestoreIdentityGate::LegacySchema1Compatibility.as_str(),
            "legacy_schema1_compatibility"
        );
        assert_eq!(
            ProtocolRestoreIdentityGate::VerifiedRecordV1.as_str(),
            "verified_record_v1"
        );
    }

    #[test]
    fn monetary_record_binds_protocol_and_frozen_policy() {
        let expected = ProtocolActivationIdentity::activated_v2(
            "pulsedag-v3-mainnet-candidate",
            "genesis-v3",
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let record = ProtocolMonetaryActivationRecordV2::from_identity(expected.clone()).unwrap();

        assert_eq!(
            record.schema_version,
            PROTOCOL_MONETARY_ACTIVATION_RECORD_SCHEMA_VERSION
        );
        assert_eq!(
            record.monetary_policy_fingerprint,
            MONETARY_POLICY_FINGERPRINT_V3
        );
        assert_eq!(record.protocol_fingerprint, expected.fingerprint().unwrap());
        assert!(record.validate_internal().is_ok());
        assert!(record.verify_expected(&expected).is_ok());
    }

    #[test]
    fn monetary_record_fails_closed_on_policy_or_binding_drift() {
        let expected = ProtocolActivationIdentity::activated_v2(
            "pulsedag-v3-mainnet-candidate",
            "genesis-v3",
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        let mut wrong_policy =
            ProtocolMonetaryActivationRecordV2::from_identity(expected.clone()).unwrap();
        wrong_policy.monetary_policy_fingerprint = "00".repeat(32);
        assert!(wrong_policy.validate_internal().is_err());

        let mut wrong_binding = ProtocolMonetaryActivationRecordV2::from_identity(expected).unwrap();
        wrong_binding.binding_fingerprint = "11".repeat(32);
        assert!(wrong_binding.validate_internal().is_err());
    }

}
