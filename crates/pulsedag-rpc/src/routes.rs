use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    api::RpcStateLike,
    handlers::{
        address::{get_address, get_address_utxos, get_utxos},
        block_validate::post_block_validate,
        bootstrap::get_bootstrap_status,
        blocks::{get_blocks, get_blocks_latest, get_blocks_page, get_blocks_recent},
        checks::get_node_checks,
        contracts::get_contracts_status,
        dashboard::get_dashboard,
        diagnostics::get_diagnostics,
        errors::get_error_catalog,
        incremental_sync::get_incremental_sync_plan,
        maintenance::get_maintenance_report,
        dag::{get_block, get_dag, get_dag_consistency, get_genesis, get_health, get_tips},
        metrics::get_metrics,
        mining_submit::post_mining_submit,
        mining_jobs::{post_claim_mining_job, post_cleanup_mining_jobs, post_submit_mining_job},
        mining_pool::{post_configure_mining_worker, post_submit_mining_share},
        mining_accounting::{get_mining_accounting, get_mining_accounting_worker},
        mining_payouts::{get_payout_history, post_run_payouts},
        mining_template::post_mining_template,
        orphans::get_orphans,
        mining_workers::{get_mining_workers_stats, post_mining_worker_heartbeat},
        mine::{post_mine, post_mine_preview},
        p2p::{get_p2p_peers, get_p2p_status, get_p2p_topics},
        pow::get_pow_info,
        pow_check::post_pow_check_header,
        pow_hash::post_pow_hash_header,
        pow_validate::post_pow_validate_header,
        pow_mine::post_pow_mine_header,
        pow_mine_capture::post_pow_mine_capture,
        pow_metrics::get_pow_metrics,
        pow_metrics_capture::post_pow_metrics_capture,
        pow_metrics_history::get_pow_metrics_history,
        pow_metrics_summary::get_pow_metrics_summary,
        pow_metrics_prune::post_pow_metrics_prune,
        pow_health::get_pow_health,
        pow_auto_run::post_pow_auto_run,
        pow_export::get_pow_export,
        pow_dashboard::get_pow_dashboard,
        pow_policy::get_pow_policy,
        policy::get_policy,
        readiness::get_readiness,
        rebuild::get_rebuild_preview,
        release::get_release_info,
        runtime::{get_runtime_events, get_runtime_events_summary, get_runtime_status},
        replay::get_replay_plan,
        search::get_search,
        snapshot::get_snapshot_info,
        status::get_status,
        sync::{get_sync_status, post_sync_rebuild, post_sync_reconcile_mempool},
        sync_blocks::get_sync_blocks,
        sync_verify::get_sync_verify,
        topology::get_topology,
        tx::{get_mempool, get_tx, get_txs, get_txs_page, get_txs_recent, post_tx_build, post_tx_submit, post_tx_validate},
        transactions::get_confirmed_transactions,
        wallet::{post_wallet_new, post_wallet_sign, post_wallet_transfer},
    },
};

