use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use pulsedag_core::{observe_pulse_v1, state::ChainState, PulseObservationV1};

#[derive(Debug, serde::Serialize)]
pub struct DashboardBlockItem {
    pub hash: String,
    pub height: u64,
    pub tx_count: usize,
    pub timestamp: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardTxItem {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct DashboardPulse {
    pub pulse_height: u64,
    pub pulse_time: i64,
    pub uncertainty_secs: u32,
    pub finality_lag: u64,
    pub selected_tip: String,
}

impl From<&PulseObservationV1> for DashboardPulse {
    fn from(pulse: &PulseObservationV1) -> Self {
        Self {
            pulse_height: pulse.pulse_height,
            pulse_time: pulse.pulse_time,
            uncertainty_secs: pulse.uncertainty_secs,
            finality_lag: pulse.finality_lag,
            selected_tip: pulse.selected_tip.clone(),
        }
    }
}

fn observational_pulse(chain: &ChainState) -> Option<DashboardPulse> {
    observe_pulse_v1(chain)
        .ok()
        .map(|pulse| DashboardPulse::from(&pulse))
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardSummary {
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub circulating_supply: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pulse: Option<DashboardPulse>,
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardData {
    pub summary: DashboardSummary,
    pub latest_blocks: Vec<DashboardBlockItem>,
    pub mempool_transactions: Vec<DashboardTxItem>,
}

pub async fn get_dashboard<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<DashboardData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    let mut latest_blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| DashboardBlockItem {
            hash: b.hash.clone(),
            height: b.header.height,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
        })
        .collect::<Vec<_>>();
    latest_blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    latest_blocks.truncate(10);

    let mut mempool_transactions = chain
        .mempool
        .transactions
        .values()
        .map(|tx| DashboardTxItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
        })
        .collect::<Vec<_>>();
    mempool_transactions.sort_by_key(|tx| std::cmp::Reverse(tx.fee));

    Json(ApiResponse::ok(DashboardData {
        summary: DashboardSummary {
            chain_id: chain.chain_id.clone(),
            best_height: chain.dag.best_height,
            block_count: chain.dag.blocks.len(),
            tip_count: chain.dag.tips.len(),
            mempool_size: chain.mempool.transactions.len(),
            utxo_count: chain.utxo.utxos.len(),
            address_count: chain.utxo.address_index.len(),
            circulating_supply: chain.utxo.utxos.values().map(|u| u.amount).sum(),
            pulse: observational_pulse(&chain),
        },
        latest_blocks,
        mempool_transactions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    #[test]
    fn dashboard_pulse_copies_observation_and_omits_host_time() {
        let pulse = PulseObservationV1 {
            pulse_version: PULSE_VERSION_V1,
            domain: PULSE_DOMAIN_V1,
            chain_id: "dash".into(),
            selected_tip: "tip".into(),
            pulse_height: 9,
            pulse_time: 1_700_000_009,
            window_k: PULSE_WINDOW_K_V1,
            sample_count: 3,
            uncertainty_secs: 2,
            finality_lag: 9,
        };
        let view = DashboardPulse::from(&pulse);
        assert_eq!(view.pulse_height, 9);
        assert_eq!(view.selected_tip, "tip");
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("host_unix").is_none());
        assert!(json.get("now").is_none());
    }
}
