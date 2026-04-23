use super::mining_template::load_template;
use crate::api::{ApiResponse, RpcStateLike, SubmitMinedBlockRequest};
use axum::{extract::State, Json};
use pulsedag_core::{
    accept_block, adopt_ready_orphans, dev_pow_accepts, dev_target_u64, preferred_tip_hash,
    AcceptSource,
};

#[derive(Debug, serde::Serialize)]
pub struct MiningSubmitData {
    pub accepted: bool,
    pub block_hash: String,
    pub height: u64,
    pub pow_algorithm: String,
    pub pow_accepted_dev: bool,
    pub target_u64: u64,
    pub stale_template: bool,
    pub selected_tip: Option<String>,
    pub adopted_orphans: usize,
}

fn persist_then_broadcast_mined_block<FPersist, FBroadcast>(
    block: &pulsedag_core::types::Block,
    chain: &pulsedag_core::state::ChainState,
    persist: FPersist,
    broadcast: FBroadcast,
) -> Result<(), pulsedag_core::errors::PulseError>
where
    FPersist: FnOnce(
        &pulsedag_core::types::Block,
        &pulsedag_core::state::ChainState,
    ) -> Result<(), pulsedag_core::errors::PulseError>,
    FBroadcast: FnOnce(&pulsedag_core::types::Block),
{
    persist(block, chain)?;
    broadcast(block);
    Ok(())
}

