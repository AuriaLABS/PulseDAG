use crate::api::{ApiResponse, RpcStateLike};
use axum::{extract::State, http::StatusCode, Json};

fn legacy_wallet_rpc_removed() -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiResponse::err(
            "legacy_wallet_rpc_removed",
            "raw-key wallet RPC has been removed; sign locally and submit a fully signed transaction",
        )),
    )
}

pub async fn post_wallet_new<S: RpcStateLike>(
    State(_state): State<S>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    legacy_wallet_rpc_removed()
}

pub async fn post_wallet_sign<S: RpcStateLike>(
    State(_state): State<S>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    legacy_wallet_rpc_removed()
}

pub async fn post_wallet_transfer<S: RpcStateLike>(
    State(_state): State<S>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    legacy_wallet_rpc_removed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_wallet_rpc_is_fail_closed() {
        let (status, Json(response)) = legacy_wallet_rpc_removed();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!response.ok);
        assert!(response.data.is_none());
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("legacy_wallet_rpc_removed")
        );
    }
}
