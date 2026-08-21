use serde::{Deserialize, Serialize};

use pulsedag_core::{
    address_from_public_key, compute_submission_id_v2, compute_txid_v2,
    errors::PulseError,
    genesis::init_chain_state,
    signing_message_v2,
    types::{Address, Transaction, Utxo},
    ProtocolActivationIdentity, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V2,
    TRANSACTION_VERSION_V2,
};
use pulsedag_core::validation_v2::validate_transaction_v2;

use crate::{build_transaction_v2, SelectedUtxo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletV2SigningPlan {
    pub protocol_identity: ProtocolActivationIdentity,
    pub protocol_fingerprint: String,
    pub from: Address,
    /// Unsigned transaction with input public keys already attached. The txid
    /// remains empty until offline signatures are attached because signatures
    /// are part of the canonical v2 transaction bytes.
    pub transaction: Transaction,
    pub selected_utxos: Vec<Utxo>,
    pub total_input: u64,
    pub change: u64,
    /// Hex-encoded canonical `PulseDAG:unsigned-tx:v2` bytes.
    pub signing_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletV2SignedSubmission {
    pub protocol_identity: ProtocolActivationIdentity,
    pub protocol_fingerprint: String,
    pub transaction: Transaction,
    pub selected_utxos: Vec<Utxo>,
    pub submission_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletV2BroadcastEnvelope {
    pub protocol_fingerprint: String,
    pub transaction: Transaction,
    pub submission_id: String,
}

fn wallet_protocol_error(message: impl Into<String>) -> PulseError {
    PulseError::InvalidTransaction(format!("wallet v2 protocol: {}", message.into()))
}

/// Verify that the node identity observed immediately before a wallet action is
/// the exact identity the wallet intends to use. This is deliberately stricter
/// than checking only `chain_id`: genesis, transaction/header versions,
/// consensus mode and DAG-ordering identity must all match.
pub fn verify_wallet_v2_node_identity(
    expected: &ProtocolActivationIdentity,
    observed: &ProtocolActivationIdentity,
) -> Result<String, PulseError> {
    expected.validate().map_err(wallet_protocol_error)?;
    observed.validate().map_err(wallet_protocol_error)?;

    if expected.transaction_protocol_version != TRANSACTION_VERSION_V2
        || expected.block_header_protocol_version != BLOCK_HEADER_VERSION_V2
        || expected.consensus_mode != ProtocolConsensusMode::GhostdagV1
    {
        return Err(wallet_protocol_error(
            "expected identity is not an activated-v2 protocol identity",
        ));
    }

    if observed != expected {
        return Err(wallet_protocol_error("node protocol identity mismatch"));
    }

    expected.fingerprint().map_err(wallet_protocol_error)
}

fn selected_full_utxos(
    selected: &[SelectedUtxo],
    available: &[Utxo],
) -> Result<Vec<Utxo>, PulseError> {
    selected
        .iter()
        .map(|entry| {
            available
                .iter()
                .find(|candidate| candidate.outpoint == entry.outpoint)
                .cloned()
                .ok_or_else(|| wallet_protocol_error("selected UTXO disappeared from wallet view"))
        })
        .collect()
}

/// Build a deterministic activated-v2 signing plan after verifying the exact
/// node protocol identity. UTXOs are sorted canonically before selection so a
/// node/API response order change cannot silently change the wallet plan.
///
/// The public key is attached before deriving the signing message because it is
/// part of the canonical unsigned transaction. No private-key material enters
/// this API; signing can happen in an encrypted keystore, hardware signer or
/// fully offline process.
pub fn prepare_wallet_v2_signing_plan(
    expected_identity: &ProtocolActivationIdentity,
    observed_node_identity: &ProtocolActivationIdentity,
    public_key: &str,
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<WalletV2SigningPlan, PulseError> {
    let protocol_fingerprint =
        verify_wallet_v2_node_identity(expected_identity, observed_node_identity)?;

    if address_from_public_key(public_key) != from {
        return Err(PulseError::InvalidSignature);
    }

    let mut canonical_available = available_utxos
        .iter()
        .filter(|utxo| utxo.address == from)
        .cloned()
        .collect::<Vec<_>>();
    canonical_available.sort_by(|a, b| {
        a.outpoint
            .txid
            .cmp(&b.outpoint.txid)
            .then_with(|| a.outpoint.index.cmp(&b.outpoint.index))
            .then_with(|| a.amount.cmp(&b.amount))
    });

    let built = build_transaction_v2(
        &expected_identity.chain_id,
        from,
        to,
        amount,
        fee,
        &canonical_available,
        nonce,
    )?;
    let selected_utxos = selected_full_utxos(&built.selected_utxos, &canonical_available)?;

    let mut transaction = built.transaction;
    for input in &mut transaction.inputs {
        input.public_key = public_key.to_string();
        input.signature.clear();
    }
    // The builder's unsigned txid was computed before public keys/signatures
    // were attached. It is not a final v2 consensus txid.
    transaction.txid.clear();
    let signing_message = signing_message_v2(&transaction, &expected_identity.chain_id)?;

    Ok(WalletV2SigningPlan {
        protocol_identity: expected_identity.clone(),
        protocol_fingerprint,
        from: from.to_string(),
        transaction,
        selected_utxos,
        total_input: built.total_input,
        change: built.change,
        signing_message: hex::encode(signing_message),
    })
}

fn validation_state_for_selected_utxos(
    plan_chain_id: &str,
    selected: &[Utxo],
) -> pulsedag_core::ChainState {
    let mut state = init_chain_state(plan_chain_id.to_string());
    for utxo in selected {
        state
            .utxo
            .utxos
            .insert(utxo.outpoint.clone(), utxo.clone());
        state
            .utxo
            .address_index
            .entry(utxo.address.clone())
            .or_default()
            .push(utxo.outpoint.clone());
    }
    state
}

/// Attach signatures produced by an external/offline signer and locally
/// validate the final canonical transaction before producing a submission
/// identity. One signature is required per input.
pub fn finalize_wallet_v2_signed_plan(
    plan: &WalletV2SigningPlan,
    signatures: &[String],
) -> Result<WalletV2SignedSubmission, PulseError> {
    verify_wallet_v2_node_identity(&plan.protocol_identity, &plan.protocol_identity)?;

    let expected_message = signing_message_v2(&plan.transaction, &plan.protocol_identity.chain_id)?;
    if hex::encode(&expected_message) != plan.signing_message {
        return Err(wallet_protocol_error(
            "signing plan bytes changed after preparation",
        ));
    }
    if signatures.len() != plan.transaction.inputs.len() {
        return Err(wallet_protocol_error(format!(
            "expected {} input signatures, got {}",
            plan.transaction.inputs.len(),
            signatures.len()
        )));
    }

    let mut transaction = plan.transaction.clone();
    for (input, signature) in transaction.inputs.iter_mut().zip(signatures) {
        let signature_bytes = hex::decode(signature).map_err(|_| PulseError::InvalidSignature)?;
        if signature_bytes.len() != 64 {
            return Err(PulseError::InvalidSignature);
        }
        input.signature = signature.clone();
    }
    transaction.txid = compute_txid_v2(&transaction, &plan.protocol_identity.chain_id)?;

    let validation_state = validation_state_for_selected_utxos(
        &plan.protocol_identity.chain_id,
        &plan.selected_utxos,
    );
    validate_transaction_v2(
        &transaction,
        &validation_state,
        &plan.protocol_identity.chain_id,
    )?;

    let submission_id = compute_submission_id_v2(&transaction, &plan.protocol_identity.chain_id)?;

    Ok(WalletV2SignedSubmission {
        protocol_identity: plan.protocol_identity.clone(),
        protocol_fingerprint: plan.protocol_fingerprint.clone(),
        transaction,
        selected_utxos: plan.selected_utxos.clone(),
        submission_id,
    })
}

/// Re-check node identity and the complete signed transaction immediately
/// before transport/broadcast. Returning this envelope is the authorization
/// boundary; callers should not broadcast a v2 transaction without it.
pub fn prepare_wallet_v2_broadcast(
    expected_identity: &ProtocolActivationIdentity,
    observed_node_identity: &ProtocolActivationIdentity,
    signed: &WalletV2SignedSubmission,
) -> Result<WalletV2BroadcastEnvelope, PulseError> {
    let protocol_fingerprint =
        verify_wallet_v2_node_identity(expected_identity, observed_node_identity)?;
    if signed.protocol_identity != *expected_identity
        || signed.protocol_fingerprint != protocol_fingerprint
    {
        return Err(wallet_protocol_error(
            "signed submission protocol identity changed before broadcast",
        ));
    }

    let expected_txid = compute_txid_v2(&signed.transaction, &expected_identity.chain_id)?;
    if signed.transaction.txid != expected_txid {
        return Err(PulseError::InvalidTxid);
    }

    let validation_state =
        validation_state_for_selected_utxos(&expected_identity.chain_id, &signed.selected_utxos);
    validate_transaction_v2(
        &signed.transaction,
        &validation_state,
        &expected_identity.chain_id,
    )?;

    let expected_submission_id =
        compute_submission_id_v2(&signed.transaction, &expected_identity.chain_id)?;
    if signed.submission_id != expected_submission_id {
        return Err(wallet_protocol_error("submission identity mismatch"));
    }

    Ok(WalletV2BroadcastEnvelope {
        protocol_fingerprint,
        transaction: signed.transaction.clone(),
        submission_id: signed.submission_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{address_from_public_key, types::OutPoint};

    const PUBLIC_KEY: &str =
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
    const SIGNATURE: &str =
        "f3d0895cc46bcc7b3655d607588b98543b0e15c3589592f290147ae5072a0375a0933bbb97304309af21f534d5f2f992bfa16bfed7bfd8b7a5f5098e9a34cc02";

    fn identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(chain_id, "genesis-v2", "ghostdag-order-v1")
    }

    fn source_address() -> String {
        address_from_public_key(PUBLIC_KEY)
    }

    fn utxo(txid: &str, index: u32, amount: u64) -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: txid.to_string(),
                index,
            },
            address: source_address(),
            amount,
            coinbase: false,
            height: 1,
        }
    }

    fn plan() -> WalletV2SigningPlan {
        let identity = identity("pulsedag-testnet");
        prepare_wallet_v2_signing_plan(
            &identity,
            &identity,
            PUBLIC_KEY,
            &source_address(),
            "pulse1recipient",
            7,
            1,
            &[utxo("funding", 0, 10)],
            9,
        )
        .expect("v2 plan")
    }

    #[test]
    fn exact_node_identity_is_required_before_plan_creation() {
        let expected = identity("pulsedag-testnet");
        let observed = identity("pulsedag-private");
        let err = prepare_wallet_v2_signing_plan(
            &expected,
            &observed,
            PUBLIC_KEY,
            &source_address(),
            "pulse1recipient",
            7,
            1,
            &[utxo("funding", 0, 10)],
            9,
        )
        .unwrap_err();

        assert!(matches!(err, PulseError::InvalidTransaction(_)));
    }

    #[test]
    fn wallet_plan_attaches_public_key_before_signing_and_has_no_final_txid() {
        let plan = plan();
        assert!(plan.transaction.txid.is_empty());
        assert_eq!(plan.transaction.version, TRANSACTION_VERSION_V2);
        assert_eq!(plan.transaction.inputs[0].public_key, PUBLIC_KEY);
        assert!(plan.transaction.inputs[0].signature.is_empty());
        assert_eq!(
            plan.signing_message,
            hex::encode(
                signing_message_v2(&plan.transaction, &plan.protocol_identity.chain_id).unwrap()
            )
        );
    }

    #[test]
    fn wallet_plan_selection_is_stable_across_utxo_response_order() {
        let identity = identity("pulsedag-testnet");
        let source = source_address();
        let a = utxo("a", 0, 5);
        let b = utxo("b", 0, 5);

        let first = prepare_wallet_v2_signing_plan(
            &identity,
            &identity,
            PUBLIC_KEY,
            &source,
            "pulse1recipient",
            7,
            1,
            &[b.clone(), a.clone()],
            9,
        )
        .unwrap();
        let second = prepare_wallet_v2_signing_plan(
            &identity,
            &identity,
            PUBLIC_KEY,
            &source,
            "pulse1recipient",
            7,
            1,
            &[a, b],
            9,
        )
        .unwrap();

        assert_eq!(first.transaction.inputs, second.transaction.inputs);
        assert_eq!(first.signing_message, second.signing_message);
        assert_eq!(first.selected_utxos, second.selected_utxos);
    }

    #[test]
    fn offline_signature_finalizes_chain_bound_txid_and_submission_id() {
        let plan = plan();
        let signed = finalize_wallet_v2_signed_plan(&plan, &[SIGNATURE.to_string()])
            .expect("offline signature validates");

        assert_eq!(
            signed.transaction.txid,
            compute_txid_v2(&signed.transaction, "pulsedag-testnet").unwrap()
        );
        assert_eq!(
            signed.submission_id,
            compute_submission_id_v2(&signed.transaction, "pulsedag-testnet").unwrap()
        );
        assert_eq!(signed.transaction.inputs[0].signature, SIGNATURE);
    }

    #[test]
    fn broadcast_rechecks_node_identity_and_signed_transaction() {
        let plan = plan();
        let signed = finalize_wallet_v2_signed_plan(&plan, &[SIGNATURE.to_string()]).unwrap();
        let expected = identity("pulsedag-testnet");

        let envelope = prepare_wallet_v2_broadcast(&expected, &expected, &signed)
            .expect("exact identity authorizes broadcast");
        assert_eq!(envelope.transaction.txid, signed.transaction.txid);
        assert_eq!(envelope.submission_id, signed.submission_id);

        let wrong = identity("pulsedag-private");
        assert!(prepare_wallet_v2_broadcast(&expected, &wrong, &signed).is_err());
    }

    #[test]
    fn invalid_external_signature_fails_before_broadcast() {
        let plan = plan();
        let err = finalize_wallet_v2_signed_plan(&plan, &["00".repeat(64)]).unwrap_err();
        assert!(matches!(err, PulseError::InvalidSignature));
    }
}
