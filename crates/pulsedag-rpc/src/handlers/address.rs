use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{
    extract::{Path, State},
    Json,
};
use pulsedag_core::types::Utxo;

#[derive(Debug, serde::Serialize)]
pub struct AddressOutpointData {
    pub txid: String,
    pub index: u32,
    pub amount: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct AddressData {
    pub address: String,
    pub balance: u64,
    pub utxo_count: usize,
    pub confirmed_balance: u64,
    pub confirmed_utxo_count: usize,
    pub largest_utxo: u64,
    pub outpoints: Vec<AddressOutpointData>,
}
#[derive(Debug, serde::Serialize)]
pub struct AddressUtxosData {
    pub address: String,
    pub count: usize,
    pub utxos: Vec<Utxo>,
}
#[derive(Debug, serde::Serialize)]
pub struct UtxoListData {
    pub count: usize,
    pub utxos: Vec<Utxo>,
}
#[derive(Debug, serde::Serialize)]
pub struct AddressSummaryData {
    pub address: String,
    pub confirmed_balance: u64,
    pub confirmed_utxo_count: usize,
    pub pending_incoming: u64,
    pub pending_outgoing: u64,
    pub pending_net: i64,
    pub mempool_tx_count: usize,
    pub mempool_txids: Vec<String>,
    pub mempool_explicit: bool,
}

fn sorted_address_utxos(chain: &pulsedag_core::ChainState, address: &str) -> Vec<Utxo> {
    let mut utxos = chain
        .utxo
        .address_index
        .get(address)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|op| chain.utxo.utxos.get(&op).cloned())
        .collect::<Vec<_>>();
    utxos.sort_by(|a, b| {
        a.outpoint
            .txid
            .cmp(&b.outpoint.txid)
            .then_with(|| a.outpoint.index.cmp(&b.outpoint.index))
    });
    utxos
}

pub async fn get_address<S: RpcStateLike>(
    State(state): State<S>,
    Path(address): Path<String>,
) -> Json<ApiResponse<AddressData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let utxos = sorted_address_utxos(&chain, &address);
    let balance = utxos.iter().map(|u| u.amount).sum();
    let largest_utxo = utxos.iter().map(|u| u.amount).max().unwrap_or(0);
    let outpoints = utxos
        .iter()
        .map(|u| AddressOutpointData {
            txid: u.outpoint.txid.clone(),
            index: u.outpoint.index,
            amount: u.amount,
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::ok(AddressData {
        address,
        balance,
        utxo_count: utxos.len(),
        confirmed_balance: balance,
        confirmed_utxo_count: utxos.len(),
        largest_utxo,
        outpoints,
    }))
}

pub async fn get_address_utxos<S: RpcStateLike>(
    State(state): State<S>,
    Path(address): Path<String>,
) -> Json<ApiResponse<AddressUtxosData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let utxos = sorted_address_utxos(&chain, &address);
    Json(ApiResponse::ok(AddressUtxosData {
        address,
        count: utxos.len(),
        utxos,
    }))
}

pub async fn get_utxos<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<UtxoListData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut utxos = chain.utxo.utxos.values().cloned().collect::<Vec<_>>();
    utxos.sort_by(|a, b| {
        a.outpoint
            .txid
            .cmp(&b.outpoint.txid)
            .then_with(|| a.outpoint.index.cmp(&b.outpoint.index))
    });
    Json(ApiResponse::ok(UtxoListData {
        count: utxos.len(),
        utxos,
    }))
}

