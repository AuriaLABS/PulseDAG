pub mod address;
pub mod dag;
pub mod mine;
pub mod p2p;
#[path = "tx_protocol.rs"]
pub mod tx;
#[allow(dead_code)]
#[path = "tx.rs"]
mod tx_legacy;
pub mod wallet;

pub mod canonical_sync;

pub mod sync;

pub mod search;

pub mod metrics;

pub mod blocks;

pub mod transactions;

pub mod dashboard;

pub mod status;

pub mod sync_blocks;

pub mod sync_verify;

pub mod maintenance;

pub mod checks;

pub mod block_validate;

pub mod snapshot;

pub mod errors;

pub mod replay;

pub mod readiness;

pub mod release;

pub mod policy;

pub mod diagnostics;

pub mod rebuild;

pub mod bootstrap;

pub mod topology;

pub mod incremental_sync;

pub mod pow;

pub mod pow_validate;

pub mod pow_hash;

pub mod pow_check;

pub mod pow_mine;

pub mod pow_policy;

pub mod pow_metrics;

pub mod pow_metrics_capture;

pub mod pow_metrics_history;

pub mod pow_metrics_summary;

pub mod pow_health;

pub mod pow_metrics_prune;

pub mod pow_export;

pub mod pow_dashboard;

pub mod pow_mine_capture;

pub mod pow_auto_run;

// Task 28 keeps protocol-aware mining facades separate from the retained legacy handlers.
#[path = "mining_submit_guard.rs"]
pub mod mining_submit;
#[path = "mining_submit.rs"]
mod mining_submit_legacy;
#[path = "mining_submit_protocol.rs"]
mod mining_submit_protocol;
#[path = "mining_template_protocol.rs"]
pub mod mining_template;
#[path = "mining_template.rs"]
mod mining_template_legacy;
pub mod mining_workers;

pub mod mining_jobs;

pub mod mining_pool;

pub mod mining_accounting;

pub mod mining_payouts;

pub mod contracts;

pub mod orphans;

pub mod pruning;
pub mod runtime;
