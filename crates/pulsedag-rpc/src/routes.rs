use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{from_fn, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use crate::{
    api::{ApiResponse, RpcStateLike},
    handlers::{
        address::{
            get_address, get_address_activity, get_address_summary, get_address_utxos, get_utxos,
        },
        block_validate::post_block_validate,
        blocks::{
            get_block_overview, get_block_transactions, get_blocks, get_blocks_latest,
            get_blocks_page, get_blocks_recent,
        },
        bootstrap::get_bootstrap_status,
        checks::get_node_checks,
        dag::{get_block, get_dag, get_dag_consistency, get_genesis, get_health, get_tips},
        dashboard::get_dashboard,
        diagnostics::{get_diagnostics, get_operator_query_pack},
        errors::get_error_catalog,
        incremental_sync::get_incremental_sync_plan,
        maintenance::get_maintenance_report,
        metrics::get_metrics,
        mine::{post_mine, post_mine_preview},
        mining_jobs::{post_claim_mining_job, post_cleanup_mining_jobs, post_submit_mining_job},
        mining_submit::post_mining_submit,
        mining_template::post_mining_template,
        mining_workers::{get_mining_workers_stats, post_mining_worker_heartbeat},
        orphans::get_orphans,
        p2p::{get_p2p_peers, get_p2p_propagation, get_p2p_status, get_p2p_topics},
        policy::get_policy,
        pow::get_pow_info,
        pow_auto_run::post_pow_auto_run,
        pow_check::post_pow_check_header,
        pow_dashboard::get_pow_dashboard,
        pow_export::get_pow_export,
        pow_hash::post_pow_hash_header,
        pow_health::get_pow_health,
        pow_metrics::get_pow_metrics,
        pow_metrics_capture::post_pow_metrics_capture,
        pow_metrics_history::get_pow_metrics_history,
        pow_metrics_prune::post_pow_metrics_prune,
        pow_metrics_summary::get_pow_metrics_summary,
        pow_mine::post_pow_mine_header,
        pow_mine_capture::post_pow_mine_capture,
        pow_policy::get_pow_policy,
        pow_validate::post_pow_validate_header,
        pruning::post_prune_chain,
        readiness::get_readiness,
        rebuild::get_rebuild_preview,
        release::{get_release_info, operator_stage, repo_version},
        replay::get_replay_plan,
        runtime::{
            get_runtime_events, get_runtime_events_stream, get_runtime_events_summary,
            get_runtime_status,
        },
        search::get_search,
        snapshot::{get_snapshot_info, post_snapshot_create},
        status::get_status,
        sync::{get_sync_missing, get_sync_status, post_sync_rebuild, post_sync_reconcile_mempool},
        sync_blocks::get_sync_blocks,
        sync_verify::get_sync_verify,
        topology::get_topology,
        transactions::get_confirmed_transactions,
        tx::{
            get_mempool, get_tx, get_tx_lookup, get_txs, get_txs_activity, get_txs_page,
            get_txs_recent, post_tx_build, post_tx_submit,
        },
        wallet::{post_wallet_new, post_wallet_sign, post_wallet_transfer},
    },
};

#[derive(Debug, serde::Serialize)]
pub struct ApiVersionData {
    pub api_version: String,
    pub stable_prefix: String,
    pub release_version: String,
    pub stage: String,
}

pub async fn get_api_version() -> Json<ApiResponse<ApiVersionData>> {
    Json(ApiResponse::ok(ApiVersionData {
        api_version: "v1".to_string(),
        stable_prefix: "/api/v1".to_string(),
        release_version: repo_version(),
        stage: operator_stage(),
    }))
}

pub fn router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    router_with_profile(ApiExposureProfile::PrivateOperator, true, None, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiExposureProfile {
    LocalDev,
    PrivateOperator,
    PublicSafe,
    DisabledAdmin,
}

#[derive(Debug, Clone)]
pub struct RpcHardeningLimits {
    pub request_body_limit_bytes: usize,
    pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_window: u32,
    pub window_secs: u64,
    pub per_ip: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RateKey {
    Global,
    Ip(IpAddr),
}

#[derive(Debug, Default)]
struct RateLimiter {
    windows: HashMap<RateKey, (Instant, u32)>,
}

impl RpcHardeningLimits {
    pub fn for_profile(profile: ApiExposureProfile) -> Self {
        match profile {
            ApiExposureProfile::PublicSafe => Self {
                request_body_limit_bytes: 128 * 1024,
                rate_limit: Some(RateLimitConfig {
                    requests_per_window: 30,
                    window_secs: 60,
                    per_ip: true,
                }),
            },
            ApiExposureProfile::PrivateOperator => Self {
                request_body_limit_bytes: 512 * 1024,
                rate_limit: Some(RateLimitConfig {
                    requests_per_window: 120,
                    window_secs: 60,
                    per_ip: true,
                }),
            },
            ApiExposureProfile::LocalDev | ApiExposureProfile::DisabledAdmin => Self {
                request_body_limit_bytes: 1024 * 1024,
                rate_limit: None,
            },
        }
    }
}

pub fn router_with_profile<S>(
    profile: ApiExposureProfile,
    admin_enabled: bool,
    operator_auth_token: Option<String>,
    limits: Option<RpcHardeningLimits>,
) -> Router<S>
where
    S: RpcStateLike,
{
    let auth = operator_auth_token;
    let limits = limits.unwrap_or_else(|| RpcHardeningLimits::for_profile(profile));
    match profile {
        ApiExposureProfile::PublicSafe => Router::new()
            .nest("/api/v1", public_safe_api_v1_router::<S>())
            .merge(public_safe_compatibility_router::<S>())
            .layer(from_fn(move |req, next| {
                hardening_middleware(req, next, limits.clone())
            })),
        ApiExposureProfile::DisabledAdmin => {
            router_with_admin(false, auth).layer(from_fn(move |req, next| {
                hardening_middleware(req, next, limits.clone())
            }))
        }
        ApiExposureProfile::LocalDev | ApiExposureProfile::PrivateOperator => {
            router_with_admin(admin_enabled, auth).layer(from_fn(move |req, next| {
                hardening_middleware(req, next, limits.clone())
            }))
        }
    }
}

async fn hardening_middleware(req: Request, next: Next, limits: RpcHardeningLimits) -> Response {
    let path = req.uri().path();
    let is_guarded = matches!(
        path,
        "/tx/submit"
            | "/api/v1/tx/submit"
            | "/mining/submit"
            | "/api/v1/mining/submit"
            | "/snapshot/create"
            | "/admin/snapshot/create"
            | "/prune"
            | "/admin/prune"
            | "/sync/rebuild"
            | "/admin/sync/rebuild"
            | "/sync/reconcile-mempool"
            | "/admin/sync/reconcile-mempool"
            | "/diagnostics"
            | "/admin/diagnostics"
            | "/operator/query-pack"
            | "/admin/operator/query-pack"
    );
    if !is_guarded {
        return next.run(req).await;
    }
    if let Some(len) = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if len > limits.request_body_limit_bytes {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ApiResponse::<serde_json::Value>::err(
                    "request_too_large",
                    "request body exceeds configured limit",
                )),
            )
                .into_response();
        }
    }
    if let Some(cfg) = &limits.rate_limit {
        static LIMITER: std::sync::OnceLock<Arc<Mutex<RateLimiter>>> = std::sync::OnceLock::new();
        let limiter = LIMITER
            .get_or_init(|| Arc::new(Mutex::new(RateLimiter::default())))
            .clone();
        let key = if cfg.per_ip {
            req.extensions()
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| RateKey::Ip(c.0.ip()))
                .unwrap_or(RateKey::Global)
        } else {
            RateKey::Global
        };
        let mut guard = limiter.lock().await;
        let now = Instant::now();
        let entry = guard.windows.entry(key).or_insert((now, 0));
        if now.duration_since(entry.0) >= Duration::from_secs(cfg.window_secs) {
            *entry = (now, 0);
        }
        if entry.1 >= cfg.requests_per_window {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ApiResponse::<serde_json::Value>::err(
                    "rate_limited",
                    "request rate exceeded configured limit",
                )),
            )
                .into_response();
        }
        entry.1 = entry.1.saturating_add(1);
    }
    next.run(req).await
}

