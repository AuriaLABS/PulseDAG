use sha2::{Digest, Sha256};

const SNAPSHOT_TRANSFER_PAYLOAD_DOMAIN_V1: &[u8] = b"PulseDAG:snapshot-transfer-payload:v1";
const SNAPSHOT_TRANSFER_CHUNK_DOMAIN_V1: &[u8] = b"PulseDAG:snapshot-transfer-chunk:v1";

fn update_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    let len = u64::try_from(bytes.len()).expect("snapshot transfer field length exceeds u64::MAX");
    hasher.update(len.to_le_bytes());
    hasher.update(bytes);
}

/// SHA-256 commitment for one exact serialized fast-sync payload.
///
/// The domain is distinct from consensus transaction, block, Merkle and state
/// hashing so transfer integrity cannot be confused with a consensus identity.
pub fn snapshot_transfer_payload_digest_v1(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SNAPSHOT_TRANSFER_PAYLOAD_DOMAIN_V1);
    update_len_prefixed(&mut hasher, payload);
    hex::encode(hasher.finalize())
}

/// SHA-256 commitment for one indexed chunk of an already committed payload.
///
/// Binding the whole-payload transfer id and chunk index prevents a valid chunk
/// from one transfer or position from being replayed into another position.
pub fn snapshot_transfer_chunk_digest_v1(
    transfer_id: &str,
    chunk_index: u32,
    chunk: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SNAPSHOT_TRANSFER_CHUNK_DOMAIN_V1);
    update_len_prefixed(&mut hasher, transfer_id.as_bytes());
    hasher.update(chunk_index.to_le_bytes());
    update_len_prefixed(&mut hasher, chunk);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_digest_is_stable_and_domain_separated_from_chunks() {
        let payload = b"snapshot-payload";
        let transfer_id = snapshot_transfer_payload_digest_v1(payload);
        assert_eq!(transfer_id, snapshot_transfer_payload_digest_v1(payload));
        assert_ne!(
            transfer_id,
            snapshot_transfer_chunk_digest_v1(&transfer_id, 0, payload)
        );
    }

    #[test]
    fn chunk_digest_binds_transfer_index_and_bytes() {
        let transfer_a = snapshot_transfer_payload_digest_v1(b"transfer-a");
        let transfer_b = snapshot_transfer_payload_digest_v1(b"transfer-b");
        let chunk = b"chunk";
        let digest = snapshot_transfer_chunk_digest_v1(&transfer_a, 7, chunk);

        assert_ne!(
            digest,
            snapshot_transfer_chunk_digest_v1(&transfer_b, 7, chunk)
        );
        assert_ne!(
            digest,
            snapshot_transfer_chunk_digest_v1(&transfer_a, 8, chunk)
        );
        assert_ne!(
            digest,
            snapshot_transfer_chunk_digest_v1(&transfer_a, 7, b"changed")
        );
    }
}
