use crate::api::{ApiResponse, RpcStateLike, SubmitTxRequest};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use pulsedag_wallet::{build_transaction, BuildTxRequest};

#[derive(Debug, serde::Serialize)]
pub struct TxListItem {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
}
#[derive(Debug, serde::Serialize)]
pub struct TxListData {
    pub count: usize,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
    pub transactions: Vec<TxListItem>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TxsQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TxsPageQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolData {
    pub transaction_count: usize,
    pub orphan_transaction_count: usize,
    pub orphan_limit: usize,
    pub spent_outpoints_count: usize,
    pub orphaned_total: u64,
    pub orphan_promoted_total: u64,
    pub orphan_dropped_total: u64,
    pub orphan_pruned_total: u64,
    pub txids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TxValidateData {
    pub valid: bool,
    pub txid: String,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TxDetailData {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
    pub status: String,
    pub is_mempool: bool,
    pub is_confirmed: bool,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct TxLookupData {
    pub txid: String,
    pub status: String,
    pub is_mempool: bool,
    pub is_confirmed: bool,
    pub fee: u64,
    pub nonce: u64,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
    pub confirmations: Option<u64>,
    pub inputs: Vec<pulsedag_core::types::OutPoint>,
    pub outputs: Vec<pulsedag_core::types::TxOutput>,
}

#[derive(Debug, serde::Serialize)]
pub struct TxActivityItem {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
    pub context: String,
    pub is_mempool: bool,
    pub is_confirmed: bool,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct TxActivityData {
    pub count: usize,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
    pub transactions: Vec<TxActivityItem>,
}

fn sorted_mempool_transactions(chain: &pulsedag_core::ChainState) -> Vec<TxListItem> {
    let mut transactions = chain
        .mempool
        .transactions
        .values()
        .map(|tx| TxListItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
        })
        .collect::<Vec<_>>();
    transactions.sort_by(|a, b| b.fee.cmp(&a.fee).then_with(|| a.txid.cmp(&b.txid)));
    transactions
}

fn sorted_tx_activity(chain: &pulsedag_core::ChainState) -> Vec<TxActivityItem> {
    let mut activity = chain
        .mempool
        .transactions
        .values()
        .map(|tx| TxActivityItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
            context: "mempool".to_string(),
            is_mempool: true,
            is_confirmed: false,
            block_hash: None,
            block_height: None,
        })
        .collect::<Vec<_>>();
    for block in chain.dag.blocks.values() {
        for tx in &block.transactions {
            activity.push(TxActivityItem {
                txid: tx.txid.clone(),
                fee: tx.fee,
                inputs: tx.inputs.len(),
                outputs: tx.outputs.len(),
                context: "confirmed".to_string(),
                is_mempool: false,
                is_confirmed: true,
                block_hash: Some(block.hash.clone()),
                block_height: Some(block.header.height),
            });
        }
    }
    activity.sort_by(|a, b| {
        b.is_mempool
            .cmp(&a.is_mempool)
            .then_with(|| b.block_height.cmp(&a.block_height))
            .then_with(|| b.fee.cmp(&a.fee))
            .then_with(|| a.txid.cmp(&b.txid))
    });
    activity
}

fn paged_txs(all: Vec<TxListItem>, limit: usize, offset: usize) -> TxListData {
    let total = all.len();
    let transactions = all.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
    let count = transactions.len();
    let has_more = offset.saturating_add(count) < total;
    TxListData {
        count,
        total,
        limit,
        offset,
        has_more,
        transactions,
    }
}

pub async fn get_txs_recent<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<TxsQuery>,
) -> Json<ApiResponse<TxListData>> {
    let limit = query.limit.unwrap_or(10).min(100);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let transactions = sorted_mempool_transactions(&chain);
    Json(ApiResponse::ok(paged_txs(transactions, limit, 0)))
}

pub async fn get_txs<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<TxListData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let transactions = sorted_mempool_transactions(&chain);
    let total = transactions.len();
    Json(ApiResponse::ok(TxListData {
        count: total,
        total,
        limit: total,
        offset: 0,
        has_more: false,
        transactions,
    }))
}

pub async fn get_mempool<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MempoolData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut txids = chain
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    txids.sort();
    Json(ApiResponse::ok(MempoolData {
        transaction_count: chain.mempool.transactions.len(),
        orphan_transaction_count: chain.mempool.orphan_transactions.len(),
        orphan_limit: chain.mempool.max_orphans,
        spent_outpoints_count: chain.mempool.spent_outpoints.len(),
        orphaned_total: chain.mempool.counters.orphaned_total,
        orphan_promoted_total: chain.mempool.counters.orphan_promoted_total,
        orphan_dropped_total: chain.mempool.counters.orphan_dropped_total,
        orphan_pruned_total: chain.mempool.counters.orphan_pruned_total,
        txids,
    }))
}

