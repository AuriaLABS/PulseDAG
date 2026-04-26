use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{
    extract::{Path, Query, State},
    Json,
};

#[derive(Debug, serde::Serialize)]
pub struct BlockListItem {
    pub hash: String,
    pub height: u64,
    pub blue_score: u64,
    pub tx_count: usize,
    pub timestamp: u64,
    pub parent_count: usize,
}

#[derive(Debug, serde::Deserialize)]
pub struct ListQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
pub struct PageQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct BlocksData {
    pub count: usize,
    pub blocks: Vec<BlockListItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct BlockOverviewData {
    pub hash: String,
    pub height: u64,
    pub blue_score: u64,
    pub timestamp: u64,
    pub parent_hashes: Vec<String>,
    pub child_hashes: Vec<String>,
    pub tx_count: usize,
    pub txids: Vec<String>,
    pub is_tip: bool,
    pub selected_tip: Option<String>,
    pub confirmations: u64,
}

fn bounded_limit(limit: Option<usize>, default: usize, max: usize) -> usize {
    limit.unwrap_or(default).min(max)
}

pub async fn get_blocks<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<BlocksData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    Json(ApiResponse::ok(BlocksData {
        count: blocks.len(),
        blocks,
    }))
}

pub async fn get_blocks_latest<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<BlockListItem>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match chain.dag.blocks.values().max_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
    }) {
        Some(b) => Json(ApiResponse::ok(BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })),
        None => Json(ApiResponse::err("NOT_FOUND", "no blocks found")),
    }
}

pub async fn get_blocks_recent<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<ListQuery>,
) -> Json<ApiResponse<BlocksData>> {
    let limit = bounded_limit(query.limit, 10, 100);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    blocks.truncate(limit);
    Json(ApiResponse::ok(BlocksData {
        count: blocks.len(),
        blocks,
    }))
}

pub async fn get_blocks_page<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<PageQuery>,
) -> Json<ApiResponse<BlocksData>> {
    let limit = bounded_limit(query.limit, 20, 100);
    let offset = query.offset.unwrap_or(0);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    let blocks = blocks
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Json(ApiResponse::ok(BlocksData {
        count: blocks.len(),
        blocks,
    }))
}

pub async fn get_block_overview<S: RpcStateLike>(
    State(state): State<S>,
    Path(hash): Path<String>,
) -> Json<ApiResponse<BlockOverviewData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let selected_tip = pulsedag_core::preferred_tip_hash(&chain);

    match chain.dag.blocks.get(&hash) {
        Some(block) => {
            let child_hashes = chain.dag.children.get(&hash).cloned().unwrap_or_default();
            let txids = block
                .transactions
                .iter()
                .map(|tx| tx.txid.clone())
                .collect::<Vec<_>>();
            let confirmations = chain.dag.best_height.saturating_sub(block.header.height) + 1;
            Json(ApiResponse::ok(BlockOverviewData {
                hash: block.hash.clone(),
                height: block.header.height,
                blue_score: block.header.blue_score,
                timestamp: block.header.timestamp,
                parent_hashes: block.header.parents.clone(),
                child_hashes,
                tx_count: block.transactions.len(),
                txids,
                is_tip: chain.dag.tips.contains(&hash),
                selected_tip,
                confirmations,
            }))
        }
        None => Json(ApiResponse::err("NOT_FOUND", "block not found")),
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_limit, get_block_overview, get_blocks, get_blocks_recent};
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::extract::{Path, Query, State};
    use proptest::prelude::*;
    use pulsedag_core::types::{Block, BlockHeader, Transaction};
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
        let path = temp_db_path("blocks-handler");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let mut chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();
        let genesis = chain.dag.genesis_hash.clone();
        let block = Block {
            hash: "block-1".into(),
            header: BlockHeader {
                version: 1,
                parents: vec![genesis.clone()],
                timestamp: 2,
                difficulty: 1,
                nonce: 42,
                merkle_root: "m1".into(),
                state_root: "s1".into(),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![Transaction {
                txid: "tx-1".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 1,
                nonce: 1,
            }],
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

    proptest! {
        #[test]
        fn limit_normalization_is_capped_and_never_zero_for_defaults(raw in any::<usize>()) {
            let recent = bounded_limit(Some(raw), 10, 100);
            prop_assert!(recent <= 100);

            let page = bounded_limit(Some(raw), 20, 100);
            prop_assert!(page <= 100);
        }
    }

    #[test]
    fn limit_normalization_uses_defaults_when_missing() {
        assert_eq!(bounded_limit(None, 10, 100), 10);
        assert_eq!(bounded_limit(None, 20, 100), 20);
    }

    #[tokio::test]
    async fn block_overview_matches_chain_relations() {
        let state = mk_state().await;
        let axum::Json(resp) = get_block_overview(State(state), Path("block-1".to_string())).await;
        let data = resp.data.expect("overview data");
        assert_eq!(data.hash, "block-1");
        assert_eq!(data.parent_hashes.len(), 1);
        assert!(data.child_hashes.is_empty());
        assert_eq!(data.confirmations, 1);
        assert!(data.is_tip);
        assert_eq!(data.txids, vec!["tx-1".to_string()]);
    }

    #[tokio::test]
    async fn existing_block_surfaces_stay_compatible() {
        let state = mk_state().await;
        let axum::Json(all_resp) = get_blocks(State(state.clone())).await;
        assert!(all_resp.ok);
        assert!(all_resp.data.expect("blocks data").count >= 2);

        let axum::Json(recent_resp) =
            get_blocks_recent(State(state), Query(super::ListQuery { limit: Some(1) })).await;
        let recent = recent_resp.data.expect("recent blocks");
        assert_eq!(recent.count, 1);
    }
}
