//! Planning matcher for Based Apps v0.
//!
//! Commit-and-challenge measured in PulseClock pulses. L1 does not
//! execute the payload. Default evaluation is fail-closed.

use sha2::{Digest, Sha256};

use crate::pulseclock_v1::{
    PulseClockV1Error, PulseObservationV1, PULSE_DOMAIN_V1, PULSE_VERSION_V1,
};

pub const BASED_APP_DOMAIN_V0: &str = "PulseDAG:based-app:v0";
pub const BASED_COMMIT_TEMPLATE_V0: &str = "based_commit_v0";
pub const BASED_CHALLENGE_MIN_PULSES_V0: u32 = 64;
pub const BASED_CHALLENGE_MAX_PULSES_V0: u32 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasedDaModeV0 {
    Inline,
    CommitOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasedAppProfileV0 {
    pub chain_id: String,
    pub operator_set: Vec<String>,
    pub challenge_pulses: u32,
    pub max_blob_bytes: u32,
    pub da_mode: BasedDaModeV0,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasedCommitV0 {
    pub template_id: String,
    pub app_id: [u8; 32],
    pub round: u64,
    pub prev_state_root: [u8; 32],
    pub next_state_root: [u8; 32],
    pub payload_hash: [u8; 32],
    pub payload: Option<Vec<u8>>,
    pub opened_pulse_height: u64,
    pub committer_pk: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasedAppPathV0 {
    Open,
    Challenge,
    Settle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasedAppAdmissionV0 {
    pub based_apps_enabled: bool,
}

impl BasedAppAdmissionV0 {
    pub const INACTIVE: Self = Self {
        based_apps_enabled: false,
    };

    pub fn admitted(self) -> bool {
        self.based_apps_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasedAppV0Error {
    TemplateDisabled,
    UnknownTemplateId { observed: String },
    ChallengeWindowOutOfRange { value: u32 },
    EmptyChainId,
    HiddenOperator,
    DuplicateOperator,
    OperatorNotDeclared,
    OversizedBlob { len: usize, max: u32 },
    InlinePayloadRequired,
    CommitOnlyPayloadForbidden,
    PayloadHashMismatch,
    PulseClockUnavailable,
    PulseClockContextMismatch,
    SettleTooEarly { current: u64, required: u64 },
    ChallengeTooLate { current: u64, deadline: u64 },
}

impl std::fmt::Display for BasedAppV0Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemplateDisabled => {
                write!(f, "based-app v0 is not admitted; output fails closed")
            }
            Self::UnknownTemplateId { observed } => {
                write!(f, "unknown template {observed}; not based_commit_v0")
            }
            Self::ChallengeWindowOutOfRange { value } => {
                write!(f, "challenge_pulses {value} is outside planning bounds")
            }
            Self::EmptyChainId => write!(f, "based-app chain_id must not be empty"),
            Self::HiddenOperator => write!(f, "operator_set must not contain empty keys"),
            Self::DuplicateOperator => {
                write!(f, "operator_set must not contain duplicate keys")
            }
            Self::OperatorNotDeclared => {
                write!(f, "committer is not in the declared operator_set")
            }
            Self::OversizedBlob { len, max } => {
                write!(f, "payload {len} bytes exceeds max_blob_bytes {max}")
            }
            Self::InlinePayloadRequired => {
                write!(f, "da_mode=inline requires a payload matching payload_hash")
            }
            Self::CommitOnlyPayloadForbidden => {
                write!(f, "da_mode=commit_only forbids inline payload bytes")
            }
            Self::PayloadHashMismatch => write!(f, "SHA-256(payload) does not match payload_hash"),
            Self::PulseClockUnavailable => {
                write!(
                    f,
                    "PulseClock metadata unavailable; based-app path fails closed"
                )
            }
            Self::PulseClockContextMismatch => {
                write!(f, "PulseClock context does not match based-app chain/domain")
            }
            Self::SettleTooEarly { current, required } => {
                write!(f, "settle at pulse {current} before required {required}")
            }
            Self::ChallengeTooLate { current, deadline } => {
                write!(f, "challenge at pulse {current} after deadline {deadline}")
            }
        }
    }
}

impl std::error::Error for BasedAppV0Error {}

impl From<PulseClockV1Error> for BasedAppV0Error {
    fn from(_: PulseClockV1Error) -> Self {
        Self::PulseClockUnavailable
    }
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

pub fn derive_app_id_v0(profile: &BasedAppProfileV0) -> Result<[u8; 32], BasedAppV0Error> {
    if profile.chain_id.is_empty() {
        return Err(BasedAppV0Error::EmptyChainId);
    }
    if profile.challenge_pulses < BASED_CHALLENGE_MIN_PULSES_V0
        || profile.challenge_pulses > BASED_CHALLENGE_MAX_PULSES_V0
    {
        return Err(BasedAppV0Error::ChallengeWindowOutOfRange {
            value: profile.challenge_pulses,
        });
    }
    if profile.operator_set.iter().any(|pk| pk.is_empty()) {
        return Err(BasedAppV0Error::HiddenOperator);
    }
    let mut operators = profile.operator_set.clone();
    operators.sort();
    if operators.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(BasedAppV0Error::DuplicateOperator);
    }

    let mut out = Vec::new();
    encode_len_prefixed(&mut out, BASED_APP_DOMAIN_V0.as_bytes());
    encode_len_prefixed(&mut out, profile.chain_id.as_bytes());
    out.extend_from_slice(&profile.challenge_pulses.to_le_bytes());
    out.extend_from_slice(&profile.max_blob_bytes.to_le_bytes());
    let mode = match profile.da_mode {
        BasedDaModeV0::Inline => b"inline".as_slice(),
        BasedDaModeV0::CommitOnly => b"commit_only".as_slice(),
    };
    encode_len_prefixed(&mut out, mode);
    for pk in &operators {
        encode_len_prefixed(&mut out, pk.as_bytes());
    }
    Ok(Sha256::digest(&out).into())
}

pub fn settle_height_v0(commit: &BasedCommitV0, profile: &BasedAppProfileV0) -> u64 {
    commit
        .opened_pulse_height
        .saturating_add(u64::from(profile.challenge_pulses))
}

fn validate_commit_shape(
    profile: &BasedAppProfileV0,
    commit: &BasedCommitV0,
) -> Result<(), BasedAppV0Error> {
    if commit.template_id != BASED_COMMIT_TEMPLATE_V0 {
        return Err(BasedAppV0Error::UnknownTemplateId {
            observed: commit.template_id.clone(),
        });
    }
    let expected = derive_app_id_v0(profile)?;
    if commit.app_id != expected {
        return Err(BasedAppV0Error::UnknownTemplateId {
            observed: hex::encode(commit.app_id),
        });
    }
    if !profile.operator_set.is_empty()
        && !profile
            .operator_set
            .iter()
            .any(|pk| pk == &commit.committer_pk)
    {
        return Err(BasedAppV0Error::OperatorNotDeclared);
    }
    match profile.da_mode {
        BasedDaModeV0::Inline => {
            let payload = commit
                .payload
                .as_ref()
                .ok_or(BasedAppV0Error::InlinePayloadRequired)?;
            if payload.is_empty() {
                return Err(BasedAppV0Error::InlinePayloadRequired);
            }
            if payload.len() > usize::try_from(profile.max_blob_bytes).unwrap_or(usize::MAX) {
                return Err(BasedAppV0Error::OversizedBlob {
                    len: payload.len(),
                    max: profile.max_blob_bytes,
                });
            }
            if Sha256::digest(payload).as_slice() != commit.payload_hash {
                return Err(BasedAppV0Error::PayloadHashMismatch);
            }
        }
        BasedDaModeV0::CommitOnly => {
            if commit.payload.is_some() {
                return Err(BasedAppV0Error::CommitOnlyPayloadForbidden);
            }
        }
    }
    Ok(())
}

fn validate_pulse_context_v0(
    profile: &BasedAppProfileV0,
    pulse: &PulseObservationV1,
) -> Result<(), BasedAppV0Error> {
    if pulse.pulse_version != PULSE_VERSION_V1
        || pulse.domain != PULSE_DOMAIN_V1
        || pulse.chain_id != profile.chain_id
    {
        return Err(BasedAppV0Error::PulseClockContextMismatch);
    }
    Ok(())
}

pub fn evaluate_based_app_v0(
    admission: BasedAppAdmissionV0,
    profile: &BasedAppProfileV0,
    commit: &BasedCommitV0,
    path: BasedAppPathV0,
    pulse: Result<&PulseObservationV1, PulseClockV1Error>,
) -> Result<(), BasedAppV0Error> {
    if !admission.admitted() {
        return Err(BasedAppV0Error::TemplateDisabled);
    }
    validate_commit_shape(profile, commit)?;
    match path {
        BasedAppPathV0::Open => Ok(()),
        BasedAppPathV0::Challenge => {
            let pulse = pulse.map_err(BasedAppV0Error::from)?;
            validate_pulse_context_v0(profile, pulse)?;
            let deadline = settle_height_v0(commit, profile);
            if pulse.pulse_height >= deadline {
                return Err(BasedAppV0Error::ChallengeTooLate {
                    current: pulse.pulse_height,
                    deadline,
                });
            }
            Ok(())
        }
        BasedAppPathV0::Settle => {
            let pulse = pulse.map_err(BasedAppV0Error::from)?;
            validate_pulse_context_v0(profile, pulse)?;
            let required = settle_height_v0(commit, profile);
            if pulse.pulse_height < required {
                return Err(BasedAppV0Error::SettleTooEarly {
                    current: pulse.pulse_height,
                    required,
                });
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulseclock_v1::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    fn pulse_at(height: u64) -> PulseObservationV1 {
        PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "app-test".into(),
            selected_tip: "tip".into(),
            pulse_height: height,
            pulse_time: 1_700_000_000,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 3,
            uncertainty_secs: 1,
            finality_lag: height,
        }
    }

    fn profile() -> BasedAppProfileV0 {
        BasedAppProfileV0 {
            chain_id: "app-test".into(),
            operator_set: vec!["op-a".into()],
            challenge_pulses: 64,
            max_blob_bytes: 64,
            da_mode: BasedDaModeV0::Inline,
        }
    }

    fn commit_for(profile: &BasedAppProfileV0, payload: &[u8]) -> BasedCommitV0 {
        BasedCommitV0 {
            template_id: BASED_COMMIT_TEMPLATE_V0.into(),
            app_id: derive_app_id_v0(profile).unwrap(),
            round: 1,
            prev_state_root: [0u8; 32],
            next_state_root: [1u8; 32],
            payload_hash: Sha256::digest(payload).into(),
            payload: Some(payload.to_vec()),
            opened_pulse_height: 100,
            committer_pk: "op-a".into(),
        }
    }

    fn admitted() -> BasedAppAdmissionV0 {
        BasedAppAdmissionV0 {
            based_apps_enabled: true,
        }
    }

    #[test]
    fn inactive_flag_rejects_open() {
        let profile = profile();
        let commit = commit_for(&profile, b"hello");
        assert_eq!(
            evaluate_based_app_v0(
                BasedAppAdmissionV0::INACTIVE,
                &profile,
                &commit,
                BasedAppPathV0::Open,
                Ok(&pulse_at(100))
            ),
            Err(BasedAppV0Error::TemplateDisabled)
        );
    }

    #[test]
    fn inline_open_requires_matching_payload() {
        let profile = profile();
        let commit = commit_for(&profile, b"hello");
        evaluate_based_app_v0(
            admitted(),
            &profile,
            &commit,
            BasedAppPathV0::Open,
            Ok(&pulse_at(100)),
        )
        .unwrap();
        let mut bad = commit.clone();
        bad.payload = Some(b"other".to_vec());
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &bad,
                BasedAppPathV0::Open,
                Ok(&pulse_at(100))
            ),
            Err(BasedAppV0Error::PayloadHashMismatch)
        );
    }

    #[test]
    fn app_identity_is_chain_and_da_mode_separated() {
        let inline = profile();

        let mut other_chain = inline.clone();
        other_chain.chain_id = "app-test-other".into();
        assert_ne!(
            derive_app_id_v0(&inline).unwrap(),
            derive_app_id_v0(&other_chain).unwrap()
        );

        let mut commit_only = inline.clone();
        commit_only.da_mode = BasedDaModeV0::CommitOnly;
        assert_ne!(
            derive_app_id_v0(&inline).unwrap(),
            derive_app_id_v0(&commit_only).unwrap()
        );
    }

    #[test]
    fn operator_set_identity_is_order_independent_and_unique() {
        let mut a = profile();
        a.operator_set = vec!["op-b".into(), "op-a".into()];

        let mut b = profile();
        b.operator_set = vec!["op-a".into(), "op-b".into()];

        assert_eq!(derive_app_id_v0(&a).unwrap(), derive_app_id_v0(&b).unwrap());

        let mut duplicate = profile();
        duplicate.operator_set = vec!["op-a".into(), "op-a".into()];
        assert_eq!(
            derive_app_id_v0(&duplicate),
            Err(BasedAppV0Error::DuplicateOperator)
        );
    }

    #[test]
    fn commit_only_forbids_inline_payload_bytes() {
        let mut profile = profile();
        profile.da_mode = BasedDaModeV0::CommitOnly;
        let mut commit = commit_for(&profile, b"hello");

        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Open,
                Ok(&pulse_at(100))
            ),
            Err(BasedAppV0Error::CommitOnlyPayloadForbidden)
        );

        commit.payload = None;
        evaluate_based_app_v0(
            admitted(),
            &profile,
            &commit,
            BasedAppPathV0::Open,
            Ok(&pulse_at(100)),
        )
        .unwrap();
    }

    #[test]
    fn undeclared_operator_is_rejected() {
        let profile = profile();
        let mut commit = commit_for(&profile, b"hello");
        commit.committer_pk = "hidden".into();
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Open,
                Ok(&pulse_at(100))
            ),
            Err(BasedAppV0Error::OperatorNotDeclared)
        );
    }

    #[test]
    fn settle_at_deadline_and_challenge_before() {
        let profile = profile();
        let commit = commit_for(&profile, b"hello");
        evaluate_based_app_v0(
            admitted(),
            &profile,
            &commit,
            BasedAppPathV0::Challenge,
            Ok(&pulse_at(163)),
        )
        .unwrap();
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Challenge,
                Ok(&pulse_at(164))
            ),
            Err(BasedAppV0Error::ChallengeTooLate {
                current: 164,
                deadline: 164
            })
        );
        evaluate_based_app_v0(
            admitted(),
            &profile,
            &commit,
            BasedAppPathV0::Settle,
            Ok(&pulse_at(164)),
        )
        .unwrap();
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Settle,
                Ok(&pulse_at(163))
            ),
            Err(BasedAppV0Error::SettleTooEarly {
                current: 163,
                required: 164
            })
        );
    }

    #[test]
    fn foreign_or_invalid_pulseclock_context_fails_closed() {
        let profile = profile();
        let commit = commit_for(&profile, b"hello");

        let mut foreign = pulse_at(120);
        foreign.chain_id = "other-chain".into();
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Challenge,
                Ok(&foreign)
            ),
            Err(BasedAppV0Error::PulseClockContextMismatch)
        );

        let mut wrong_version = pulse_at(120);
        wrong_version.pulse_version = PULSE_VERSION_V1 + 1;
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Challenge,
                Ok(&wrong_version)
            ),
            Err(BasedAppV0Error::PulseClockContextMismatch)
        );
    }

    #[test]
    fn missing_pulseclock_fails_closed_on_timed_paths() {
        let profile = profile();
        let commit = commit_for(&profile, b"hello");
        assert_eq!(
            evaluate_based_app_v0(
                admitted(),
                &profile,
                &commit,
                BasedAppPathV0::Settle,
                Err(PulseClockV1Error::SelectedTipUnavailable)
            ),
            Err(BasedAppV0Error::PulseClockUnavailable)
        );
    }
}
