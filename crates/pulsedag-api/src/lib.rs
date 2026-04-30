use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ApiMeta {}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            meta: ApiMeta::default(),
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
            }),
            meta: ApiMeta::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineRequest {
    pub miner_address: String,
    pub pow_max_tries: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBlockTemplateRequest {
    pub miner_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitMinedBlockRequest {
    pub template_id: Option<String>,
    pub block: pulsedag_core::types::Block,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn api_response_ok_shape_is_stable() {
        let resp = ApiResponse::ok(123u64);
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["ok"], Value::Bool(true));
        assert_eq!(value["data"], Value::from(123u64));
        assert!(value["error"].is_null());
        assert!(value["meta"].is_object());
    }

    #[test]
    fn api_response_err_shape_is_stable() {
        let resp: ApiResponse<u64> = ApiResponse::err("BAD_REQUEST", "invalid payload");
        let value = serde_json::to_value(&resp).unwrap();
        assert_eq!(value["ok"], Value::Bool(false));
        assert!(value["data"].is_null());
        assert_eq!(value["error"]["code"], Value::from("BAD_REQUEST"));
        assert_eq!(value["error"]["message"], Value::from("invalid payload"));
        assert!(value["meta"].is_object());
    }
}