pub fn router_with_admin<S>(admin_enabled: bool, operator_auth_token: Option<String>) -> Router<S>
where
    S: RpcStateLike,
{
    let mut app = Router::new()
        .nest("/api/v1", public_api_v1_router::<S>())
        .merge(public_compatibility_router::<S>());

    if admin_enabled {
        app = app
            .nest("/admin", admin_router::<S>(operator_auth_token.clone()))
            .merge(admin_compatibility_router::<S>(operator_auth_token));
    } else {
        app = app
            .nest("/admin", disabled_admin_router::<S>())
            .route("/admin", any(disabled_admin_endpoint))
            .route("/admin/*path", any(disabled_admin_endpoint));
    }

    app
}

async fn disabled_admin_endpoint() -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    (
        StatusCode::FORBIDDEN,
        Json(ApiResponse::err(
            "FORBIDDEN",
            "admin endpoints are disabled",
        )),
    )
}

fn disabled_admin_router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    Router::new().fallback(any(disabled_admin_endpoint))
}

fn public_api_v1_router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    public_routes::<S>()
        .route("/", get(get_api_version))
        .route("/version", get(get_api_version))
}

fn public_compatibility_router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    public_routes::<S>()
}

fn public_safe_api_v1_router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    public_safe_routes::<S>()
        .route("/", get(get_api_version))
        .route("/version", get(get_api_version))
}

