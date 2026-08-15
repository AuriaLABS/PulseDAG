use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{rejection::JsonRejection, ConnectInfo, DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{from_fn, Next},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use tokio::sync::Mutex;

use crate::{
    api::{
        record_rpc_rate_limit_eviction, record_rpc_rate_limit_rejected,
        record_rpc_rate_limit_tracked_keys, ApiResponse, RpcStateLike, SubmitTxRequest,
    },
    handlers::tx::post_tx_submit,
};

pub use crate::routes_base::{
    get_api_version, router, router_with_admin, ApiExposureProfile, ApiVersionData, RateLimitConfig,
    RpcHardeningLimits,
};

const PUBLIC_RELAY_RATE_LIMIT_MAX_TRACKED_KEYS: usize = 4096;
const REQUEST_TOO_LARGE_CODE: &str = "request_too_large";
const RATE_LIMITED_CODE: &str = "rate_limited";
const INVALID_TRANSACTION_PAYLOAD_CODE: &str = "invalid_transaction_payload";

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicRelaySubmitRequest {
    transaction: pulsedag_core::types::Transaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RelayRateKey {
    Global,
    Ip(IpAddr),
}

#[derive(Debug, Clone)]
struct RelayRateWindow {
    started_at: Instant,
    count: u32,
    sequence: u64,
}

#[derive(Debug, Default)]
struct RelayRateLimiter {
    windows: HashMap<RelayRateKey, RelayRateWindow>,
    next_sequence: u64,
}

impl RelayRateLimiter {
    fn allow(&mut self, key: RelayRateKey, cfg: &RateLimitConfig, now: Instant) -> bool {
        let window = Duration::from_secs(cfg.window_secs.max(1));
        self.windows
            .retain(|_, entry| now.duration_since(entry.started_at) < window);

        if !self.windows.contains_key(&key)
            && self.windows.len() >= PUBLIC_RELAY_RATE_LIMIT_MAX_TRACKED_KEYS
        {
            let oldest = self
                .windows
                .iter()
                .min_by_key(|(_, entry)| (entry.started_at, entry.sequence))
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.windows.remove(&oldest);
                record_rpc_rate_limit_eviction();
            }
        }

        if !self.windows.contains_key(&key) {
            let sequence = self.next_sequence;
            self.next_sequence = self.next_sequence.saturating_add(1);
            self.windows.insert(
                key.clone(),
                RelayRateWindow {
                    started_at: now,
                    count: 0,
                    sequence,
                },
            );
        }

        record_rpc_rate_limit_tracked_keys(self.windows.len() as u64);
        let entry = self
            .windows
            .get_mut(&key)
            .expect("public relay rate-limit entry must exist after insertion");
        if entry.count >= cfg.requests_per_window {
            record_rpc_rate_limit_rejected();
            return false;
        }
        entry.count = entry.count.saturating_add(1);
        true
    }
}

fn parse_public_relay_cors_allowed_origins(raw: Option<&str>) -> Vec<String> {
    raw.into_iter()
        .flat_map(|value| value.split(',').map(str::trim).map(str::to_string))
        .filter(|origin| !origin.is_empty() && origin != "*")
        .collect()
}

fn public_relay_cors_allowed_origins() -> Vec<String> {
    let raw = std::env::var("PULSEDAG_RPC_CORS_ALLOWLIST").ok();
    parse_public_relay_cors_allowed_origins(raw.as_deref())
}

fn apply_public_relay_cors_headers(response: &mut Response, origin: &str) {
    let Ok(origin) = HeaderValue::try_from(origin) else {
        return;
    };
    response
        .headers_mut()
        .insert(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    response.headers_mut().insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    response.headers_mut().insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Accept, Content-Type"),
    );
    response
        .headers_mut()
        .insert(axum::http::header::VARY, HeaderValue::from_static("Origin"));
}

async fn public_relay_cors_middleware(
    req: Request,
    next: Next,
    allowed_origins: Arc<Vec<String>>,
) -> Response {
    let origin = req
        .headers()
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let Some(origin) = origin else {
        return next.run(req).await;
    };

    if !allowed_origins.iter().any(|allowed| allowed == &origin) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<serde_json::Value>::err(
                "cors_origin_denied",
                "cross-origin access is not allowed for this public relay origin",
            )),
        )
            .into_response();
    }

    if req.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_public_relay_cors_headers(&mut response, &origin);
        return response;
    }

    let mut response = next.run(req).await;
    apply_public_relay_cors_headers(&mut response, &origin);
    response
}

