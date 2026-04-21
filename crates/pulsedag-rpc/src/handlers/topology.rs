use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct TopologyData {
    pub p2p_enabled: bool,
    pub mode: Option<String>,
    pub peer_count: usize,
    pub topic_count: usize,
    pub peers: Vec<String>,
    pub topics: Vec<String>,
}

pub async fn get_topology<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<TopologyData>> {
    match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => Json(ApiResponse::ok(TopologyData {
                p2p_enabled: true,
                mode: Some(status.mode),
                peer_count: status.connected_peers.len(),
                topic_count: status.topics.len(),
                peers: status.connected_peers,
                topics: status.topics,
            })),
            Err(e) => Json(ApiResponse::err("P2P_ERROR", e.to_string())),
        },
        None => Json(ApiResponse::ok(TopologyData {
            p2p_enabled: false,
            mode: None,
            peer_count: 0,
            topic_count: 0,
            peers: Vec::new(),
            topics: Vec::new(),
        })),
    }
}