fn public_safe_compatibility_router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    public_safe_routes::<S>()
}

fn public_safe_routes<S>() -> Router<S>
where
    S: RpcStateLike,
{
    Router::new()
        .route("/health", get(get_health::<S>))
        .route("/bootstrap", get(get_bootstrap_status::<S>))
        .route("/genesis", get(get_genesis::<S>))
        .route("/dag", get(get_dag::<S>))
        .route("/tips", get(get_tips::<S>))
        .route("/blocks", get(get_blocks::<S>))
        .route("/blocks/latest", get(get_blocks_latest::<S>))
        .route("/blocks/recent", get(get_blocks_recent::<S>))
        .route("/blocks/page", get(get_blocks_page::<S>))
        .route("/blocks/:hash/overview", get(get_block_overview::<S>))
        .route(
            "/blocks/:hash/transactions",
            get(get_block_transactions::<S>),
        )
        .route("/blocks/:hash", get(get_block::<S>))
        .route("/utxos", get(get_utxos::<S>))
        .route("/address/:address", get(get_address::<S>))
        .route("/address/:address/summary", get(get_address_summary::<S>))
        .route("/address/:address/activity", get(get_address_activity::<S>))
        .route("/address/:address/utxos", get(get_address_utxos::<S>))
        .route("/txs", get(get_txs::<S>))
        .route("/txs/recent", get(get_txs_recent::<S>))
        .route("/txs/page", get(get_txs_page::<S>))
        .route("/txs/activity", get(get_txs_activity::<S>))
        .route("/txs/:txid/lookup", get(get_tx_lookup::<S>))
        .route("/transactions", get(get_confirmed_transactions::<S>))
        .route("/mempool", get(get_mempool::<S>))
        .route("/txs/:txid", get(get_tx::<S>))
        .route("/search/:query", get(get_search::<S>))
        .route("/metrics", get(get_metrics::<S>))
        .route("/orphans", get(get_orphans::<S>))
        .route("/dashboard", get(get_dashboard::<S>))
        .route("/errors", get(get_error_catalog))
        .route("/status", get(get_status::<S>))
        .route("/checks", get(get_node_checks::<S>))
        .route("/readiness", get(get_readiness::<S>))
        .route("/release", get(get_release_info))
        .route("/policy", get(get_policy::<S>))
}

