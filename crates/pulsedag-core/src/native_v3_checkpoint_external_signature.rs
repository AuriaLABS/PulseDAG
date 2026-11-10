//! DORMANT native-v3 checkpoint signature verification against an independently
//! supplied trusted public key and exact checkpoint/snapshot/monetary pins.
//!
//! A signature from a self-declared manifest key grants NO authority. The
//! trusted pin and monotonically committed previous position MUST come from
//! outside the untrusted snapshot, database and network. Even this checked
//! signature is NOT full-DAG consensus, monetary replay, pruning finality,
//! snapshot-byte authentication or a live restore/launch permission (#1367).
//! No runtime or P2P admission path imports this module.

use ed25519_dalek::{Signature, VerifyingKey};

use crate::errors::PulseError;

const SIGNATURE_DOMAIN: &[u8] =
    b"PulseDAG:NATIVE-V3:DORMANT-EXTERNALLY-PINNED-CHECKPOINT:v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeV3CheckpointSignedClaim {
    pub chain_id: String,
    pub genesis_hash: [u8; 32],
    pub checkpoint_hash: [u8; 32],
    pub selected_tip_hash: [u8; 32],
    pub snapshot_sha256: [u8; 32],
    pub consensus_digest: [u8; 32],
    pub monetary_digest: [u8; 32],
    pub checkpoint_height: u64,
    pub blue_score: u64,
    pub sequence: u64,
    /// Copied from untrusted data: NEVER accepted as a trust anchor.
    pub claimed_public_key: [u8; 32],
    pub signature: [u8; 64],
}

/// This object must be supplied from a separately authenticated operator or
/// network configuration. Constructing it from the claim makes it untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeV3ExternallyTrustedCheckpointPin {
    pub chain_id: String,
    pub genesis_hash: [u8; 32],
    pub signer_public_key: [u8; 32],
    pub checkpoint_hash: [u8; 32],
    pub selected_tip_hash: [u8; 32],
    pub snapshot_sha256: [u8; 32],
    pub consensus_digest: [u8; 32],
    pub monetary_digest: [u8; 32],
    pub minimum_height: u64,
    pub minimum_blue_score: u64,
    pub minimum_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeV3PreviousAuthenticatedPosition {
    pub height: u64,
    pub blue_score: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeV3DormantSignatureVerdict {
    pub checkpoint_hash: [u8; 32],
    pub sequence: u64,
    /// This means *only* the message verified with the separately pinned key.
    pub signature_matches_pinned_key: bool,
    pub full_dag_consensus_verified: bool,
    pub monetary_replay_verified: bool,
    pub snapshot_bytes_verified: bool,
    pub checkpoint_restore_authorized: bool,
    pub testnet_or_mainnet_launch_authorized: bool,
}

fn reject(reason: &str) -> PulseError {
    PulseError::InvalidBlock(format!(
        "DORMANT externally pinned native-v3 checkpoint: {reason}"
    ))
}

fn append_field(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value);
}

/// Canonical offline signature preimage, not a consensus checkpoint encoding.
pub fn dormant_native_v3_checkpoint_signature_preimage(
    claim: &NativeV3CheckpointSignedClaim,
) -> Result<Vec<u8>, PulseError> {
    if claim.chain_id.is_empty()
        || claim.chain_id.len() > 128
        || !claim
            .chain_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(reject("chain identity must be explicit bounded ASCII"));
    }
    if claim.checkpoint_height == 0 || claim.blue_score == 0 || claim.sequence == 0 {
        return Err(reject("checkpoint height, score and sequence must be positive"));
    }
    if [
        claim.genesis_hash,
        claim.checkpoint_hash,
        claim.selected_tip_hash,
        claim.snapshot_sha256,
        claim.consensus_digest,
        claim.monetary_digest,
    ]
    .iter()
    .any(|digest| digest.iter().all(|byte| *byte == 0))
    {
        return Err(reject("missing zero-valued authenticated domain or state digest"));
    }
    let mut message = Vec::with_capacity(512);
    append_field(&mut message, SIGNATURE_DOMAIN);
    append_field(&mut message, claim.chain_id.as_bytes());
    for digest in [
        &claim.genesis_hash,
        &claim.checkpoint_hash,
        &claim.selected_tip_hash,
        &claim.snapshot_sha256,
        &claim.consensus_digest,
        &claim.monetary_digest,
        &claim.claimed_public_key,
    ] {
        message.extend_from_slice(digest);
    }
    message.extend_from_slice(&claim.checkpoint_height.to_le_bytes());
    message.extend_from_slice(&claim.blue_score.to_le_bytes());
    message.extend_from_slice(&claim.sequence.to_le_bytes());
    Ok(message)
}

