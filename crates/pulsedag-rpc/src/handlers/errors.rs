use crate::api::ApiResponse;
use axum::Json;

#[derive(Debug, serde::Serialize)]
pub struct ErrorCatalogItem {
    pub code: String,
    pub description: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ErrorCatalogData {
    pub count: usize,
    pub errors: Vec<ErrorCatalogItem>,
}

pub async fn get_error_catalog() -> Json<ApiResponse<ErrorCatalogData>> {
    let errors = vec![
        ErrorCatalogItem {
            code: "NOT_FOUND".into(),
            description: "requested resource was not found".into(),
        },
        ErrorCatalogItem {
            code: "TX_REJECTED".into(),
            description: "transaction failed validation or mempool acceptance".into(),
        },
        ErrorCatalogItem {
            code: "MEMPOOL_FULL".into(),
            description: "transaction rejected because mempool reached configured capacity".into(),
        },
        ErrorCatalogItem {
            code: "MINE_ERROR".into(),
            description: "mining or block acceptance failed".into(),
        },
        ErrorCatalogItem {
            code: "P2P_DISABLED".into(),
            description: "requested p2p action while p2p is disabled".into(),
        },
        ErrorCatalogItem {
            code: "STORAGE_ERROR".into(),
            description: "storage read or write operation failed".into(),
        },
        ErrorCatalogItem {
            code: "BAD_REQUEST".into(),
            description: "request body or parameters are invalid".into(),
        },
    ];

    Json(ApiResponse::ok(ErrorCatalogData {
        count: errors.len(),
        errors,
    }))
}