pub async fn post_mining_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitMinedBlockRequest>,
) -> Json<ApiResponse<MiningSubmitData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let pow_accepted_dev = dev_pow_accepts(&req.block.header);
    let target_u64 = dev_target_u64(req.block.header.difficulty as u64);
    let block_hash = req.block.hash.clone();
    let height = req.block.header.height;

    if !pow_accepted_dev {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.rejected_mined_blocks += 1;
        return Json(ApiResponse::err(
            "INVALID_POW",
            "submitted block does not satisfy current dev pow check".to_string(),
        ));
    }

    if height <= chain.dag.best_height {
        return Json(ApiResponse::err(
            "STALE_TEMPLATE",
            format!(
                "stale template: current best height is {} and submitted block height is {}",
                chain.dag.best_height, height
            ),
        ));
    }

    let mut current_parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
    current_parents.sort();
    let current_selected_tip = preferred_tip_hash(&chain);
    let expected_template_id = format!(
        "{}:{}",
        chain.dag.best_height + 1,
        current_parents.join(",")
    );

    if let Some(template_id) = req.template_id.as_ref() {
        if template_id != &expected_template_id {
            if let Some(stored) = load_template(template_id) {
                if stored.height != chain.dag.best_height + 1 {
                    return Json(ApiResponse::err(
                        "STALE_TEMPLATE",
                        format!(
                            "template height {} is stale; current next height is {}",
                            stored.height,
                            chain.dag.best_height + 1
                        ),
                    ));
                }
                if stored.parent_hashes != current_parents {
                    return Json(ApiResponse::err(
                        "STALE_TEMPLATE",
                        "template parents no longer match current tips",
                    ));
                }
                if stored.selected_tip != current_selected_tip {
                    return Json(ApiResponse::err(
                        "STALE_TEMPLATE",
                        "template selected_tip no longer matches current preferred tip",
                    ));
                }
            } else {
                return Json(ApiResponse::err(
                    "UNKNOWN_TEMPLATE",
                    format!("template_id {} not found", template_id),
                ));
            }
        }
    }

    let mut submitted_parents = req.block.header.parents.clone();
    submitted_parents.sort();
    if submitted_parents != current_parents {
        return Json(ApiResponse::err(
            "STALE_TEMPLATE",
            "submitted block parents no longer match current tip set",
        ));
    }

    match accept_block(req.block.clone(), &mut chain, AcceptSource::Rpc) {
        Ok(_) => {
            let adopted_orphans = adopt_ready_orphans(&mut chain, AcceptSource::Rpc);

            if let Err(e) = persist_then_broadcast_mined_block(
                &req.block,
                &chain,
                |block, chain| {
                    state.storage().persist_block(block)?;
                    state.storage().persist_chain_state(chain)?;
                    Ok(())
                },
                |block| {
                    if let Some(p2p) = state.p2p() {
                        let _ = p2p.broadcast_block(block);
                    }
                },
            ) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }

            {
                let runtime_handle = state.runtime();
                let mut runtime = runtime_handle.write().await;
                runtime.accepted_mined_blocks += 1;
                runtime.adopted_orphan_blocks += adopted_orphans as u64;
            }

            Json(ApiResponse::ok(MiningSubmitData {
                accepted: true,
                block_hash,
                height,
                pow_algorithm: pulsedag_core::selected_pow_name().to_string(),
                pow_accepted_dev,
                target_u64,
                stale_template: false,
                selected_tip: preferred_tip_hash(&chain),
                adopted_orphans,
            }))
        }
        Err(e) => {
            let runtime_handle = state.runtime();
            let mut runtime = runtime_handle.write().await;
            runtime.rejected_mined_blocks += 1;
            Json(ApiResponse::err("SUBMIT_BLOCK_ERROR", e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{persist_then_broadcast_mined_block, post_mining_submit};
    use crate::api::{NodeRuntimeStats, RpcStateLike, SubmitMinedBlockRequest};
    use axum::{extract::State, Json};
    use pulsedag_core::{
        build_candidate_block, build_coinbase_transaction, dev_difficulty_snapshot,
        dev_mine_header,
        errors::PulseError,
        state::ChainState,
        types::{Block, Transaction},
    };
    use pulsedag_p2p::{P2pHandle, P2pStatus};
    use pulsedag_storage::Storage;
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    #[derive(Clone)]
    struct FakeP2pHandle {
        broadcast_block_calls: Arc<AtomicUsize>,
    }

    impl FakeP2pHandle {
        fn new() -> Self {
            Self {
                broadcast_block_calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn block_calls(&self) -> usize {
            self.broadcast_block_calls.load(Ordering::SeqCst)
        }
    }

    impl P2pHandle for FakeP2pHandle {
        fn broadcast_transaction(&self, _tx: &Transaction) -> Result<(), PulseError> {
            Ok(())
        }

        fn broadcast_block(&self, _block: &Block) -> Result<(), PulseError> {
            self.broadcast_block_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn status(&self) -> Result<P2pStatus, PulseError> {
            Ok(P2pStatus {
                mode: "test".into(),
                peer_id: "fake".into(),
                listening: vec![],
                connected_peers: vec![],
                topics: vec![],
                mdns: false,
                kademlia: false,
                broadcasted_messages: self.block_calls(),
                publish_attempts: self.block_calls(),
                seen_message_ids: self.block_calls(),
                queued_messages: 0,
                inbound_messages: 0,
                runtime_started: true,
                runtime_mode_detail: "unit-test".into(),
                swarm_events_seen: 0,
                subscriptions_active: 0,
                last_message_kind: Some("block".into()),
                last_swarm_event: None,
                per_topic_publishes: std::collections::HashMap::new(),
                inbound_decode_failed: 0,
                inbound_chain_mismatch_dropped: 0,
                inbound_duplicates_suppressed: 0,
                tx_outbound_duplicates_suppressed: 0,
                tx_outbound_first_seen_relayed: 0,
                last_drop_reason: None,
                peer_reconnect_attempts: 0,
                peer_recovery_success_count: 0,
                last_peer_recovery_unix: None,
                peer_cooldown_suppressed_count: 0,
                peer_flap_suppressed_count: 0,
                peers_under_cooldown: 0,
                peers_under_flap_guard: 0,
                peer_recovery: vec![],
                sync_candidates: vec![],
                selected_sync_peer: None,
            })
        }
    }

    #[derive(Clone)]
    struct TestState {
        chain: Arc<RwLock<ChainState>>,
        storage: Arc<Storage>,
        p2p: Option<Arc<dyn P2pHandle>>,
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

    fn temp_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pulsedag-{name}-{unique}"))
    }

    fn build_state_with_fake_p2p() -> (TestState, FakeP2pHandle) {
        let path = temp_db_path("mining-submit-tests");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let fake = FakeP2pHandle::new();
        let state = TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            p2p: Some(Arc::new(fake.clone())),
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        };
        (state, fake)
    }

    async fn build_mined_block(state: &TestState) -> Block {
        let chain = state.chain.read().await;
        let height = chain.dag.best_height + 1;
        let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        let difficulty =
            u32::try_from(dev_difficulty_snapshot(&chain).suggested_difficulty).unwrap_or(u32::MAX);
        let txs = vec![build_coinbase_transaction("kaspa:qptestminer", 50, height)];
        let mut block = build_candidate_block(parents, height, difficulty, txs);
        let (mined_header, mined, _, _) = dev_mine_header(block.header.clone(), 100_000);
        assert!(mined, "expected test block to satisfy dev pow");
        block.header = mined_header;
        block
    }

    #[tokio::test]
    async fn accepted_block_broadcasts_to_p2p() {
        let (state, fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;

        let Json(response) = post_mining_submit(
            State(state),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block,
            }),
        )
        .await;

        assert!(response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);
    }

    #[tokio::test]
    async fn rejected_block_does_not_broadcast() {
        let (state, fake_p2p) = build_state_with_fake_p2p();
        let block = build_mined_block(&state).await;

        let Json(first_response) = post_mining_submit(
            State(state.clone()),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block: block.clone(),
            }),
        )
        .await;
        assert!(first_response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);

        let Json(second_response) = post_mining_submit(
            State(state),
            Json(SubmitMinedBlockRequest {
                template_id: None,
                block,
            }),
        )
        .await;

        assert!(!second_response.ok);
        assert_eq!(fake_p2p.block_calls(), 1);
    }

    #[test]
    fn persist_error_does_not_broadcast() {
        let path = temp_db_path("persist-error-tests");
        let storage = Arc::new(Storage::open(path.to_str().unwrap()).unwrap());
        let chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let height = chain.dag.best_height + 1;
        let mut parents = chain.dag.tips.iter().cloned().collect::<Vec<_>>();
        parents.sort();
        let difficulty =
            u32::try_from(dev_difficulty_snapshot(&chain).suggested_difficulty).unwrap_or(u32::MAX);
        let txs = vec![build_coinbase_transaction("kaspa:qptestminer", 50, height)];
        let block = build_candidate_block(parents, height, difficulty, txs);
        let broadcasts = Arc::new(AtomicUsize::new(0));
        let broadcast_counter = broadcasts.clone();

        let result = persist_then_broadcast_mined_block(
            &block,
            &chain,
            |_block, _chain| Err(PulseError::StorageError("forced persist failure".into())),
            |_block| {
                broadcast_counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_err());
        assert_eq!(broadcasts.load(Ordering::SeqCst), 0);
    }
}