async fn public_relay_rate_limit_middleware(
    req: Request,
    next: Next,
    cfg: Option<RateLimitConfig>,
    limiter: Arc<Mutex<RelayRateLimiter>>,
) -> Response {
    let Some(cfg) = cfg else {
        return next.run(req).await;
    };

    let key = if cfg.per_ip {
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| RelayRateKey::Ip(c.0.ip()))
            .unwrap_or(RelayRateKey::Global)
    } else {
        RelayRateKey::Global
    };

    let mut guard = limiter.lock().await;
    if !guard.allow(key, &cfg, Instant::now()) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::<serde_json::Value>::err(
                RATE_LIMITED_CODE,
                "request rate exceeded configured limit",
            )),
        )
            .into_response();
    }
    drop(guard);
    next.run(req).await
}

async fn public_relay_declared_body_limit_middleware(
    req: Request,
    next: Next,
    request_body_limit_bytes: usize,
) -> Response {
    if let Some(len) = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
    {
        if len > request_body_limit_bytes {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ApiResponse::<serde_json::Value>::err(
                    REQUEST_TOO_LARGE_CODE,
                    "request body exceeds configured limit",
                )),
            )
                .into_response();
        }
    }
    next.run(req).await
}

async fn public_relay_options() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn post_public_tx_submit<S: RpcStateLike>(
    State(state): State<S>,
    payload: Result<Json<PublicRelaySubmitRequest>, JsonRejection>,
) -> Response {
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::err(
                    INVALID_TRANSACTION_PAYLOAD_CODE,
                    "request must contain exactly one fully formed transaction",
                )),
            )
                .into_response();
        }
    };

    post_tx_submit(
        State(state),
        Json(SubmitTxRequest {
            transaction: payload.transaction,
        }),
    )
    .await
    .into_response()
}

/// Public routing facade.
///
/// The base router remains authoritative for every existing profile and route.
/// For `PublicSafe` only, this facade adds one narrow write surface:
/// `POST /api/v1/tx/submit`, which accepts a fully formed transaction and
/// delegates to the existing consensus/mempool admission handler.
pub fn router_with_profile<S>(
    profile: ApiExposureProfile,
    admin_enabled: bool,
    operator_auth_token: Option<String>,
    limits: Option<RpcHardeningLimits>,
) -> Router<S>
where
    S: RpcStateLike,
{
    let effective_limits = limits
        .clone()
        .unwrap_or_else(|| RpcHardeningLimits::for_profile(profile));
    let base = crate::routes_base::router_with_profile::<S>(
        profile,
        admin_enabled,
        operator_auth_token,
        limits,
    );

    if profile != ApiExposureProfile::PublicSafe {
        return base;
    }

    let allowed_origins = Arc::new(public_relay_cors_allowed_origins());
    let rate_limit = effective_limits.rate_limit.clone();
    let limiter = Arc::new(Mutex::new(RelayRateLimiter::default()));
    let request_body_limit_bytes = effective_limits.request_body_limit_bytes;

    let relay = Router::<S>::new()
        .route(
            "/api/v1/tx/submit",
            post(post_public_tx_submit::<S>).options(public_relay_options),
        )
        // Stable declared-length rejection plus an extractor-level hard backstop
        // for chunked or otherwise undeclared oversized requests.
        .layer(DefaultBodyLimit::max(request_body_limit_bytes))
        .layer(from_fn(move |req, next| {
            public_relay_declared_body_limit_middleware(req, next, request_body_limit_bytes)
        }))
        .layer(from_fn(move |req, next| {
            let cfg = rate_limit.clone();
            let limiter = Arc::clone(&limiter);
            async move { public_relay_rate_limit_middleware(req, next, cfg, limiter).await }
        }))
        .layer(from_fn(move |req, next| {
            let allowed_origins = Arc::clone(&allowed_origins);
            async move { public_relay_cors_middleware(req, next, allowed_origins).await }
        }));

    base.merge(relay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_cors_allowlist_rejects_wildcard_and_empty_entries() {
        assert_eq!(
            parse_public_relay_cors_allowed_origins(Some(
                "https://wallet.example, *, ,https://explorer.example"
            )),
            vec![
                "https://wallet.example".to_string(),
                "https://explorer.example".to_string()
            ]
        );
    }

    #[test]
    fn relay_request_schema_rejects_secret_bearing_unknown_fields() {
        let value = serde_json::json!({
            "transaction": {
                "txid": "x",
                "version": 1,
                "inputs": [],
                "outputs": [],
                "fee": 0,
                "nonce": 0
            },
            "private_key": "must-not-be-accepted"
        });
        let err = serde_json::from_value::<PublicRelaySubmitRequest>(value)
            .expect_err("unknown secret-bearing field must be rejected");
        assert!(err.to_string().contains("unknown field"));
    }
}