pub async fn get_tx<S: RpcStateLike>(
    State(state): State<S>,
    Path(txid): Path<String>,
) -> Json<ApiResponse<TxDetailData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    if let Some(tx) = chain.mempool.transactions.get(&txid) {
        return Json(ApiResponse::ok(TxDetailData {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
            status: "mempool".into(),
            is_mempool: true,
            is_confirmed: false,
            block_hash: None,
            block_height: None,
        }));
    }

    for block in chain.dag.blocks.values() {
        if let Some(tx) = block.transactions.iter().find(|t| t.txid == txid) {
            return Json(ApiResponse::ok(TxDetailData {
                txid: tx.txid.clone(),
                fee: tx.fee,
                inputs: tx.inputs.len(),
                outputs: tx.outputs.len(),
                status: "confirmed".into(),
                is_mempool: false,
                is_confirmed: true,
                block_hash: Some(block.hash.clone()),
                block_height: Some(block.header.height),
            }));
        }
    }

    Json(ApiResponse::err(
        "TX_NOT_FOUND",
        format!("transaction {txid} not found"),
    ))
}

pub async fn post_tx_build<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<BuildTxRequest>,
) -> Json<ApiResponse<pulsedag_wallet::BuildTxResponse>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let available = chain
        .utxo
        .address_index
        .get(&req.from)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|op| chain.utxo.utxos.get(&op).cloned())
        .collect::<Vec<_>>();
    match build_transaction(&req.from, &req.to, req.amount, req.fee, &available, 1) {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(e) => Json(ApiResponse::err("BUILD_ERROR", e.to_string())),
    }
}

pub async fn post_tx_validate<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<ApiResponse<TxValidateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut simulated = chain.clone();
    drop(chain);

    match pulsedag_core::accept_transaction(
        req.transaction.clone(),
        &mut simulated,
        pulsedag_core::AcceptSource::Rpc,
    ) {
        Ok(_) => Json(ApiResponse::ok(TxValidateData {
            valid: true,
            txid: req.transaction.txid,
            reason: None,
        })),
        Err(e) => Json(ApiResponse::ok(TxValidateData {
            valid: false,
            txid: req.transaction.txid,
            reason: Some(e.to_string()),
        })),
    }
}

pub async fn post_tx_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    match pulsedag_core::accept_transaction(
        req.transaction.clone(),
        &mut chain,
        pulsedag_core::AcceptSource::Rpc,
    ) {
        Ok(_) => {
            let mempool_size = chain.mempool.transactions.len();
            let snapshot = chain.clone();
            drop(chain);
            if let Err(e) = state.storage().persist_chain_state(&snapshot) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Some(p2p) = state.p2p() {
                let _ = p2p.broadcast_transaction(&req.transaction);
            }
            Json(ApiResponse::ok(
                serde_json::json!({"accepted": true, "txid": req.transaction.txid, "mempool_size": mempool_size}),
            ))
        }
        Err(e) => Json(ApiResponse::err("TX_REJECTED", e.to_string())),
    }
}