pub fn router<S>() -> Router<S>
where
    S: RpcStateLike,
{
    Router::new()
        .route("/health", get(get_health::<S>))
        .route("/bootstrap", get(get_bootstrap_status::<S>))
        .route("/genesis", get(get_genesis::<S>))
        .route("/dag", get(get_dag::<S>))
        .route("/dag/consistency", get(get_dag_consistency::<S>))
        .route("/tips", get(get_tips::<S>))
        .route("/blocks", get(get_blocks::<S>))
        .route("/blocks/validate", post(post_block_validate::<S>))
        .route("/blocks/latest", get(get_blocks_latest::<S>))
        .route("/blocks/recent", get(get_blocks_recent::<S>))
        .route("/blocks/page", get(get_blocks_page::<S>))
        .route("/blocks/:hash", get(get_block::<S>))
        .route("/utxos", get(get_utxos::<S>))
        .route("/address/:address", get(get_address::<S>))
        .route("/address/:address/utxos", get(get_address_utxos::<S>))
        .route("/txs", get(get_txs::<S>))
        .route("/txs/recent", get(get_txs_recent::<S>))
        .route("/txs/page", get(get_txs_page::<S>))
        .route("/transactions", get(get_confirmed_transactions::<S>))
        .route("/mempool", get(get_mempool::<S>))
        .route("/txs/:txid", get(get_tx::<S>))
        .route("/tx/build", post(post_tx_build::<S>))
        .route("/tx/submit", post(post_tx_submit::<S>))
        .route("/wallet/new", post(post_wallet_new::<S>))
        .route("/wallet/sign", post(post_wallet_sign::<S>))
        .route("/wallet/transfer", post(post_wallet_transfer::<S>))
        .route("/mine", post(post_mine::<S>))
        .route("/mining/template", post(post_mining_template::<S>))
        .route("/mining/submit", post(post_mining_submit::<S>))
        .route("/mining/workers/heartbeat", post(post_mining_worker_heartbeat))
        .route("/mining/workers/stats", get(get_mining_workers_stats))
        .route("/mining/jobs/claim", post(post_claim_mining_job::<S>))
        .route("/mining/jobs/submit", post(post_submit_mining_job::<S>))
        .route("/mining/jobs/cleanup", post(post_cleanup_mining_jobs))
        .route("/mining/workers/configure", post(post_configure_mining_worker))
        .route("/mining/shares/submit", post(post_submit_mining_share))
        .route("/mining/accounting", get(get_mining_accounting))
        .route("/mining/accounting/:worker_id", get(get_mining_accounting_worker))
        .route("/mining/payouts/run", post(post_run_payouts))
        .route("/mining/payouts/history", get(get_payout_history))
        .route("/mine/preview", post(post_mine_preview::<S>))
        .route("/p2p/status", get(get_p2p_status::<S>))
        .route("/p2p/peers", get(get_p2p_peers::<S>))
        .route("/p2p/topics", get(get_p2p_topics::<S>))
        .route("/p2p/topology", get(get_topology::<S>))
        .route("/search/:query", get(get_search::<S>))
        .route("/metrics", get(get_metrics::<S>))
         .route("/orphans", get(get_orphans::<S>))
        .route("/dashboard", get(get_dashboard::<S>))
        .route("/runtime/events", get(get_runtime_events::<S>))
        .route("/runtime/events/summary", get(get_runtime_events_summary::<S>))
        .route("/diagnostics", get(get_diagnostics::<S>))
        .route("/errors", get(get_error_catalog))
        .route("/status", get(get_status::<S>))
        .route("/contracts/status", get(get_contracts_status::<S>))
        .route("/checks", get(get_node_checks::<S>))
        .route("/maintenance/report", get(get_maintenance_report::<S>))
        .route("/readiness", get(get_readiness::<S>))
        .route("/runtime", get(get_runtime_status::<S>))
        .route("/release", get(get_release_info))
        .route("/policy", get(get_policy))
        .route("/pow", get(get_pow_info))
        .route("/pow/validate-header", post(post_pow_validate_header))
        .route("/pow/hash-header", post(post_pow_hash_header))
        .route("/pow/check-header", post(post_pow_check_header))
        .route("/pow/mine-header", post(post_pow_mine_header))
        .route("/pow/policy", get(get_pow_policy::<S>))
        .route("/pow/metrics", get(get_pow_metrics::<S>))
        .route("/pow/metrics/capture", post(post_pow_metrics_capture::<S>))
        .route("/pow/metrics/history", get(get_pow_metrics_history))
        .route("/pow/metrics/summary", get(get_pow_metrics_summary))
        .route("/pow/health", get(get_pow_health))
        .route("/pow/metrics/prune", post(post_pow_metrics_prune))
        .route("/pow/export", get(get_pow_export))
        .route("/pow/dashboard", get(get_pow_dashboard::<S>))
        .route("/pow/mine-and-capture", post(post_pow_mine_capture::<S>))
        .route("/pow/auto/run", post(post_pow_auto_run::<S>))
        .route("/sync/status", get(get_sync_status::<S>))
        .route("/sync/replay-plan", get(get_replay_plan::<S>))
        .route("/sync/incremental-plan", get(get_incremental_sync_plan::<S>))
        .route("/sync/blocks", get(get_sync_blocks::<S>))
        .route("/sync/verify", get(get_sync_verify::<S>))
        .route("/snapshot", get(get_snapshot_info::<S>))
        .route("/sync/rebuild", post(post_sync_rebuild::<S>))
        .route("/sync/reconcile-mempool", post(post_sync_reconcile_mempool::<S>))
        .route("/sync/rebuild-preview", get(get_rebuild_preview::<S>))
}
