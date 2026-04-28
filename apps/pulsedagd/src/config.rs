use anyhow::{bail, Result};

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
    pub p2p_connection_slot_budget: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigProfile {
    Dev,
    Testnet,
    Operator,
}

impl ConfigProfile {
    fn from_env_value(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" => Ok(Self::Dev),
            "testnet" => Ok(Self::Testnet),
            "operator" | "staging" => Ok(Self::Operator),
            other => bail!(
                "invalid PULSEDAG_CONFIG_PROFILE value '{other}'. Supported values: dev, testnet, operator (alias: staging)"
            ),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let profile = std::env::var("PULSEDAG_CONFIG_PROFILE")
            .ok()
            .map(|v| ConfigProfile::from_env_value(&v))
            .transpose()?
            .unwrap_or(ConfigProfile::Dev);
        let mut cfg = Self::defaults_for_profile(profile);
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    fn defaults_for_profile(profile: ConfigProfile) -> Self {
        match profile {
            ConfigProfile::Dev => Self {
                chain_id: "pulsedag-devnet".into(),
                rpc_bind: "127.0.0.1:8080".into(),
                p2p_enabled: false,
                p2p_mode: "memory".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/30333".into(),
                p2p_bootstrap: Vec::new(),
                p2p_mdns: true,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 8,
                rocksdb_path: "./data/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                difficulty_window: 10,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: false,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 300,
                prune_require_snapshot: true,
            },
            ConfigProfile::Testnet => Self {
                chain_id: "pulsedag-testnet".into(),
                rpc_bind: "0.0.0.0:8080".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/30333".into(),
                p2p_bootstrap: Vec::new(),
                p2p_mdns: true,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 24,
                rocksdb_path: "./data/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 500,
                prune_require_snapshot: true,
            },
            ConfigProfile::Operator => Self {
                chain_id: "pulsedag-testnet".into(),
                rpc_bind: "0.0.0.0:8080".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-real".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/30333".into(),
                p2p_bootstrap: Vec::new(),
                p2p_mdns: false,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 32,
                rocksdb_path: "./data/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 1000,
                prune_require_snapshot: true,
            },
        }
    }

    fn apply_env_overrides(&mut self) {
        self.chain_id = read_env_string("PULSEDAG_CHAIN_ID", &self.chain_id);
        self.rpc_bind = read_env_string("PULSEDAG_RPC_BIND", &self.rpc_bind);
        self.p2p_enabled = read_env_bool("PULSEDAG_P2P_ENABLED", self.p2p_enabled);
        self.p2p_mode = read_env_string("PULSEDAG_P2P_MODE", &self.p2p_mode);
        self.p2p_listen = read_env_string("PULSEDAG_P2P_LISTEN", &self.p2p_listen);
        self.p2p_bootstrap = read_env_list("PULSEDAG_P2P_BOOTSTRAP", &self.p2p_bootstrap);
        self.p2p_mdns = read_env_bool("PULSEDAG_P2P_MDNS", self.p2p_mdns);
        self.p2p_kademlia = read_env_bool("PULSEDAG_P2P_KADEMLIA", self.p2p_kademlia);
        self.p2p_connection_slot_budget = read_env_usize_positive(
            "PULSEDAG_P2P_CONNECTION_SLOT_BUDGET",
            self.p2p_connection_slot_budget,
            1,
        );
        self.rocksdb_path = read_env_string("PULSEDAG_ROCKSDB_PATH", &self.rocksdb_path);
        self.simulated_peers = read_env_list("PULSEDAG_SIMULATED_PEERS", &self.simulated_peers);
        self.auto_rebuild_on_start =
            read_env_bool("PULSEDAG_AUTO_REBUILD_ON_START", self.auto_rebuild_on_start);
        self.persist_snapshot_on_start = read_env_bool(
            "PULSEDAG_PERSIST_SNAPSHOT_ON_START",
            self.persist_snapshot_on_start,
        );
        self.target_block_interval_secs = read_env_u64_positive(
            "PULSEDAG_TARGET_BLOCK_INTERVAL_SECS",
            self.target_block_interval_secs,
            1,
        );
        self.difficulty_window =
            read_env_usize_positive("PULSEDAG_DIFFICULTY_WINDOW", self.difficulty_window, 2);
        self.max_future_drift_secs = read_env_u64_positive(
            "PULSEDAG_MAX_FUTURE_DRIFT_SECS",
            self.max_future_drift_secs,
            1,
        );
        self.snapshot_auto_every_blocks = read_env_u64(
            "PULSEDAG_SNAPSHOT_AUTO_EVERY_BLOCKS",
            self.snapshot_auto_every_blocks,
        );
        self.auto_prune_enabled =
            read_env_bool("PULSEDAG_AUTO_PRUNE_ENABLED", self.auto_prune_enabled);
        self.auto_prune_every_blocks = read_env_u64(
            "PULSEDAG_AUTO_PRUNE_EVERY_BLOCKS",
            self.auto_prune_every_blocks,
        );
        self.prune_keep_recent_blocks = read_env_u64_positive(
            "PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS",
            self.prune_keep_recent_blocks,
            1,
        );
        self.prune_require_snapshot = read_env_bool(
            "PULSEDAG_PRUNE_REQUIRE_SNAPSHOT",
            self.prune_require_snapshot,
        );
    }
}

fn read_env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn read_env_list(key: &str, default: &[String]) -> Vec<String> {
    std::env::var(key)
        .map(|v| {
            v.split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect()
        })
        .unwrap_or_else(|_| default.to_vec())
}

fn read_env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn read_env_u64_positive(key: &str, default: u64, min: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
}

fn read_env_usize_positive(key: &str, default: usize, min: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_test_env() {
        for key in [
            "PULSEDAG_CONFIG_PROFILE",
            "PULSEDAG_CHAIN_ID",
            "PULSEDAG_RPC_BIND",
            "PULSEDAG_P2P_ENABLED",
            "PULSEDAG_P2P_MODE",
            "PULSEDAG_P2P_CONNECTION_SLOT_BUDGET",
            "PULSEDAG_P2P_MDNS",
            "PULSEDAG_AUTO_PRUNE_ENABLED",
            "PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn loads_dev_profile_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.chain_id, "pulsedag-devnet");
        assert!(!cfg.p2p_enabled);
        assert_eq!(cfg.p2p_mode, "memory");
        assert_eq!(cfg.p2p_connection_slot_budget, 8);
        assert!(!cfg.auto_prune_enabled);
    }

    #[test]
    fn loads_testnet_profile_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "testnet");
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.chain_id, "pulsedag-testnet");
        assert!(cfg.p2p_enabled);
        assert_eq!(cfg.p2p_mode, "libp2p");
        assert_eq!(cfg.p2p_connection_slot_budget, 24);
        assert!(cfg.auto_prune_enabled);
        assert_eq!(cfg.prune_keep_recent_blocks, 500);
    }

    #[test]
    fn loads_operator_profile_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.chain_id, "pulsedag-testnet");
        assert!(cfg.p2p_enabled);
        assert_eq!(cfg.p2p_mode, "libp2p-real");
        assert_eq!(cfg.p2p_connection_slot_budget, 32);
        assert!(!cfg.p2p_mdns);
        assert_eq!(cfg.prune_keep_recent_blocks, 1000);
    }

    #[test]
    fn explicit_overrides_take_precedence() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_P2P_MODE", "memory");
        std::env::set_var("PULSEDAG_P2P_CONNECTION_SLOT_BUDGET", "5");
        std::env::set_var("PULSEDAG_CHAIN_ID", "custom-chain");
        std::env::set_var("PULSEDAG_AUTO_PRUNE_ENABLED", "false");
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.p2p_mode, "memory");
        assert_eq!(cfg.p2p_connection_slot_budget, 5);
        assert_eq!(cfg.chain_id, "custom-chain");
        assert!(!cfg.auto_prune_enabled);
    }

    #[test]
    fn invalid_profile_fails_clearly() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "invalid");
        let err = Config::from_env().expect_err("invalid profile should fail");
        assert!(
            err.to_string()
                .contains("invalid PULSEDAG_CONFIG_PROFILE value"),
            "unexpected error: {err}"
        );
    }
}