pub async fn get_txs_page<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<TxsPageQuery>,
) -> Json<ApiResponse<TxListData>> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let transactions = sorted_mempool_transactions(&chain);
    Json(ApiResponse::ok(paged_txs(transactions, limit, offset)))
}

pub async fn get_tx_lookup<S: RpcStateLike>(
    State(state): State<S>,
    Path(txid): Path<String>,
) -> Json<ApiResponse<TxLookupData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    if let Some(tx) = chain.mempool.transactions.get(&txid) {
        return Json(ApiResponse::ok(TxLookupData {
            txid: tx.txid.clone(),
            status: "mempool".into(),
            is_mempool: true,
            is_confirmed: false,
            fee: tx.fee,
            nonce: tx.nonce,
            block_hash: None,
            block_height: None,
            confirmations: None,
            inputs: tx
                .inputs
                .iter()
                .map(|i| i.previous_output.clone())
                .collect(),
            outputs: tx.outputs.clone(),
        }));
    }

    for block in chain.dag.blocks.values() {
        if let Some(tx) = block.transactions.iter().find(|t| t.txid == txid) {
            return Json(ApiResponse::ok(TxLookupData {
                txid: tx.txid.clone(),
                status: "confirmed".into(),
                is_mempool: false,
                is_confirmed: true,
                fee: tx.fee,
                nonce: tx.nonce,
                block_hash: Some(block.hash.clone()),
                block_height: Some(block.header.height),
                confirmations: Some(chain.dag.best_height.saturating_sub(block.header.height) + 1),
                inputs: tx
                    .inputs
                    .iter()
                    .map(|i| i.previous_output.clone())
                    .collect(),
                outputs: tx.outputs.clone(),
            }));
        }
    }

    Json(ApiResponse::err(
        "TX_NOT_FOUND",
        format!("transaction {txid} not found"),
    ))
}

