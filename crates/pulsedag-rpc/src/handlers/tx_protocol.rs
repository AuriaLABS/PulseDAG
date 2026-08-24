use crate::{
    api::{ApiResponse, RpcStateLike, SubmitTxRequest},
    tx_rejection::classify_rpc_transaction_acceptance,
};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_transaction, accept_transaction_for_protocol, accept_transaction_with_result,
    accept_transaction_with_result_for_protocol, compute_submission_id_v2,
    tx_protocol::resolve_transaction_validation_path, AcceptSource, ChainState,
    ProtocolActivationIdentity, PulseError, TransactionValidationPath, TxAcceptanceResult,
};

pub use super::tx_legacy::{
    get_mempool, get_tx, get_tx_lookup, get_txs, get_txs_activity, get_txs_page, get_txs_recent,
    post_tx_build, MempoolData, TxActivityData, TxActivityItem, TxDetailData, TxListData,
    TxListItem, TxLookupData, TxValidateData, TxsPageQuery, TxsQuery,
};

fn rpc_protocol_identity<S: RpcStateLike>(
    state: &S,
) -> Result<Option<ProtocolActivationIdentity>, PulseError> {
    super::mining_submit_protocol::rpc_protocol_identity(state)
}

fn accept_rpc_transaction(
    transaction: pulsedag_core::types::Transaction,
    chain: &mut ChainState,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<(), PulseError> {
    match identity {
        Some(identity) => {
            accept_transaction_for_protocol(transaction, chain, AcceptSource::Rpc, identity)
        }
        None => accept_transaction(transaction, chain, AcceptSource::Rpc),
    }
}

fn accept_rpc_transaction_with_result(
    transaction: pulsedag_core::types::Transaction,
    chain: &mut ChainState,
    identity: Option<&ProtocolActivationIdentity>,
) -> TxAcceptanceResult {
    match identity {
        Some(identity) => accept_transaction_with_result_for_protocol(
            transaction,
            chain,
            AcceptSource::Rpc,
            identity,
        ),
        None => accept_transaction_with_result(transaction, chain, AcceptSource::Rpc),
    }
}

fn legacy_orphan_compatibility(
    chain: &ChainState,
    identity: Option<&ProtocolActivationIdentity>,
) -> bool {
    match identity {
        None => true,
        Some(identity) => matches!(
            resolve_transaction_validation_path(identity, chain),
            Ok(TransactionValidationPath::LegacyV1)
        ),
    }
}

fn prospective_v2_submission_id(
    transaction: &pulsedag_core::types::Transaction,
    chain: &ChainState,
    identity: Option<&ProtocolActivationIdentity>,
) -> Option<String> {
    let identity = identity?;
    if transaction.version != pulsedag_core::TRANSACTION_VERSION_V2
        || !matches!(
            resolve_transaction_validation_path(identity, chain),
            Ok(TransactionValidationPath::ActivatedV2)
        )
    {
        return None;
    }
    compute_submission_id_v2(transaction, &identity.chain_id).ok()
}

fn rejection_reason(result: &TxAcceptanceResult) -> String {
    match result {
        TxAcceptanceResult::Duplicate => "transaction already exists".to_string(),
        TxAcceptanceResult::Orphan => "transaction inputs are not yet available".to_string(),
        TxAcceptanceResult::Invalid(reason) | TxAcceptanceResult::Rejected(reason) => {
            reason.clone()
        }
        TxAcceptanceResult::Accepted => "transaction accepted".to_string(),
    }
}

fn classified_rejection(
    transaction: &pulsedag_core::types::Transaction,
    chain: &ChainState,
    identity: Option<&ProtocolActivationIdentity>,
    result: &TxAcceptanceResult,
) -> ApiResponse<serde_json::Value> {
    let reason = rejection_reason(result);
    match classify_rpc_transaction_acceptance(transaction, chain, identity, result) {
        Some(classification) => {
            ApiResponse::err_classified("TX_REJECTED", reason, classification.as_str())
        }
        None => ApiResponse::err("TX_REJECTED", reason),
    }
}

pub async fn post_tx_validate<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<ApiResponse<TxValidateData>> {
    let txid = req.transaction.txid.clone();
    let identity = match rpc_protocol_identity(&state) {
        Ok(identity) => identity,
        Err(error) => {
            return Json(ApiResponse::ok(TxValidateData {
                valid: false,
                txid,
                reason: Some(error.to_string()),
            }));
        }
    };

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut simulated = chain.clone();
    drop(chain);

    match accept_rpc_transaction(req.transaction, &mut simulated, identity.as_ref()) {
        Ok(()) => Json(ApiResponse::ok(TxValidateData {
            valid: true,
            txid,
            reason: None,
        })),
        Err(error) => Json(ApiResponse::ok(TxValidateData {
            valid: false,
            txid,
            reason: Some(error.to_string()),
        })),
    }
}

pub async fn post_tx_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let transaction = req.transaction;
    let txid = transaction.txid.clone();
    let identity = match rpc_protocol_identity(&state) {
        Ok(identity) => identity,
        Err(error) => return Json(ApiResponse::err("TX_REJECTED", error.to_string())),
    };

    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let submission_id = prospective_v2_submission_id(&transaction, &chain, identity.as_ref());
    let legacy_orphan = legacy_orphan_compatibility(&chain, identity.as_ref());
    let result =
        accept_rpc_transaction_with_result(transaction.clone(), &mut chain, identity.as_ref());

    if matches!(
        &result,
        TxAcceptanceResult::Accepted | TxAcceptanceResult::Orphan
    ) {
        let mempool_size = chain.mempool.transactions.len();
        let snapshot = chain.clone();
        let response = match &result {
            TxAcceptanceResult::Accepted => {
                let mut data = serde_json::json!({
                    "accepted": true,
                    "txid": txid,
                    "mempool_size": mempool_size
                });
                if let Some(submission_id) = submission_id {
                    data["status"] = serde_json::json!("accepted");
                    data["submission_id"] = serde_json::json!(submission_id);
                }
                ApiResponse::ok(data)
            }
            TxAcceptanceResult::Orphan if legacy_orphan => ApiResponse::ok(serde_json::json!({
                "accepted": true,
                "txid": txid,
                "mempool_size": mempool_size
            })),
            TxAcceptanceResult::Orphan => {
                classified_rejection(&transaction, &snapshot, identity.as_ref(), &result)
            }
            _ => unreachable!("persist/relay branch only accepts accepted or orphan"),
        };
        drop(chain);
        if let Err(error) = state.storage().persist_chain_state(&snapshot) {
            return Json(ApiResponse::err("STORAGE_ERROR", error.to_string()));
        }
        if let Some(p2p) = state.p2p() {
            let _ = p2p.broadcast_transaction(&transaction);
        }
        return Json(response);
    }

    Json(classified_rejection(
        &transaction,
        &chain,
        identity.as_ref(),
        &result,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::NodeRuntimeStats;
    use ed25519_dalek::{Signer, SigningKey};
    use pulsedag_core::{
        address_from_public_key, compute_submission_id_v2, compute_txid, compute_txid_v2,
        genesis::init_chain_state, signing_message, signing_message_v2, OutPoint, Transaction,
        TxInput, TxOutput, Utxo, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V1,
        TRANSACTION_VERSION_V2,
    };
    use pulsedag_p2p::{
        messages::{ProtocolCapabilitiesV1, P2P_PROTOCOL_CAPABILITIES_VERSION},
        MemoryP2pHandle, P2pHandle, P2pStatus,
    };
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        p2p: Option<Arc<dyn P2pHandle>>,
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }

        fn p2p(&self) -> Option<Arc<dyn P2pHandle>> {
            self.p2p.clone()
        }

        fn storage(&self) -> Arc<Storage> {
            self.storage.clone()
        }

        fn runtime(&self) -> Arc<RwLock<NodeRuntimeStats>> {
            self.runtime.clone()
        }
    }

    struct FailingCapabilitiesP2p;

    impl P2pHandle for FailingCapabilitiesP2p {
        fn local_protocol_capabilities_v1(
            &self,
        ) -> Result<Option<ProtocolCapabilitiesV1>, PulseError> {
            Err(PulseError::Internal(
                "task28 capability identity unavailable".to_string(),
            ))
        }

        fn broadcast_transaction(&self, _tx: &Transaction) -> Result<(), PulseError> {
            Ok(())
        }

        fn broadcast_block(&self, _block: &pulsedag_core::types::Block) -> Result<(), PulseError> {
            Ok(())
        }

        fn status(&self) -> Result<P2pStatus, PulseError> {
            Ok(P2pStatus::default())
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    fn test_state(chain: ChainState, p2p: Option<Arc<dyn P2pHandle>>, name: &str) -> TestState {
        let path = temp_db_path(name);
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            p2p,
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        }
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public_key_hex(signing_key: &SigningKey) -> String {
        hex::encode(signing_key.verifying_key().to_bytes())
    }

    fn fund_key(
        chain: &mut ChainState,
        txid: &str,
        signing_key: &SigningKey,
        amount: u64,
    ) -> OutPoint {
        let public_key = public_key_hex(signing_key);
        let address = address_from_public_key(&public_key);
        let outpoint = OutPoint {
            txid: txid.to_string(),
            index: 0,
        };
        chain.utxo.utxos.insert(
            outpoint.clone(),
            Utxo {
                outpoint: outpoint.clone(),
                address: address.clone(),
                amount,
                coinbase: false,
                height: 1,
            },
        );
        chain
            .utxo
            .address_index
            .entry(address)
            .or_default()
            .push(outpoint.clone());
        outpoint
    }

    fn signed_transaction(
        signing_key: &SigningKey,
        previous_output: OutPoint,
        version: u32,
        nonce: u64,
        chain_id: &str,
    ) -> Transaction {
        let public_key = public_key_hex(signing_key);
        let mut transaction = Transaction {
            txid: String::new(),
            version,
            inputs: vec![TxInput {
                previous_output,
                public_key,
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1task28rpcrecipient".to_string(),
                amount: 9,
            }],
            fee: 1,
            nonce,
        };
        let message = match version {
            TRANSACTION_VERSION_V1 => signing_message(&transaction),
            TRANSACTION_VERSION_V2 => signing_message_v2(&transaction, chain_id).unwrap(),
            _ => panic!("unsupported test transaction version"),
        };
        transaction.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
        transaction.txid = match version {
            TRANSACTION_VERSION_V1 => compute_txid(&transaction),
            TRANSACTION_VERSION_V2 => compute_txid_v2(&transaction, chain_id).unwrap(),
            _ => unreachable!(),
        };
        transaction
    }

    fn activated_p2p(chain: &ChainState) -> (Arc<MemoryP2pHandle>, Arc<dyn P2pHandle>) {
        let (handle, _inbound_rx) = MemoryP2pHandle::new(chain.chain_id.clone(), Vec::new());
        handle
            .configure_protocol_capabilities_v1(ProtocolCapabilitiesV1 {
                capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
                protocol_identity: ProtocolActivationIdentity::activated_v2(
                    chain.chain_id.clone(),
                    chain.dag.genesis_hash.clone(),
                    GHOSTDAG_V1_ORDERING_VERSION,
                ),
                consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
                finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
                supports_dag_frontier: true,
                supports_consensus_metadata: true,
                high_cadence_allowed: false,
            })
            .expect("configure activated protocol capabilities");
        let handle = Arc::new(handle);
        let trait_handle: Arc<dyn P2pHandle> = handle.clone();
        (handle, trait_handle)
    }

    #[tokio::test]
    async fn validate_uses_explicit_v2_capability_identity() {
        let mut chain = init_chain_state("task28-rpc-v2-validate".to_string());
        let key = signing_key(21);
        let outpoint = fund_key(&mut chain, "v2-validate-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 1, &chain.chain_id);
        let (_handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-validate");

        let Json(response) =
            post_tx_validate(State(state), Json(SubmitTxRequest { transaction })).await;

        assert!(response.ok);
        let data = response.data.expect("validation response data");
        assert!(data.valid, "activated v2 transaction should validate");
        assert!(data.reason.is_none());
    }

    #[tokio::test]
    async fn validate_without_capabilities_keeps_legacy_v1_default() {
        let mut chain = init_chain_state("task28-rpc-legacy-default".to_string());
        let key = signing_key(22);
        let outpoint = fund_key(&mut chain, "legacy-default-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 2, &chain.chain_id);
        let state = test_state(chain, None, "task28-rpc-legacy-default");

        let Json(response) =
            post_tx_validate(State(state), Json(SubmitTxRequest { transaction })).await;

        assert!(response.ok);
        let data = response.data.expect("validation response data");
        assert!(
            !data.valid,
            "v2 must not activate when capabilities are absent"
        );
        assert!(data.reason.is_some());
    }

    #[tokio::test]
    async fn submit_rejects_v1_under_activated_v2_without_mempool_mutation() {
        let mut chain = init_chain_state("task28-rpc-v1-reject".to_string());
        let key = signing_key(23);
        let outpoint = fund_key(&mut chain, "v1-reject-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V1, 3, &chain.chain_id);
        let (_handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v1-reject");

        let Json(response) =
            post_tx_submit(State(state.clone()), Json(SubmitTxRequest { transaction })).await;

        assert!(!response.ok);
        let error = response.error.expect("submission rejection");
        assert_eq!(error.code, "TX_REJECTED");
        assert_eq!(
            error.classification.as_deref(),
            Some("inactive_transaction_version")
        );
        assert!(error.message.contains("requires transaction version 2"));
        assert!(state.chain.read().await.mempool.transactions.is_empty());
    }

    #[tokio::test]
    async fn submit_accepts_and_broadcasts_v2_under_activated_identity() {
        let mut chain = init_chain_state("task28-rpc-v2-submit".to_string());
        let key = signing_key(24);
        let outpoint = fund_key(&mut chain, "v2-submit-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 4, &chain.chain_id);
        let txid = transaction.txid.clone();
        let (handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-submit");

        let Json(response) =
            post_tx_submit(State(state.clone()), Json(SubmitTxRequest { transaction })).await;

        assert!(response.ok);
        assert!(state
            .chain
            .read()
            .await
            .mempool
            .transactions
            .contains_key(&txid));
        assert!(handle.status().unwrap().broadcasted_messages >= 1);
    }

    #[tokio::test]
    async fn accepted_v2_submit_echoes_stable_submission_id() {
        let mut chain = init_chain_state("task28-rpc-v2-submission-id".to_string());
        let key = signing_key(26);
        let outpoint = fund_key(&mut chain, "v2-submission-id-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 6, &chain.chain_id);
        let expected_submission_id =
            compute_submission_id_v2(&transaction, &chain.chain_id).unwrap();
        let (_handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-submission-id");

        let Json(response) =
            post_tx_submit(State(state), Json(SubmitTxRequest { transaction })).await;

        assert!(response.ok);
        let data = response.data.expect("v2 submit response data");
        assert_eq!(data["status"], serde_json::json!("accepted"));
        assert_eq!(
            data["submission_id"],
            serde_json::json!(expected_submission_id)
        );
    }

    #[tokio::test]
    async fn duplicate_v2_retry_is_machine_readable_and_not_rebroadcast() {
        let mut chain = init_chain_state("task28-rpc-v2-duplicate".to_string());
        let key = signing_key(27);
        let outpoint = fund_key(&mut chain, "v2-duplicate-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 7, &chain.chain_id);
        let (handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-duplicate");

        let Json(first) = post_tx_submit(
            State(state.clone()),
            Json(SubmitTxRequest {
                transaction: transaction.clone(),
            }),
        )
        .await;
        assert!(first.ok);
        let broadcasts_after_first = handle.status().unwrap().broadcasted_messages;

        let Json(second) =
            post_tx_submit(State(state.clone()), Json(SubmitTxRequest { transaction })).await;
        assert!(!second.ok);
        assert_eq!(
            second
                .error
                .expect("duplicate rejection")
                .classification
                .as_deref(),
            Some("duplicate")
        );
        assert_eq!(
            handle.status().unwrap().broadcasted_messages,
            broadcasts_after_first
        );
    }

    #[tokio::test]
    async fn conflicting_v2_retry_is_machine_readable() {
        let mut chain = init_chain_state("task28-rpc-v2-conflict".to_string());
        let key = signing_key(28);
        let outpoint = fund_key(&mut chain, "v2-conflict-funding", &key, 10);
        let first = signed_transaction(
            &key,
            outpoint.clone(),
            TRANSACTION_VERSION_V2,
            8,
            &chain.chain_id,
        );
        let second = signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 9, &chain.chain_id);
        let (_handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-conflict");

        let Json(first_response) = post_tx_submit(
            State(state.clone()),
            Json(SubmitTxRequest { transaction: first }),
        )
        .await;
        assert!(first_response.ok);

        let Json(second_response) = post_tx_submit(
            State(state.clone()),
            Json(SubmitTxRequest {
                transaction: second,
            }),
        )
        .await;
        assert!(!second_response.ok);
        assert_eq!(
            second_response
                .error
                .expect("conflict rejection")
                .classification
                .as_deref(),
            Some("conflict")
        );
    }

    #[tokio::test]
    async fn invalid_v2_signature_is_machine_readable() {
        let mut chain = init_chain_state("task28-rpc-v2-invalid-signature".to_string());
        let key = signing_key(29);
        let outpoint = fund_key(&mut chain, "v2-invalid-signature-funding", &key, 10);
        let chain_id = chain.chain_id.clone();
        let mut transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 10, &chain_id);
        transaction.inputs[0].signature = "00".repeat(64);
        transaction.txid = compute_txid_v2(&transaction, &chain_id).unwrap();
        let (_handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-invalid-signature");

        let Json(response) =
            post_tx_submit(State(state), Json(SubmitTxRequest { transaction })).await;
        assert!(!response.ok);
        assert_eq!(
            response
                .error
                .expect("invalid signature rejection")
                .classification
                .as_deref(),
            Some("invalid_signature")
        );
    }

    #[tokio::test]
    async fn v2_mempool_capacity_rejection_is_machine_readable() {
        let mut chain = init_chain_state("task28-rpc-v2-mempool-full".to_string());
        chain.mempool.max_spent_outpoints = 0;
        let key = signing_key(30);
        let outpoint = fund_key(&mut chain, "v2-mempool-full-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V2, 11, &chain.chain_id);
        let (_handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-mempool-full");

        let Json(response) =
            post_tx_submit(State(state), Json(SubmitTxRequest { transaction })).await;
        assert!(!response.ok);
        assert_eq!(
            response
                .error
                .expect("mempool capacity rejection")
                .classification
                .as_deref(),
            Some("mempool_full")
        );
    }

    #[tokio::test]
    async fn activated_v2_orphan_is_staged_but_not_reported_as_accepted() {
        let chain = init_chain_state("task28-rpc-v2-orphan".to_string());
        let key = signing_key(31);
        let missing = OutPoint {
            txid: "missing-v2-funding".to_string(),
            index: 0,
        };
        let transaction =
            signed_transaction(&key, missing, TRANSACTION_VERSION_V2, 12, &chain.chain_id);
        let txid = transaction.txid.clone();
        let (handle, p2p) = activated_p2p(&chain);
        let state = test_state(chain, Some(p2p), "task28-rpc-v2-orphan");

        let Json(response) =
            post_tx_submit(State(state.clone()), Json(SubmitTxRequest { transaction })).await;

        assert!(!response.ok);
        assert_eq!(
            response
                .error
                .expect("orphan rejection")
                .classification
                .as_deref(),
            Some("orphan")
        );
        assert!(state
            .chain
            .read()
            .await
            .mempool
            .orphan_transactions
            .contains_key(&txid));
        assert!(handle.status().unwrap().broadcasted_messages >= 1);
    }

    #[tokio::test]
    async fn legacy_orphan_keeps_historical_success_contract() {
        let chain = init_chain_state("task28-rpc-v1-orphan-compat".to_string());
        let key = signing_key(32);
        let missing = OutPoint {
            txid: "missing-v1-funding".to_string(),
            index: 0,
        };
        let transaction =
            signed_transaction(&key, missing, TRANSACTION_VERSION_V1, 13, &chain.chain_id);
        let txid = transaction.txid.clone();
        let state = test_state(chain, None, "task28-rpc-v1-orphan-compat");

        let Json(response) =
            post_tx_submit(State(state.clone()), Json(SubmitTxRequest { transaction })).await;

        assert!(response.ok);
        assert!(state
            .chain
            .read()
            .await
            .mempool
            .orphan_transactions
            .contains_key(&txid));
    }

    #[tokio::test]
    async fn capability_lookup_error_fails_closed_before_mempool_mutation() {
        let mut chain = init_chain_state("task28-rpc-capability-error".to_string());
        let key = signing_key(25);
        let outpoint = fund_key(&mut chain, "capability-error-funding", &key, 10);
        let transaction =
            signed_transaction(&key, outpoint, TRANSACTION_VERSION_V1, 5, &chain.chain_id);
        let p2p: Arc<dyn P2pHandle> = Arc::new(FailingCapabilitiesP2p);
        let state = test_state(chain, Some(p2p), "task28-rpc-capability-error");

        let Json(response) =
            post_tx_submit(State(state.clone()), Json(SubmitTxRequest { transaction })).await;

        assert!(!response.ok);
        let error = response.error.expect("capability lookup rejection");
        assert_eq!(error.code, "TX_REJECTED");
        assert!(error.classification.is_none());
        assert!(error
            .message
            .contains("task28 capability identity unavailable"));
        assert!(state.chain.read().await.mempool.transactions.is_empty());
    }
}