fn public_routes<S>() -> Router<S>
where
    S: RpcStateLike,
{
    Router::new()
        .route("/health", get(get_health::<S>))
        .route("/bootstrap", get(get_bootstrap_status::<S>))
        .route("/genesis", get(get_genesis::<S>))
        .route("/dag", get(get_dag::<S>))
        .route("/tips", get(get_tips::<S>))
        .route("/blocks", get(get_blocks::<S>))
        .route("/blocks/validate", post(post_block_validate::<S>))
        .route("/blocks/latest", get(get_blocks_latest::<S>))
        .route("/blocks/recent", get(get_blocks_recent::<S>))
        .route("/blocks/page", get(get_blocks_page::<S>))
        .route("/blocks/:hash/overview", get(get_block_overview::<S>))
        .route(
            "/blocks/:hash/transactions",
            get(get_block_transactions::<S>),
        )
        .route("/blocks/:hash", get(get_block::<S>))
        .route("/utxos", get(get_utxos::<S>))
        .route("/address/:address", get(get_address::<S>))
        .route("/address/:address/summary", get(get_address_summary::<S>))
        .route("/address/:address/activity", get(get_address_activity::<S>))
        .route("/address/:address/utxos", get(get_address_utxos::<S>))
        .route("/txs", get(get_txs::<S>))
        .route("/txs/recent", get(get_txs_recent::<S>))
        .route("/txs/page", get(get_txs_page::<S>))
        .route("/txs/activity", get(get_txs_activity::<S>))
        .route("/txs/:txid/lookup", get(get_tx_lookup::<S>))
        .route("/transactions", get(get_confirmed_transactions::<S>))
        .route("/mempool", get(get_mempool::<S>))
        .route("/txs/:txid", get(get_tx::<S>))
        .route("/tx/build", post(post_tx_build::<S>))
        .route("/tx/submit", post(post_tx_submit::<S>))
        .route("/mine", post(post_mine::<S>))
        .route("/mining/template", post(post_mining_template::<S>))
        .route("/mining/submit", post(post_mining_submit::<S>))
        .route(
            "/mining/workers/heartbeat",
            post(post_mining_worker_heartbeat),
        )
        .route("/mining/workers/stats", get(get_mining_workers_stats))
        .route("/mining/jobs/claim", post(post_claim_mining_job::<S>))
        .route("/mining/jobs/submit", post(post_submit_mining_job::<S>))
        .route("/mine/preview", post(post_mine_preview::<S>))
        .route("/p2p/status", get(get_p2p_status::<S>))
        .route("/p2p/peers", get(get_p2p_peers::<S>))
        .route("/p2p/propagation", get(get_p2p_propagation::<S>))
        .route("/p2p/topics", get(get_p2p_topics::<S>))
        .route("/p2p/topology", get(get_topology::<S>))
        .route("/search/:query", get(get_search::<S>))
        .route("/metrics", get(get_metrics::<S>))
        .route("/orphans", get(get_orphans::<S>))
        .route("/dashboard", get(get_dashboard::<S>))
        .route("/errors", get(get_error_catalog))
        .route("/status", get(get_status::<S>))
        .route("/checks", get(get_node_checks::<S>))
        .route("/readiness", get(get_readiness::<S>))
        .route("/release", get(get_release_info))
        .route("/policy", get(get_policy::<S>))
        .route("/pow", get(get_pow_info))
        .route("/pow/validate-header", post(post_pow_validate_header))
        .route("/pow/hash-header", post(post_pow_hash_header))
        .route("/pow/check-header", post(post_pow_check_header))
        .route("/pow/mine-header", post(post_pow_mine_header))
        .route("/pow/policy", get(get_pow_policy::<S>))
        .route("/pow/metrics", get(get_pow_metrics::<S>))
        .route("/pow/metrics/history", get(get_pow_metrics_history))
        .route("/pow/metrics/summary", get(get_pow_metrics_summary))
        .route("/pow/health", get(get_pow_health))
        .route("/pow/export", get(get_pow_export))
        .route("/pow/dashboard", get(get_pow_dashboard::<S>))
        .route("/sync/status", get(get_sync_status::<S>))
        .route("/sync/missing", get(get_sync_missing::<S>))
        .route("/sync/blocks", get(get_sync_blocks::<S>))
        .route("/sync/verify", get(get_sync_verify::<S>))
        .route("/snapshot", get(get_snapshot_info::<S>))
}

