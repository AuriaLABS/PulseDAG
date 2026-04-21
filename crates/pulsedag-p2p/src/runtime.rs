use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct P2pRuntimeStats {
    pub peer_count: usize,
    pub last_peer_message_unix: Option<u64>,
    pub inbound_block_count: u64,
    pub inbound_tx_count: u64,
    pub outbound_block_count: u64,
    pub outbound_tx_count: u64,
    pub highest_peer_advertised_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncLagSnapshot {
    pub local_best_height: u64,
    pub sync_target_height: u64,
    pub sync_lag_blocks: u64,
    pub state: String,
    pub selected_tip: Option<String>,
}

impl SyncLagSnapshot {
    pub fn from_runtime(local_best_height: u64, selected_tip: Option<String>, highest_peer_advertised_height: u64) -> Self {
        let sync_target_height = highest_peer_advertised_height.max(local_best_height);
        let sync_lag_blocks = sync_target_height.saturating_sub(local_best_height);
        let state = if sync_lag_blocks <= 2 {
            "healthy"
        } else if sync_lag_blocks <= 20 {
            "catching_up"
        } else {
            "behind"
        }.to_string();

        Self {
            local_best_height,
            sync_target_height,
            sync_lag_blocks,
            state,
            selected_tip,
        }
    }
}
