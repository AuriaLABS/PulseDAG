//! Planning matcher for work-certificate v1.
//!
//! Transferable statement that a miner produced PulseDAG-domain
//! kHeavyHash work. Consensus does not require certificates to accept
//! blocks. Default issuance is fail-closed: the node must not stamp
//! certs or run pool accounting.

pub const WORK_CERT_DOMAIN_V1: &str = "PulseDAG:work-cert:v1";
pub const WORK_CERT_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkCertificateV1 {
    pub chain_id: String,
    pub template_id: String,
    pub selected_tip: String,
    pub header_commitment: [u8; 32],
    pub nonce: u64,
    pub hash256: [u8; 32],
    pub target256: [u8; 32],
    pub miner_pk: String,
    pub issued_pulse_height: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkCertAdmissionV1 {
    pub issuance_enabled: bool,
}

impl WorkCertAdmissionV1 {
    pub const INACTIVE: Self = Self {
        issuance_enabled: false,
    };

    pub fn admitted(self) -> bool {
        self.issuance_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkCertV1Error {
    IssuanceDisabled,
    EmptyChainId,
    EmptyTemplateId,
    EmptySelectedTip,
    EmptyMinerKey,
    WorkDoesNotMeetTarget,
}

impl std::fmt::Display for WorkCertV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IssuanceDisabled => {
                write!(
                    f,
                    "work-cert issuance is disabled; node must not stamp shares"
                )
            }
            Self::EmptyChainId => write!(f, "work certificate chain_id must not be empty"),
            Self::EmptyTemplateId => write!(f, "work certificate template_id must not be empty"),
            Self::EmptySelectedTip => write!(f, "work certificate selected_tip must not be empty"),
            Self::EmptyMinerKey => write!(f, "work certificate miner_pk must not be empty"),
            Self::WorkDoesNotMeetTarget => {
                write!(f, "hash256 does not meet the certificate target")
            }
        }
    }
}

impl std::error::Error for WorkCertV1Error {}

/// Big-endian compare: `hash256 <= target256`.
pub fn work_meets_target_v1(hash256: &[u8; 32], target256: &[u8; 32]) -> bool {
    hash256.as_slice() <= target256.as_slice()
}

pub fn validate_work_certificate_v1(cert: &WorkCertificateV1) -> Result<(), WorkCertV1Error> {
    if cert.chain_id.is_empty() {
        return Err(WorkCertV1Error::EmptyChainId);
    }
    if cert.template_id.is_empty() {
        return Err(WorkCertV1Error::EmptyTemplateId);
    }
    if cert.selected_tip.is_empty() {
        return Err(WorkCertV1Error::EmptySelectedTip);
    }
    if cert.miner_pk.is_empty() {
        return Err(WorkCertV1Error::EmptyMinerKey);
    }
    if !work_meets_target_v1(&cert.hash256, &cert.target256) {
        return Err(WorkCertV1Error::WorkDoesNotMeetTarget);
    }
    Ok(())
}

/// Stamping a certificate is a node-local optional act. Default admission
/// refuses so `pulsedagd` does not grow share accounting.
pub fn issue_work_certificate_v1(
    admission: WorkCertAdmissionV1,
    cert: &WorkCertificateV1,
) -> Result<(), WorkCertV1Error> {
    if !admission.admitted() {
        return Err(WorkCertV1Error::IssuanceDisabled);
    }
    validate_work_certificate_v1(cert)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert(hash: u8, target: u8) -> WorkCertificateV1 {
        WorkCertificateV1 {
            chain_id: "pulsedag-testnet".into(),
            template_id: "job-1".into(),
            selected_tip: "tip".into(),
            header_commitment: [1u8; 32],
            nonce: 9,
            hash256: [hash; 32],
            target256: [target; 32],
            miner_pk: "miner".into(),
            issued_pulse_height: Some(10),
        }
    }

    #[test]
    fn default_node_does_not_stamp_certificates() {
        let cert = cert(0x10, 0x20);
        assert_eq!(
            issue_work_certificate_v1(WorkCertAdmissionV1::INACTIVE, &cert),
            Err(WorkCertV1Error::IssuanceDisabled)
        );
        // validation of an already-issued statement is still possible
        validate_work_certificate_v1(&cert).unwrap();
    }

    #[test]
    fn hash_must_not_exceed_target() {
        validate_work_certificate_v1(&cert(0x10, 0x10)).unwrap();
        assert_eq!(
            validate_work_certificate_v1(&cert(0x21, 0x20)),
            Err(WorkCertV1Error::WorkDoesNotMeetTarget)
        );
    }

    #[test]
    fn chain_id_and_miner_are_required() {
        let mut sample = cert(0x01, 0xff);
        sample.chain_id.clear();
        assert_eq!(
            validate_work_certificate_v1(&sample),
            Err(WorkCertV1Error::EmptyChainId)
        );
        sample = cert(0x01, 0xff);
        sample.miner_pk.clear();
        assert_eq!(
            validate_work_certificate_v1(&sample),
            Err(WorkCertV1Error::EmptyMinerKey)
        );
    }

    #[test]
    fn issuance_when_admitted_still_checks_target() {
        let admission = WorkCertAdmissionV1 {
            issuance_enabled: true,
        };
        issue_work_certificate_v1(admission, &cert(0x01, 0x02)).unwrap();
        assert_eq!(
            issue_work_certificate_v1(admission, &cert(0x03, 0x02)),
            Err(WorkCertV1Error::WorkDoesNotMeetTarget)
        );
    }
}