fn admin_router<S>(operator_auth_token: Option<String>) -> Router<S>
where
    S: RpcStateLike,
{
    admin_routes::<S>(operator_auth_token)
}

fn admin_compatibility_router<S>(operator_auth_token: Option<String>) -> Router<S>
where
    S: RpcStateLike,
{
    admin_routes::<S>(operator_auth_token)
}

fn admin_routes<S>(operator_auth_token: Option<String>) -> Router<S>
where
    S: RpcStateLike,
{
    let router = Router::new()
        .route("/dag/consistency", get(get_dag_consistency::<S>))
        .route("/wallet/new", post(post_wallet_new::<S>))
        .route("/wallet/sign", post(post_wallet_sign::<S>))
        .route("/wallet/transfer", post(post_wallet_transfer::<S>))
        .route("/mining/jobs/cleanup", post(post_cleanup_mining_jobs))
        .route("/runtime", get(get_runtime_status::<S>))
        .route("/runtime/events", get(get_runtime_events::<S>))
        .route(
            "/runtime/events/stream",
            get(get_runtime_events_stream::<S>),
        )
        .route(
            "/runtime/events/summary",
            get(get_runtime_events_summary::<S>),
        )
        .route("/diagnostics", get(get_diagnostics::<S>))
        .route("/operator/query-pack", get(get_operator_query_pack::<S>))
        .route("/maintenance/report", get(get_maintenance_report::<S>))
        .route("/pow/metrics/capture", post(post_pow_metrics_capture::<S>))
        .route("/pow/metrics/prune", post(post_pow_metrics_prune))
        .route("/pow/mine-and-capture", post(post_pow_mine_capture::<S>))
        .route("/pow/auto/run", post(post_pow_auto_run::<S>))
        .route("/sync/replay-plan", get(get_replay_plan::<S>))
        .route(
            "/sync/incremental-plan",
            get(get_incremental_sync_plan::<S>),
        )
        .route("/snapshot/create", post(post_snapshot_create::<S>))
        .route("/prune", post(post_prune_chain::<S>))
        .route("/sync/rebuild", post(post_sync_rebuild::<S>))
        .route(
            "/sync/reconcile-mempool",
            post(post_sync_reconcile_mempool::<S>),
        )
        .route("/sync/rebuild-preview", get(get_rebuild_preview::<S>));
    if let Some(token) = operator_auth_token {
        router.layer(from_fn(move |req, next| {
            operator_auth_middleware(req, next, token.clone())
        }))
    } else {
        router
    }
}

async fn operator_auth_middleware(req: Request, next: Next, token: String) -> Response {
    let auth = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let Some(auth) = auth else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<serde_json::Value>::err(
                "missing_auth",
                "authorization header is required",
            )),
        )
            .into_response();
    };
    let Some(presented) = auth.strip_prefix("Bearer ") else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<serde_json::Value>::err(
                "auth_required",
                "bearer token is required",
            )),
        )
            .into_response();
    };
    if presented != token {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<serde_json::Value>::err(
                "invalid_auth",
                "invalid bearer token",
            )),
        )
            .into_response();
    }
    next.run(req).await
}
