#[derive(Debug, Clone)]
pub struct Config {
    pub chain_id: String,
    pub rpc_bind: String,
    pub p2p_enabled: bool,
    pub p2p_mode: String,
    pub p2p_listen: String,
    pub p2p_bootstrap: Vec<String>,
    pub p2p_mdns: bool,
    pub p2p_kademlia: bool,
    pub rocksdb_path: String,
    pub simulated_peers: Vec<String>,
    pub auto_rebuild_on_start: bool,
    pub persist_snapshot_on_start: bool,
    pub target_block_interval_secs: u64,
    pub difficulty_window: usize,
    pub max_future_drift_secs: u64,
    pub snapshot_auto_every_blocks: u64,
    pub auto_prune_enabled: bool,
    pub auto_prune_every_blocks: u64,
    pub prune_keep_recent_blocks: u64,
    pub prune_require_snapshot: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            chain_id: std::env::var("PULSEDAG_CHAIN_ID")
                .unwrap_or_else(|_| "pulsedag-devnet".into()),
            rpc_bind: std::env::var("PULSEDAG_RPC_BIND")
                .unwrap_or_else(|_| "127.0.0.1:8080".into()),
            p2p_enabled: std::env::var("PULSEDAG_P2P_ENABLED")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            p2p_mode: std::env::var("PULSEDAG_P2P_MODE").unwrap_or_else(|_| "memory".into()),
            p2p_listen: std::env::var("PULSEDAG_P2P_LISTEN")
                .unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/30333".into()),
            p2p_bootstrap: std::env::var("PULSEDAG_P2P_BOOTSTRAP")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
            p2p_mdns: std::env::var("PULSEDAG_P2P_MDNS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            p2p_kademlia: std::env::var("PULSEDAG_P2P_KADEMLIA")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            rocksdb_path: std::env::var("PULSEDAG_ROCKSDB_PATH")
                .unwrap_or_else(|_| "./data/rocksdb".into()),
            simulated_peers: std::env::var("PULSEDAG_SIMULATED_PEERS")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
            auto_rebuild_on_start: std::env::var("PULSEDAG_AUTO_REBUILD_ON_START")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            persist_snapshot_on_start: std::env::var("PULSEDAG_PERSIST_SNAPSHOT_ON_START")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            target_block_interval_secs: std::env::var("PULSEDAG_TARGET_BLOCK_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(60),
            difficulty_window: std::env::var("PULSEDAG_DIFFICULTY_WINDOW")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 1)
                .unwrap_or(10),
            max_future_drift_secs: std::env::var("PULSEDAG_MAX_FUTURE_DRIFT_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(120),
            snapshot_auto_every_blocks: std::env::var("PULSEDAG_SNAPSHOT_AUTO_EVERY_BLOCKS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(25),
            auto_prune_enabled: std::env::var("PULSEDAG_AUTO_PRUNE_ENABLED")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            auto_prune_every_blocks: std::env::var("PULSEDAG_AUTO_PRUNE_EVERY_BLOCKS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(100),
            prune_keep_recent_blocks: std::env::var("PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(300),
            prune_require_snapshot: std::env::var("PULSEDAG_PRUNE_REQUIRE_SNAPSHOT")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
        }
    }
}