pub async fn get_address_summary<S: RpcStateLike>(
    State(state): State<S>,
    Path(address): Path<String>,
) -> Json<ApiResponse<AddressSummaryData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let utxos = sorted_address_utxos(&chain, &address);
    let confirmed_balance = utxos.iter().map(|u| u.amount).sum();

    let mut pending_incoming = 0u64;
    let mut pending_outgoing = 0u64;
    let mut mempool_txids = Vec::new();

    for tx in chain.mempool.transactions.values() {
        let incoming = tx
            .outputs
            .iter()
            .filter(|out| out.address == address)
            .map(|out| out.amount)
            .sum::<u64>();
        let outgoing = tx
            .inputs
            .iter()
            .filter_map(|input| chain.utxo.utxos.get(&input.previous_output))
            .filter(|spent| spent.address == address)
            .map(|spent| spent.amount)
            .sum::<u64>();
        if incoming > 0 || outgoing > 0 {
            mempool_txids.push(tx.txid.clone());
            pending_incoming = pending_incoming.saturating_add(incoming);
            pending_outgoing = pending_outgoing.saturating_add(outgoing);
        }
    }
    mempool_txids.sort();

    Json(ApiResponse::ok(AddressSummaryData {
        address,
        confirmed_balance,
        confirmed_utxo_count: utxos.len(),
        pending_incoming,
        pending_outgoing,
        pending_net: pending_incoming as i64 - pending_outgoing as i64,
        mempool_tx_count: mempool_txids.len(),
        mempool_txids,
        mempool_explicit: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::{get_address, get_address_summary, get_address_utxos};
    use crate::api::{NodeRuntimeStats, RpcStateLike};
    use axum::extract::{Path, State};
    use pulsedag_core::types::{OutPoint, Transaction, TxInput, TxOutput, Utxo};
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
        let path = temp_db_path("address-handler");
        let storage = Arc::new(Storage::open(path.to_str().expect("utf8 temp path")).unwrap());
        let mut chain = storage
            .load_or_init_genesis("testnet-dev".to_string())
            .unwrap();

        let op = OutPoint {
            txid: "funding".into(),
            index: 0,
        };
        chain.utxo.utxos.insert(
            op.clone(),
            Utxo {
                outpoint: op.clone(),
                address: "alice".into(),
                amount: 50,
                coinbase: false,
                height: 1,
            },
        );
        chain
            .utxo
            .address_index
            .insert("alice".into(), vec![op.clone()]);

        let tx = Transaction {
            txid: "tx-pending".into(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: op,
                public_key: "pk".into(),
                signature: "sig".into(),
            }],
            outputs: vec![
                TxOutput {
                    address: "alice".into(),
                    amount: 20,
                },
                TxOutput {
                    address: "bob".into(),
                    amount: 25,
                },
            ],
            fee: 5,
            nonce: 1,
        };
        chain.mempool.transactions.insert(tx.txid.clone(), tx);
        TestState {
            chain: Arc::new(RwLock::new(chain)),
            storage,
            runtime: Arc::new(RwLock::new(NodeRuntimeStats::default())),
        }
    }

    #[tokio::test]
    async fn address_summary_matches_confirmed_and_pending_state() {
        let state = mk_state().await;
        let axum::Json(resp) = get_address_summary(State(state), Path("alice".to_string())).await;
        let data = resp.data.expect("address summary");
        assert_eq!(data.confirmed_balance, 50);
        assert_eq!(data.pending_incoming, 20);
        assert_eq!(data.pending_outgoing, 50);
        assert_eq!(data.pending_net, -30);
        assert_eq!(data.mempool_tx_count, 1);
    }

    #[tokio::test]
    async fn existing_address_endpoint_remains_compatible() {
        let state = mk_state().await;
        let axum::Json(resp) = get_address(State(state), Path("alice".to_string())).await;
        let data = resp.data.expect("address data");
        assert_eq!(data.balance, 50);
        assert_eq!(data.utxo_count, 1);
        assert_eq!(data.confirmed_balance, 50);
        assert_eq!(data.confirmed_utxo_count, 1);
    }

    #[tokio::test]
    async fn address_surfaces_are_coherent_for_confirmed_view() {
        let state = mk_state().await;
        let axum::Json(address_resp) =
            get_address(State(state.clone()), Path("alice".to_string())).await;
        let address = address_resp.data.expect("address");
        let axum::Json(summary_resp) =
            get_address_summary(State(state.clone()), Path("alice".to_string())).await;
        let summary = summary_resp.data.expect("summary");
        let axum::Json(utxos_resp) =
            get_address_utxos(State(state), Path("alice".to_string())).await;
        let utxos = utxos_resp.data.expect("utxos");

        assert_eq!(address.confirmed_balance, summary.confirmed_balance);
        assert_eq!(address.confirmed_utxo_count, summary.confirmed_utxo_count);
        assert_eq!(utxos.count, summary.confirmed_utxo_count);
        assert!(summary.mempool_explicit);
    }
}