pub async fn get_txs_activity<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<TxsPageQuery>,
) -> Json<ApiResponse<TxActivityData>> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let all = sorted_tx_activity(&chain);
    let total = all.len();
    let transactions = all.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
    let count = transactions.len();
    let has_more = offset.saturating_add(count) < total;
    Json(ApiResponse::ok(TxActivityData {
        count,
        total,
        limit,
        offset,
        has_more,
        transactions,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        get_tx, get_tx_lookup, get_txs, get_txs_activity, get_txs_page, TxListData, TxsPageQuery,
    };
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::extract::{Path, State};
    use pulsedag_core::types::{
        Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput, Utxo,
    };
    use pulsedag_core::ChainState;
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
        storage: Arc<Storage>,
        runtime: Arc<RwLock<NodeRuntimeStats>>,
    }

    impl RpcStateLike for TestState {
        fn chain(&self) -> Arc<RwLock<ChainState>> {
            self.chain.clone()
        }
        fn p2p(&self) -> Option<Arc<dyn pulsedag_p2p::P2pHandle>> {
            None
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

    async fn mk_state() -> TestState {
        let path = temp_db_path("tx-handler");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let mut chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let outpoint = OutPoint {
            txid: "funding".into(),
            index: 0,
        };
        chain.utxo.utxos.insert(
            outpoint.clone(),
            Utxo {
                outpoint: outpoint.clone(),
                address: "alice".into(),
                amount: 50,
                coinbase: false,
                height: 1,
            },
        );
        chain
            .utxo
            .address_index
            .insert("alice".into(), vec![outpoint.clone()]);

        let mempool_tx = Transaction {
            txid: "tx-mempool".into(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: outpoint,
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![TxOutput {
                address: "bob".into(),
                amount: 45,
            }],
            fee: 5,
            nonce: 7,
        };
        chain
            .mempool
            .transactions
            .insert(mempool_tx.txid.clone(), mempool_tx.clone());

        let confirmed_tx = Transaction {
            txid: "tx-confirmed".into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "alice".into(),
                amount: 10,
            }],
            fee: 0,
            nonce: 9,
        };
        let genesis = chain.dag.genesis_hash.clone();
        let block = Block {
            hash: "block-1".into(),
            header: BlockHeader {
                version: 1,
                parents: vec![genesis.clone()],
                timestamp: 3,
                difficulty: 1,
                nonce: 1,
                merkle_root: "m".into(),
                state_root: "s".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![confirmed_tx],
        };
        chain
            .dag
            .children
            .entry(genesis.clone())
            .or_default()
            .push(block.hash.clone());
        chain.dag.tips.remove(&genesis);
        chain.dag.tips.insert(block.hash.clone());
        chain.dag.best_height = 1;
        chain.dag.blocks.insert(block.hash.clone(), block);

        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        }
    }

    #[tokio::test]
    async fn tx_lookup_reports_mempool_and_confirmed_sources() {
        let state = mk_state().await;
        let axum::Json(mem_resp) =
            get_tx_lookup(State(state.clone()), Path("tx-mempool".to_string())).await;
        let mem = mem_resp.data.expect("mempool tx lookup");
        assert_eq!(mem.status, "mempool");
        assert!(mem.is_mempool);
        assert!(!mem.is_confirmed);
        assert!(mem.block_hash.is_none());

        let axum::Json(conf_resp) =
            get_tx_lookup(State(state), Path("tx-confirmed".to_string())).await;
        let conf = conf_resp.data.expect("confirmed tx lookup");
        assert_eq!(conf.status, "confirmed");
        assert!(!conf.is_mempool);
        assert!(conf.is_confirmed);
        assert_eq!(conf.confirmations, Some(1));
    }

    #[tokio::test]
    async fn existing_tx_surfaces_remain_compatible() {
        let state = mk_state().await;
        let axum::Json(list_resp) = get_txs(State(state.clone())).await;
        let list: TxListData = list_resp.data.expect("tx list");
        assert_eq!(list.count, 1);
        assert_eq!(list.total, 1);

        let axum::Json(detail_resp) = get_tx(State(state), Path("tx-confirmed".to_string())).await;
        let detail = detail_resp.data.expect("tx detail");
        assert_eq!(detail.status, "confirmed");
        assert!(detail.is_confirmed);
        assert!(!detail.is_mempool);
        assert_eq!(detail.block_height, Some(1));
    }

    #[tokio::test]
    async fn tx_page_metadata_is_deterministic() {
        let state = mk_state().await;
        let axum::Json(resp) = get_txs_page(
            State(state),
            axum::extract::Query(TxsPageQuery {
                limit: Some(1),
                offset: Some(0),
            }),
        )
        .await;
        let data = resp.data.expect("tx page");
        assert_eq!(data.count, 1);
        assert_eq!(data.total, 1);
        assert_eq!(data.limit, 1);
        assert_eq!(data.offset, 0);
        assert!(!data.has_more);
    }

    #[tokio::test]
    async fn tx_activity_is_deterministic_and_context_explicit() {
        let state = mk_state().await;
        let query = axum::extract::Query(TxsPageQuery {
            limit: Some(10),
            offset: Some(0),
        });
        let axum::Json(first_resp) = get_txs_activity(State(state.clone()), query).await;
        let first = first_resp.data.expect("first activity page");
        let axum::Json(second_resp) = get_txs_activity(
            State(state),
            axum::extract::Query(TxsPageQuery {
                limit: Some(10),
                offset: Some(0),
            }),
        )
        .await;
        let second = second_resp.data.expect("second activity page");

        assert!(first.count >= 2);
        assert_eq!(first.total, first.transactions.len());
        assert!(first.transactions.len() >= 2);
        assert_eq!(
            first
                .transactions
                .iter()
                .map(|t| t.txid.clone())
                .collect::<Vec<_>>(),
            second
                .transactions
                .iter()
                .map(|t| t.txid.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(first.transactions[0].context, "mempool");
        assert!(first.transactions[0].is_mempool);
        assert!(!first.transactions[0].is_confirmed);
        assert!(first
            .transactions
            .iter()
            .any(|tx| { tx.context == "confirmed" && !tx.is_mempool && tx.is_confirmed }));
    }
}
