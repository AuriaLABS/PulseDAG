//! DORMANT offline verification of actual snapshot bytes against a separately
//! pinned Ed25519-signed checkpoint commitment. This checks ONE bounded byte
//! stream only. It does not authenticate a complete DAG, prove money/state
//! equivalence after pruning, provide atomic persistence or allow restoration.
//! No network, daemon, P2P or miner callsites may depend on this trial.

use std::io::Read;

use sha2::{Digest, Sha256};

use crate::errors::PulseError;
use crate::native_v3_checkpoint_external_signature::{
    verify_dormant_native_v3_checkpoint_signature, NativeV3CheckpointSignedClaim,
    NativeV3DormantSignatureVerdict, NativeV3ExternallyTrustedCheckpointPin,
    NativeV3PreviousAuthenticatedPosition,
};

/// Deliberately small cap: this is not a production checkpoint file loader.
pub const DORMANT_CHECKPOINT_SNAPSHOT_MAX_BYTES: u64 = 8 * 1024 * 1024;
const READ_CHUNK_BYTES: usize = 16 * 1024;

fn reject(reason: &str) -> PulseError {
    PulseError::InvalidBlock(format!(
        "DORMANT native-v3 checkpoint snapshot bytes: {reason}"
    ))
}

/// First verify an Ed25519 signature against a separately provisioned key,
/// exact signed commitments and monotonic floor, *then* read the candidate
/// bytes as one bounded sequential stream. All untrusted I/O errors fail
/// closed. This does not rule out a later file mutation or compromised pin.
pub fn verify_dormant_native_v3_checkpoint_snapshot_bytes<R: Read>(
    claim: &NativeV3CheckpointSignedClaim,
    pin: &NativeV3ExternallyTrustedCheckpointPin,
    previous: Option<NativeV3PreviousAuthenticatedPosition>,
    mut snapshot: R,
    max_bytes: u64,
) -> Result<NativeV3DormantSignatureVerdict, PulseError> {
    let mut verdict = verify_dormant_native_v3_checkpoint_signature(claim, pin, previous)?;
    if max_bytes == 0 || max_bytes > DORMANT_CHECKPOINT_SNAPSHOT_MAX_BYTES {
        return Err(reject("invalid bounded snapshot length limit"));
    }
    let mut sha = Sha256::new();
    let mut length = 0_u64;
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    loop {
        let amount = snapshot
            .read(&mut chunk)
            .map_err(|_| reject("snapshot read I/O error"))?;
        if amount == 0 {
            break;
        }
        length = length
            .checked_add(amount as u64)
            .ok_or_else(|| reject("snapshot length overflow"))?;
        if length > max_bytes {
            return Err(reject("snapshot exceeds bounded byte limit"));
        }
        sha.update(&chunk[..amount]);
    }
    if length == 0 {
        return Err(reject("empty signed snapshot is not a usable state"));
    }
    let actual_sha256: [u8; 32] = sha.finalize().into();
    if actual_sha256 != claim.snapshot_sha256 {
        return Err(reject("actual snapshot bytes differ from signed SHA-256"));
    }

    // Still no full-DAG, monetary replay, persistence, restore or launch GO.
    verdict.snapshot_bytes_verified = true;
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::{Cursor, Error, ErrorKind};

    fn signed_fixture(
        snapshot: &[u8],
    ) -> (
        NativeV3CheckpointSignedClaim,
        NativeV3ExternallyTrustedCheckpointPin,
    ) {
        let key = SigningKey::from_bytes(&[27_u8; 32]);
        let digest: [u8; 32] = Sha256::digest(snapshot).into();
        let mut claim = NativeV3CheckpointSignedClaim {
            chain_id: "pulsedag-public-testnet-v3".into(),
            genesis_hash: [11; 32],
            checkpoint_hash: [12; 32],
            selected_tip_hash: [13; 32],
            snapshot_sha256: digest,
            consensus_digest: [14; 32],
            monetary_digest: [15; 32],
            checkpoint_height: 100,
            blue_score: 90,
            sequence: 3,
            claimed_public_key: key.verifying_key().to_bytes(),
            signature: [0; 64],
        };
        let preimage =
            crate::native_v3_checkpoint_external_signature::dormant_native_v3_checkpoint_signature_preimage(
                &claim,
            )
            .unwrap();
        claim.signature = key.sign(&preimage).to_bytes();
        let pin = NativeV3ExternallyTrustedCheckpointPin {
            chain_id: claim.chain_id.clone(),
            genesis_hash: claim.genesis_hash,
            signer_public_key: claim.claimed_public_key,
            checkpoint_hash: claim.checkpoint_hash,
            selected_tip_hash: claim.selected_tip_hash,
            snapshot_sha256: claim.snapshot_sha256,
            consensus_digest: claim.consensus_digest,
            monetary_digest: claim.monetary_digest,
            minimum_height: 99,
            minimum_blue_score: 89,
            minimum_sequence: 3,
        };
        (claim, pin)
    }

    #[test]
    fn actual_signed_snapshot_stream_proves_only_digest_not_consensus_or_launch() {
        let bytes = b"synthetic signed native-v3 checkpoint bytes, no monetary authority";
        let (claim, pin) = signed_fixture(bytes);
        let verdict = verify_dormant_native_v3_checkpoint_snapshot_bytes(
            &claim,
            &pin,
            None,
            Cursor::new(bytes),
            bytes.len() as u64,
        )
        .unwrap();
        assert!(verdict.signature_matches_pinned_key);
        assert!(verdict.snapshot_bytes_verified);
        assert!(!verdict.full_dag_consensus_verified);
        assert!(!verdict.monetary_replay_verified);
        assert!(!verdict.checkpoint_restore_authorized);
        assert!(!verdict.testnet_or_mainnet_launch_authorized);
    }

    #[test]
    fn mutated_truncated_empty_and_oversized_snapshot_bytes_fail_closed() {
        let bytes = b"fixed signed checkpoint snapshot bytes";
        let (claim, pin) = signed_fixture(bytes);
        let mut mutated = bytes.to_vec();
        mutated[3] ^= 1;
        for rejected in [
            mutated,
            bytes[..bytes.len() - 1].to_vec(),
            Vec::new(),
            [&bytes[..], &b"-extra"[..]].concat(),
        ] {
            assert!(verify_dormant_native_v3_checkpoint_snapshot_bytes(
                &claim,
                &pin,
                None,
                Cursor::new(&rejected),
                1024,
            )
            .is_err());
        }
        for max in [0, 1, DORMANT_CHECKPOINT_SNAPSHOT_MAX_BYTES + 1] {
            assert!(verify_dormant_native_v3_checkpoint_snapshot_bytes(
                &claim,
                &pin,
                None,
                Cursor::new(bytes),
                max,
            )
            .is_err());
        }
    }

    #[test]
    fn forged_signature_wrong_independent_pin_and_rollback_fail_before_read() {
        struct NeverRead;
        impl Read for NeverRead {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("untrusted snapshot must not be opened before signature passes")
            }
        }

        let bytes = b"fixed signed snapshot";
        let (claim, pin) = signed_fixture(bytes);
        let mut forged = claim.clone();
        forged.signature[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_snapshot_bytes(
            &forged, &pin, None, NeverRead, 1024,
        )
        .is_err());
        let mut wrong_pin = pin.clone();
        wrong_pin.consensus_digest[0] ^= 1;
        assert!(verify_dormant_native_v3_checkpoint_snapshot_bytes(
            &claim, &wrong_pin, None, NeverRead, 1024,
        )
        .is_err());
        let last = NativeV3PreviousAuthenticatedPosition {
            height: 100,
            blue_score: 90,
            sequence: 3,
        };
        assert!(verify_dormant_native_v3_checkpoint_snapshot_bytes(
            &claim,
            &pin,
            Some(last),
            NeverRead,
            1024,
        )
        .is_err());
    }

    #[test]
    fn untrusted_reader_error_must_fail_even_if_valid_prefix_matches() {
        struct InterruptedRead {
            sent: bool,
        }
        impl Read for InterruptedRead {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.sent {
                    return Err(Error::new(ErrorKind::BrokenPipe, "untrusted stream broken"));
                }
                self.sent = true;
                buf[0] = b'x';
                Ok(1)
            }
        }
        let bytes = b"test snapshot bytes";
        let (claim, pin) = signed_fixture(bytes);
        assert!(verify_dormant_native_v3_checkpoint_snapshot_bytes(
            &claim,
            &pin,
            None,
            InterruptedRead { sent: false },
            1024,
        )
        .is_err());
    }
}