/// A successful signature check is evidence for ONE externally pinned byte
/// commitment, NOT permission to trust pruned history or activate monetary v3.
pub fn verify_dormant_native_v3_checkpoint_signature(
    claim: &NativeV3CheckpointSignedClaim,
    pin: &NativeV3ExternallyTrustedCheckpointPin,
    previous: Option<NativeV3PreviousAuthenticatedPosition>,
) -> Result<NativeV3DormantSignatureVerdict, PulseError> {
    let preimage = dormant_native_v3_checkpoint_signature_preimage(claim)?;
    if claim.chain_id != pin.chain_id || claim.genesis_hash != pin.genesis_hash {
        return Err(reject("chain identity/genesis differs from independent frozen pin"));
    }
    if claim.claimed_public_key != pin.signer_public_key {
        return Err(reject("self-declared signer differs from externally pinned key"));
    }
    if claim.checkpoint_hash != pin.checkpoint_hash
        || claim.selected_tip_hash != pin.selected_tip_hash
        || claim.snapshot_sha256 != pin.snapshot_sha256
        || claim.consensus_digest != pin.consensus_digest
        || claim.monetary_digest != pin.monetary_digest
    {
        return Err(reject("checkpoint/consensus/monetary digest differs from independent pin"));
    }
    if claim.checkpoint_height < pin.minimum_height
        || claim.blue_score < pin.minimum_blue_score
        || claim.sequence < pin.minimum_sequence
    {
        return Err(reject("checkpoint below externally pinned anti-rollback floor"));
    }
    if let Some(last) = previous {
        if claim.sequence <= last.sequence
            || claim.checkpoint_height < last.height
            || claim.blue_score < last.blue_score
        {
            return Err(reject("checkpoint cannot replay sequence or rewind committed position"));
        }
    }
    let verifying_key = VerifyingKey::from_bytes(&pin.signer_public_key)
        .map_err(|_| reject("externally pinned signer has invalid Ed25519 key bytes"))?;
    let signature = Signature::from_bytes(&claim.signature);
    verifying_key
        .verify_strict(&preimage, &signature)
        .map_err(|_| reject("Ed25519 checkpoint signature does not verify against pin"))?;
    Ok(NativeV3DormantSignatureVerdict {
        checkpoint_hash: claim.checkpoint_hash,
        sequence: claim.sequence,
        signature_matches_pinned_key: true,
        full_dag_consensus_verified: false,
        monetary_replay_verified: false,
        snapshot_bytes_verified: false,
        checkpoint_restore_authorized: false,
        testnet_or_mainnet_launch_authorized: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture() -> (
        NativeV3CheckpointSignedClaim,
        NativeV3ExternallyTrustedCheckpointPin,
        SigningKey,
    ) {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let public_key = key.verifying_key().to_bytes();
        let claim = NativeV3CheckpointSignedClaim {
            chain_id: "pulsedag-public-testnet-v3".into(),
            genesis_hash: [1; 32],
            checkpoint_hash: [2; 32],
            selected_tip_hash: [3; 32],
            snapshot_sha256: [4; 32],
            consensus_digest: [5; 32],
            monetary_digest: [6; 32],
            checkpoint_height: 120,
            blue_score: 118,
            sequence: 9,
            claimed_public_key: public_key,
            signature: [0; 64],
        };
        let pin = NativeV3ExternallyTrustedCheckpointPin {
            chain_id: claim.chain_id.clone(),
            genesis_hash: claim.genesis_hash,
            signer_public_key: public_key,
            checkpoint_hash: claim.checkpoint_hash,
            selected_tip_hash: claim.selected_tip_hash,
            snapshot_sha256: claim.snapshot_sha256,
            consensus_digest: claim.consensus_digest,
            monetary_digest: claim.monetary_digest,
            minimum_height: 119,
            minimum_blue_score: 117,
            minimum_sequence: 9,
        };
        (claim, pin, key)
    }

    fn signed(mut claim: NativeV3CheckpointSignedClaim, signer: &SigningKey) -> NativeV3CheckpointSignedClaim {
        let message = dormant_native_v3_checkpoint_signature_preimage(&claim).unwrap();
        claim.signature = signer.sign(&message).to_bytes();
        claim
    }

    #[test]
    fn native_v3_checkpoint_external_signature_passes_without_authorizing_restore() {
        let (claim, pin, signer) = fixture();
        let claim = signed(claim, &signer);
        let result = verify_dormant_native_v3_checkpoint_signature(&claim, &pin, None).unwrap();
        assert_eq!(result.checkpoint_hash, [2; 32]);
        assert_eq!(result.sequence, 9);
        assert!(result.signature_matches_pinned_key);
        assert!(!result.full_dag_consensus_verified);
        assert!(!result.monetary_replay_verified);
        assert!(!result.snapshot_bytes_verified);
        assert!(!result.checkpoint_restore_authorized);
        assert!(!result.testnet_or_mainnet_launch_authorized);
    }

    #[test]
    fn native_v3_checkpoint_external_signature_rejects_untrusted_and_forged_fields() {
        let (claim, pin, signer) = fixture();
        let claim = signed(claim, &signer);
        let mut changed = claim.clone();
        changed.signature[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_signature(&changed, &pin, None).is_err());
        let mut changed = claim.clone();
        changed.snapshot_sha256[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_signature(&changed, &pin, None).is_err());
        let mut changed = claim.clone();
        changed.consensus_digest[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_signature(&changed, &pin, None).is_err());
        let mut changed = claim.clone();
        changed.monetary_digest[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_signature(&changed, &pin, None).is_err());
        let mut changed = claim.clone();
        changed.selected_tip_hash[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_signature(&changed, &pin, None).is_err());
        let mut changed = claim.clone();
        changed.chain_id = "pulsedag-mainnet-v3".into();
        assert!(verify_dormant_native_v3_checkpoint_signature(&changed, &pin, None).is_err());

        // A completely valid signature by an attacker is not valid unless its
        // signer was independently pinned, not taken from the fake claim.
        let attacker = SigningKey::from_bytes(&[19_u8; 32]);
        let mut forged = claim.clone();
        forged.claimed_public_key = attacker.verifying_key().to_bytes();
        forged = signed(forged, &attacker);
        assert!(verify_dormant_native_v3_checkpoint_signature(&forged, &pin, None).is_err());
    }

    #[test]
    fn native_v3_checkpoint_external_signature_rejects_replay_and_stale_floor() {
        let (claim, pin, signer) = fixture();
        let claim = signed(claim, &signer);
        for last in [
            NativeV3PreviousAuthenticatedPosition { height: 120, blue_score: 118, sequence: 9 },
            NativeV3PreviousAuthenticatedPosition { height: 121, blue_score: 118, sequence: 8 },
            NativeV3PreviousAuthenticatedPosition { height: 120, blue_score: 119, sequence: 8 },
        ] {
            assert!(verify_dormant_native_v3_checkpoint_signature(&claim, &pin, Some(last)).is_err());
        }
        let last = NativeV3PreviousAuthenticatedPosition { height: 119, blue_score: 117, sequence: 8 };
        assert!(verify_dormant_native_v3_checkpoint_signature(&claim, &pin, Some(last)).is_ok());
        let mut pin = pin;
        pin.minimum_sequence = 10;
        assert!(verify_dormant_native_v3_checkpoint_signature(&claim, &pin, None).is_err());
        let mut pin = pin;
        pin.minimum_sequence = 9;
        pin.minimum_blue_score = 119;
        assert!(verify_dormant_native_v3_checkpoint_signature(&claim, &pin, None).is_err());
    }

    #[test]
    fn native_v3_checkpoint_external_signature_rejects_invalid_preimage_inputs() {
        let (claim, _, _) = fixture();
        let mut bad = claim.clone();
        bad.chain_id = "".into();
        assert!(dormant_native_v3_checkpoint_signature_preimage(&bad).is_err());
        let mut bad = claim.clone();
        bad.chain_id = "wrong/id".into();
        assert!(dormant_native_v3_checkpoint_signature_preimage(&bad).is_err());
        let mut bad = claim.clone();
        bad.sequence = 0;
        assert!(dormant_native_v3_checkpoint_signature_preimage(&bad).is_err());
        let mut bad = claim;
        bad.consensus_digest = [0; 32];
        assert!(dormant_native_v3_checkpoint_signature_preimage(&bad).is_err());
    }
}
