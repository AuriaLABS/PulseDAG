use anyhow::{bail, Result};
use pulsedag_core::ConsensusMode;

#[derive(Debug, Clone)]
pub struct Config {
    pub network_profile: String,
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
    pub target_block_interval_ms: u64,
    pub experimental_ghostdag_selection: bool,
    pub experimental_fast_cadence: bool,
    pub consensus_mode: ConsensusMode,
    pub max_parallel_tips: usize,
    pub max_merge_set_size: usize,
    pub max_orphan_count: usize,
    pub max_pending_missing_parents: usize,
    pub max_block_mass: usize,
    pub max_template_age_ms: u64,
    pub difficulty_window: usize,
    pub max_future_drift_secs: u64,
    pub snapshot_auto_every_blocks: u64,
    pub auto_prune_enabled: bool,
    pub auto_prune_every_blocks: u64,
    pub prune_keep_recent_blocks: u64,
    pub prune_require_snapshot: bool,
    pub admin_enabled: bool,
    pub operator_auth_token: Option<String>,
    pub api_profile: ApiExposureProfile,
    pub rpc_request_body_limit_bytes: usize,
    pub rpc_rate_limit_requests_per_minute: u32,
    pub rpc_rate_limit_per_ip: bool,
    pub rpc_cors_allowlist: Vec<String>,
    pub rpc_cors_unsafe_allow_wildcard_with_admin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiExposureProfile {
    LocalDev,
    PrivateOperator,
    PublicSafe,
    DisabledAdmin,
}

impl ApiExposureProfile {
    fn from_env_value(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "local_dev" => Ok(Self::LocalDev),
            "private_operator" => Ok(Self::PrivateOperator),
            "public_safe" => Ok(Self::PublicSafe),
            "disabled_admin" => Ok(Self::DisabledAdmin),
            other => bail!("invalid PULSEDAG_API_PROFILE value '{other}'. Supported values: local_dev, private_operator, public_safe, disabled_admin"),
        }
    }

    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::LocalDev => "local_dev",
            Self::PrivateOperator => "private_operator",
            Self::PublicSafe => "public_safe",
            Self::DisabledAdmin => "disabled_admin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigProfile {
    Dev,
    Local,
    Private,
    Testnet,
    Operator,
    RehearsalA,
    RehearsalB,
    RehearsalC,
}

impl ConfigProfile {
    fn from_env_value(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" => Ok(Self::Dev),
            "local" => Ok(Self::Local),
            "private" | "private-testnet" => Ok(Self::Private),
            "testnet" => Ok(Self::Testnet),
            "operator" | "staging" => Ok(Self::Operator),
            "rehearsal-a" => Ok(Self::RehearsalA),
            "rehearsal-b" => Ok(Self::RehearsalB),
            "rehearsal-c" => Ok(Self::RehearsalC),
            other => bail!(
                "invalid PULSEDAG_CONFIG_PROFILE value '{other}'. Supported values: dev, local, private, testnet, operator (alias: staging), rehearsal-a, rehearsal-b, rehearsal-c"
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
        if let Ok(v) = std::env::var("PULSEDAG_API_PROFILE") {
            cfg.api_profile = ApiExposureProfile::from_env_value(&v)?;
        }
        cfg.apply_experimental_guards()?;
        cfg.validate_api_exposure()?;
        cfg.validate_cors_policy()?;
        cfg.validate_security_hardening()?;
        Ok(cfg)
    }

    fn defaults_for_profile(profile: ConfigProfile) -> Self {
        match profile {
            ConfigProfile::Dev => Self {
                network_profile: "dev".into(),
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
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 10,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: false,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 300,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::LocalDev,
                rpc_request_body_limit_bytes: 1024 * 1024,
                rpc_rate_limit_requests_per_minute: 0,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::Local => Self {
                network_profile: "local".into(),
                chain_id: "pulsedag-localnet".into(),
                rpc_bind: "127.0.0.1:8180".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-dev".into(),
                p2p_listen: "/ip4/127.0.0.1/tcp/31333".into(),
                p2p_bootstrap: Vec::new(),
                p2p_mdns: true,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 12,
                rocksdb_path: "./data/local/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 10,
                max_future_drift_secs: 90,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: false,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 300,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::LocalDev,
                rpc_request_body_limit_bytes: 1024 * 1024,
                rpc_rate_limit_requests_per_minute: 0,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::Private => Self {
                network_profile: "private".into(),
                chain_id: "pulsedag-private".into(),
                rpc_bind: "0.0.0.0:8280".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-real".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/32333".into(),
                p2p_bootstrap: Vec::new(),
                p2p_mdns: false,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 32,
                rocksdb_path: "./data/private/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 800,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::PrivateOperator,
                rpc_request_body_limit_bytes: 512 * 1024,
                rpc_rate_limit_requests_per_minute: 120,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::Testnet => Self {
                network_profile: "testnet".into(),
                chain_id: "pulsedag-testnet".into(),
                rpc_bind: "0.0.0.0:8080".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-real".into(),
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
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 500,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::PrivateOperator,
                rpc_request_body_limit_bytes: 512 * 1024,
                rpc_rate_limit_requests_per_minute: 120,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::RehearsalA => Self {
                network_profile: "rehearsal-a".into(),
                chain_id: "pulsedag-rehearsal".into(),
                rpc_bind: "127.0.0.1:18080".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-real".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/18181".into(),
                p2p_bootstrap: Vec::new(),
                p2p_mdns: false,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 32,
                rocksdb_path: "./data/rehearsal-a/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 800,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::LocalDev,
                rpc_request_body_limit_bytes: 1024 * 1024,
                rpc_rate_limit_requests_per_minute: 0,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::RehearsalB => Self {
                network_profile: "rehearsal-b".into(),
                chain_id: "pulsedag-rehearsal".into(),
                rpc_bind: "127.0.0.1:18081".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-real".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/18182".into(),
                p2p_bootstrap: vec!["/ip4/127.0.0.1/tcp/18181".into()],
                p2p_mdns: false,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 32,
                rocksdb_path: "./data/rehearsal-b/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 800,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::LocalDev,
                rpc_request_body_limit_bytes: 1024 * 1024,
                rpc_rate_limit_requests_per_minute: 0,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::RehearsalC => Self {
                network_profile: "rehearsal-c".into(),
                chain_id: "pulsedag-rehearsal".into(),
                rpc_bind: "127.0.0.1:18082".into(),
                p2p_enabled: true,
                p2p_mode: "libp2p-real".into(),
                p2p_listen: "/ip4/0.0.0.0/tcp/18183".into(),
                p2p_bootstrap: vec![
                    "/ip4/127.0.0.1/tcp/18181".into(),
                    "/ip4/127.0.0.1/tcp/18182".into(),
                ],
                p2p_mdns: false,
                p2p_kademlia: true,
                p2p_connection_slot_budget: 32,
                rocksdb_path: "./data/rehearsal-c/rocksdb".into(),
                simulated_peers: Vec::new(),
                auto_rebuild_on_start: true,
                persist_snapshot_on_start: true,
                target_block_interval_secs: 60,
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 800,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::LocalDev,
                rpc_request_body_limit_bytes: 1024 * 1024,
                rpc_rate_limit_requests_per_minute: 0,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
            ConfigProfile::Operator => Self {
                network_profile: "operator".into(),
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
                target_block_interval_ms: 60_000,
                experimental_ghostdag_selection: false,
                experimental_fast_cadence: false,
                consensus_mode: ConsensusMode::Legacy,
                max_parallel_tips: 1,
                max_merge_set_size: 32,
                max_orphan_count: pulsedag_core::DEFAULT_ORPHAN_MAX_COUNT,
                max_pending_missing_parents: 512,
                max_block_mass: 1_000_000,
                max_template_age_ms: 30_000,
                difficulty_window: 20,
                max_future_drift_secs: 120,
                snapshot_auto_every_blocks: 25,
                auto_prune_enabled: true,
                auto_prune_every_blocks: 100,
                prune_keep_recent_blocks: 1000,
                prune_require_snapshot: true,
                admin_enabled: false,
                operator_auth_token: None,
                api_profile: ApiExposureProfile::PrivateOperator,
                rpc_request_body_limit_bytes: 512 * 1024,
                rpc_rate_limit_requests_per_minute: 120,
                rpc_rate_limit_per_ip: true,
                rpc_cors_allowlist: vec![],
                rpc_cors_unsafe_allow_wildcard_with_admin: false,
            },
        }
    }

    fn apply_env_overrides(&mut self) {
        self.network_profile = read_env_string("PULSEDAG_NETWORK_PROFILE", &self.network_profile);
        self.chain_id = read_env_string("PULSEDAG_CHAIN_ID", &self.chain_id);
        self.rpc_bind = read_env_string("PULSEDAG_RPC_BIND", &self.rpc_bind);
        self.rpc_request_body_limit_bytes = read_env_usize(
            "PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES",
            self.rpc_request_body_limit_bytes,
        );
        self.rpc_rate_limit_requests_per_minute = read_env_u32(
            "PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE",
            self.rpc_rate_limit_requests_per_minute,
        );
        self.rpc_rate_limit_per_ip =
            read_env_bool("PULSEDAG_RPC_RATE_LIMIT_PER_IP", self.rpc_rate_limit_per_ip);
        self.rpc_cors_allowlist =
            read_env_list("PULSEDAG_RPC_CORS_ALLOWLIST", &self.rpc_cors_allowlist);
        self.rpc_cors_unsafe_allow_wildcard_with_admin = read_env_bool(
            "PULSEDAG_RPC_CORS_UNSAFE_ALLOW_WILDCARD_WITH_ADMIN",
            self.rpc_cors_unsafe_allow_wildcard_with_admin,
        );
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
        self.experimental_ghostdag_selection = read_env_bool(
            "PULSEDAG_EXPERIMENTAL_GHOSTDAG_SELECTION",
            self.experimental_ghostdag_selection,
        );
        if let Ok(raw) = std::env::var("PULSEDAG_CONSENSUS_MODE") {
            if let Ok(mode) = raw.parse() {
                self.consensus_mode = mode;
            }
        }
        self.experimental_fast_cadence = read_env_bool(
            "PULSEDAG_EXPERIMENTAL_FAST_CADENCE",
            self.experimental_fast_cadence,
        );
        let interval_ms_default = std::env::var("PULSEDAG_TARGET_BLOCK_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v >= 1)
            .map(|secs| secs.saturating_mul(1_000))
            .unwrap_or(self.target_block_interval_ms);
        self.target_block_interval_ms = guarded_target_block_interval_ms(
            read_env_u64_positive("PULSEDAG_TARGET_BLOCK_INTERVAL_MS", interval_ms_default, 1),
            self.experimental_fast_cadence,
        );
        self.target_block_interval_secs = self.target_block_interval_ms.div_ceil(1_000).max(1);
        self.max_parallel_tips =
            read_env_usize_positive("PULSEDAG_MAX_PARALLEL_TIPS", self.max_parallel_tips, 1);
        self.max_merge_set_size =
            read_env_usize_positive("PULSEDAG_MAX_MERGE_SET_SIZE", self.max_merge_set_size, 1);
        self.max_orphan_count =
            read_env_usize_positive("PULSEDAG_MAX_ORPHAN_COUNT", self.max_orphan_count, 1);
        self.max_pending_missing_parents = read_env_usize_positive(
            "PULSEDAG_MAX_PENDING_MISSING_PARENTS",
            self.max_pending_missing_parents,
            1,
        );
        self.max_block_mass =
            read_env_usize_positive("PULSEDAG_MAX_BLOCK_MASS", self.max_block_mass, 1);
        self.max_template_age_ms =
            read_env_u64_positive("PULSEDAG_MAX_TEMPLATE_AGE_MS", self.max_template_age_ms, 1);
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
        self.operator_auth_token = read_env_optional_nonempty("PULSEDAG_OPERATOR_AUTH_TOKEN");
        self.apply_admin_default_or_env_override();
    }
}

impl Config {
    fn apply_admin_default_or_env_override(&mut self) {
        if let Ok(v) = std::env::var("PULSEDAG_API_PROFILE") {
            if let Ok(profile) = ApiExposureProfile::from_env_value(&v) {
                self.api_profile = profile;
            }
        }
        self.admin_enabled = std::env::var("PULSEDAG_ADMIN_ENABLED")
            .map(|v| parse_env_bool_value(&v))
            .unwrap_or_else(|_| default_admin_enabled(&self.network_profile, &self.rpc_bind));
    }

    pub fn apply_cli_args<I>(&mut self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = String>,
    {
        let args: Vec<String> = args.into_iter().collect();

        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            if arg == "--network" {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--network requires a value"))?;
                let profile = ConfigProfile::from_env_value(value)?;
                *self = Config::defaults_for_profile(profile);
            }
        }

        self.apply_env_overrides();

        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--network" => {
                    let _ = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--network requires a value"))?;
                }
                "--p2p-listen" | "--p2p-bind" => {
                    self.p2p_listen = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                    self.p2p_enabled = true;
                }
                "--rpc-listen" | "--rpc-bind" => {
                    self.rpc_bind = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?;
                }
                "--experimental-ghostdag-selection" => {
                    self.consensus_mode = ConsensusMode::GhostdagDev;
                }
                "--experimental-fast-cadence" => {
                    self.experimental_fast_cadence = true;
                }
                "--consensus-mode" => {
                    self.consensus_mode = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()
                        .map_err(anyhow::Error::msg)?;
                }
                "--target-block-interval-ms" => {
                    self.target_block_interval_ms = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--max-parallel-tips" => {
                    self.max_parallel_tips = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--max-merge-set-size" => {
                    self.max_merge_set_size = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--max-orphan-count" => {
                    self.max_orphan_count = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--max-pending-missing-parents" => {
                    self.max_pending_missing_parents = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--max-block-mass" => {
                    self.max_block_mass = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--max-template-age-ms" => {
                    self.max_template_age_ms = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("{arg} requires a value"))?
                        .parse()?;
                }
                "--bootnode" | "--peer" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--bootnode/--peer requires a value"))?;
                    for entry in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                        self.p2p_bootstrap.push(entry.to_string());
                    }
                    self.p2p_enabled = true;
                }
                _ => {}
            }
        }
        self.apply_experimental_guards()?;
        if let Ok(v) = std::env::var("PULSEDAG_API_PROFILE") {
            self.api_profile = ApiExposureProfile::from_env_value(&v)?;
        }
        self.apply_admin_default_or_env_override();
        self.apply_experimental_guards()?;
        self.validate_api_exposure()?;
        self.validate_cors_policy()?;
        self.validate_security_hardening()?;
        Ok(())
    }

    fn apply_experimental_guards(&mut self) -> Result<()> {
        self.experimental_ghostdag_selection = self.consensus_mode == ConsensusMode::GhostdagDev;
        if self.experimental_fast_cadence && !self.experimental_ghostdag_selection {
            bail!("--experimental-fast-cadence requires --experimental-ghostdag-selection");
        }
        self.experimental_fast_cadence = false;
        self.target_block_interval_ms = guarded_target_block_interval_ms(
            self.target_block_interval_ms,
            self.experimental_fast_cadence,
        );
        self.target_block_interval_secs = self.target_block_interval_ms.div_ceil(1_000).max(1);
        if !self.experimental_ghostdag_selection {
            self.max_parallel_tips = 1;
        }
        Ok(())
    }

    fn validate_api_exposure(&self) -> Result<()> {
        if matches!(
            self.api_profile,
            ApiExposureProfile::PublicSafe | ApiExposureProfile::DisabledAdmin
        ) && self.admin_enabled
        {
            bail!(
                "invalid API exposure: admin endpoints cannot be enabled for {:?} profile",
                self.api_profile
            );
        }
        if !is_local_rpc_bind(&self.rpc_bind) && self.api_profile == ApiExposureProfile::LocalDev {
            bail!("invalid API exposure: non-local RPC bind requires explicit PULSEDAG_API_PROFILE (private_operator/public_safe/disabled_admin)");
        }
        if self.admin_enabled
            && !is_local_rpc_bind(&self.rpc_bind)
            && self.operator_auth_token.is_none()
        {
            let unsafe_override = std::env::var("PULSEDAG_ADMIN_UNSAFE_ALLOW_REMOTE_NOAUTH")
                .map(|v| parse_env_bool_value(&v))
                .unwrap_or(false);
            if !unsafe_override {
                bail!("invalid API exposure: admin enabled on non-local RPC bind requires PULSEDAG_ADMIN_UNSAFE_ALLOW_REMOTE_NOAUTH=true");
            }
        }
        Ok(())
    }

    fn validate_cors_policy(&self) -> Result<()> {
        let has_wildcard = self.rpc_cors_allowlist.iter().any(|o| o.trim() == "*");
        if has_wildcard {
            bail!("invalid CORS policy: wildcard origin is not allowed; use an explicit allowlist");
        }
        Ok(())
    }

    fn validate_security_hardening(&self) -> Result<()> {
        if self.chain_id.trim().is_empty() {
            bail!(
                "invalid config: PULSEDAG_CHAIN_ID is required and cannot be empty; set a stable chain id (for example: pulsedag-testnet)"
            );
        }
        if let Some(token) = &self.operator_auth_token {
            if token.len() < 16 {
                bail!(
                    "invalid config: PULSEDAG_OPERATOR_AUTH_TOKEN is too short ({} chars). Use at least 16 characters.",
                    token.len()
                );
            }
        }
        if self.api_profile == ApiExposureProfile::PublicSafe
            && self.rpc_rate_limit_requests_per_minute == 0
        {
            let unsafe_override = std::env::var("PULSEDAG_RPC_RATE_LIMIT_UNSAFE_ALLOW_DISABLED")
                .map(|v| parse_env_bool_value(&v))
                .unwrap_or(false);
            if !unsafe_override {
                bail!("invalid config: public_safe profile cannot disable RPC rate limiting. Set PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE to a non-zero value or explicitly acknowledge risk with PULSEDAG_RPC_RATE_LIMIT_UNSAFE_ALLOW_DISABLED=true");
            }
        }
        Ok(())
    }

    pub fn config_safety_summary(&self) -> String {
        let mut warnings = Vec::new();
        if self.rpc_bind.trim().starts_with("0.0.0.0:")
            && std::env::var("PULSEDAG_API_PROFILE").is_err()
        {
            warnings.push("RPC bound to 0.0.0.0 without explicit PULSEDAG_API_PROFILE".to_string());
        }
        if matches!(self.api_profile, ApiExposureProfile::LocalDev)
            && !is_local_rpc_bind(&self.rpc_bind)
        {
            warnings.push("local_dev API profile is used with non-local bind".to_string());
        }
        if let Some(dup) = detect_duplicate_local_profile_ports() {
            warnings.push(dup);
        }
        if warnings.is_empty() {
            "config safety: OK".to_string()
        } else {
            format!(
                "config safety: {} warning(s): {}",
                warnings.len(),
                warnings.join("; ")
            )
        }
    }
}

fn detect_duplicate_local_profile_ports() -> Option<String> {
    use std::collections::HashMap;
    let profiles = [
        ConfigProfile::Dev,
        ConfigProfile::Local,
        ConfigProfile::RehearsalA,
        ConfigProfile::RehearsalB,
        ConfigProfile::RehearsalC,
    ];
    let mut seen: HashMap<u16, String> = HashMap::new();
    for profile in profiles {
        let cfg = Config::defaults_for_profile(profile);
        for bind in [cfg.rpc_bind.as_str(), cfg.p2p_listen.as_str()] {
            if let Some(port) = extract_port(bind) {
                let label = format!("{} ({})", cfg.network_profile, bind);
                if let Some(existing) = seen.insert(port, label.clone()) {
                    return Some(format!(
                        "duplicate local profile port {} between {} and {}",
                        port, existing, label
                    ));
                }
            }
        }
    }
    None
}

fn extract_port(bind: &str) -> Option<u16> {
    let tail = bind.rsplit('/').next().unwrap_or(bind);
    tail.rsplit_once(':')?.1.parse::<u16>().ok()
}

fn read_env_optional_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
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

fn default_admin_enabled(_network_profile: &str, _rpc_bind: &str) -> bool {
    false
}

pub fn is_local_rpc_bind(rpc_bind: &str) -> bool {
    let raw = rpc_bind.trim();
    if matches!(raw, "::1" | "[::1]") {
        return true;
    }
    let host = raw
        .rsplit_once(':')
        .map(|(host, _)| host.trim_matches(['[', ']']))
        .unwrap_or(raw)
        .trim();
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn parse_env_bool_value(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true")
}

fn read_env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| parse_env_bool_value(&v))
        .unwrap_or(default)
}

fn guarded_target_block_interval_ms(candidate: u64, experimental_fast_cadence: bool) -> u64 {
    let consensus = pulsedag_core::CONSENSUS_TARGET_BLOCK_INTERVAL_SECS.saturating_mul(1_000);
    if experimental_fast_cadence {
        candidate.max(1)
    } else {
        consensus
    }
}

fn read_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn read_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
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
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_test_env() {
        for key in [
            "PULSEDAG_CONFIG_PROFILE",
            "PULSEDAG_NETWORK_PROFILE",
            "PULSEDAG_CHAIN_ID",
            "PULSEDAG_RPC_BIND",
            "PULSEDAG_P2P_ENABLED",
            "PULSEDAG_P2P_MODE",
            "PULSEDAG_P2P_CONNECTION_SLOT_BUDGET",
            "PULSEDAG_P2P_MDNS",
            "PULSEDAG_AUTO_PRUNE_ENABLED",
            "PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS",
            "PULSEDAG_TARGET_BLOCK_INTERVAL_SECS",
            "PULSEDAG_TARGET_BLOCK_INTERVAL_MS",
            "PULSEDAG_EXPERIMENTAL_GHOSTDAG_SELECTION",
            "PULSEDAG_EXPERIMENTAL_FAST_CADENCE",
            "PULSEDAG_MAX_PARALLEL_TIPS",
            "PULSEDAG_MAX_MERGE_SET_SIZE",
            "PULSEDAG_MAX_ORPHAN_COUNT",
            "PULSEDAG_MAX_PENDING_MISSING_PARENTS",
            "PULSEDAG_MAX_BLOCK_MASS",
            "PULSEDAG_MAX_TEMPLATE_AGE_MS",
            "PULSEDAG_ADMIN_ENABLED",
            "PULSEDAG_ADMIN_UNSAFE_ALLOW_REMOTE_NOAUTH",
            "PULSEDAG_API_PROFILE",
            "PULSEDAG_RPC_REQUEST_BODY_LIMIT_BYTES",
            "PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE",
            "PULSEDAG_RPC_RATE_LIMIT_PER_IP",
            "PULSEDAG_RPC_CORS_ALLOWLIST",
            "PULSEDAG_RPC_CORS_UNSAFE_ALLOW_WILDCARD_WITH_ADMIN",
            "PULSEDAG_RPC_RATE_LIMIT_UNSAFE_ALLOW_DISABLED",
            "PULSEDAG_OPERATOR_AUTH_TOKEN",
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
        assert!(!cfg.admin_enabled);
    }

    #[test]
    fn target_block_interval_is_guarded_to_consensus_sixty_seconds() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "local");
        std::env::set_var("PULSEDAG_TARGET_BLOCK_INTERVAL_SECS", "30");
        let cfg = Config::from_env().expect("config");
        assert_eq!(
            cfg.target_block_interval_secs,
            pulsedag_core::CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
        );
    }

    #[test]
    fn experimental_fast_cadence_requires_ghostdag_flag() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_EXPERIMENTAL_FAST_CADENCE", "true");
        let err = Config::from_env().expect_err("fast cadence must be gated");
        assert!(err
            .to_string()
            .contains("requires --experimental-ghostdag-selection"));
    }

    #[test]
    fn experimental_flags_unlock_millisecond_cadence_and_limits() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Local);
        cfg.apply_cli_args(vec![
            "--experimental-ghostdag-selection".to_string(),
            "--experimental-fast-cadence".to_string(),
            "--target-block-interval-ms".to_string(),
            "250".to_string(),
            "--max-parallel-tips".to_string(),
            "8".to_string(),
            "--max-merge-set-size".to_string(),
            "64".to_string(),
            "--max-orphan-count".to_string(),
            "2048".to_string(),
            "--max-pending-missing-parents".to_string(),
            "1024".to_string(),
            "--max-block-mass".to_string(),
            "2000000".to_string(),
            "--max-template-age-ms".to_string(),
            "5000".to_string(),
        ])
        .expect("experimental config");

        assert!(cfg.experimental_ghostdag_selection);
        assert!(cfg.experimental_fast_cadence);
        assert_eq!(cfg.target_block_interval_ms, 250);
        assert_eq!(cfg.max_parallel_tips, 8);
        assert_eq!(cfg.max_merge_set_size, 64);
        assert_eq!(cfg.max_orphan_count, 2048);
        assert_eq!(cfg.max_pending_missing_parents, 1024);
        assert_eq!(cfg.max_block_mass, 2_000_000);
        assert_eq!(cfg.max_template_age_ms, 5_000);
    }

    #[test]
    fn loads_testnet_profile_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "testnet");
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.chain_id, "pulsedag-testnet");
        assert!(cfg.p2p_enabled);
        assert_eq!(cfg.p2p_mode, "libp2p-real");
        assert_eq!(cfg.p2p_connection_slot_budget, 24);
        assert!(cfg.auto_prune_enabled);
        assert_eq!(cfg.prune_keep_recent_blocks, 500);
        assert!(!cfg.admin_enabled);
    }

    #[test]
    fn loads_private_profile_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "private");
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.network_profile, "private");
        assert_eq!(cfg.chain_id, "pulsedag-private");
        assert_eq!(cfg.rpc_bind, "0.0.0.0:8280");
        assert!(!cfg.admin_enabled);
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
        assert!(!cfg.admin_enabled);
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
    fn cli_network_then_rpc_override_is_preserved() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Dev);
        cfg.apply_cli_args(vec![
            "--network".to_string(),
            "private".to_string(),
            "--rpc-listen".to_string(),
            "127.0.0.1:18080".to_string(),
        ])
        .expect("apply cli args");
        assert_eq!(cfg.network_profile, "private");
        assert_eq!(cfg.rpc_bind, "127.0.0.1:18080");
    }

    #[test]
    fn cli_rpc_then_network_override_is_preserved() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Dev);
        cfg.apply_cli_args(vec![
            "--rpc-listen".to_string(),
            "127.0.0.1:18080".to_string(),
            "--network".to_string(),
            "private".to_string(),
        ])
        .expect("apply cli args");
        assert_eq!(cfg.network_profile, "private");
        assert_eq!(cfg.rpc_bind, "127.0.0.1:18080");
    }

    #[test]
    fn cli_private_profile_keeps_p2p_and_bootnode_overrides() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Dev);
        cfg.apply_cli_args(vec![
            "--network".to_string(),
            "private".to_string(),
            "--p2p-listen".to_string(),
            "0.0.0.0:18181".to_string(),
            "--bootnode".to_string(),
            "/ip4/127.0.0.1/tcp/19000".to_string(),
        ])
        .expect("apply cli args");
        assert_eq!(cfg.p2p_listen, "0.0.0.0:18181");
        assert_eq!(
            cfg.p2p_bootstrap,
            vec!["/ip4/127.0.0.1/tcp/19000".to_string()]
        );
    }

    #[test]
    fn cli_back_compat_bind_aliases_apply() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Dev);
        cfg.apply_cli_args(vec![
            "--rpc-bind".to_string(),
            "127.0.0.1:48080".to_string(),
            "--p2p-bind".to_string(),
            "/ip4/127.0.0.1/tcp/49090".to_string(),
        ])
        .expect("apply cli args");
        assert_eq!(cfg.rpc_bind, "127.0.0.1:48080");
        assert_eq!(cfg.p2p_listen, "/ip4/127.0.0.1/tcp/49090");
    }

    #[test]
    fn cli_profile_defaults_apply_without_overrides() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Dev);
        cfg.apply_cli_args(vec!["--network".to_string(), "private".to_string()])
            .expect("apply cli args");
        assert_eq!(cfg.rpc_bind, "0.0.0.0:8280");
        assert_eq!(cfg.p2p_listen, "/ip4/0.0.0.0/tcp/32333");
    }

    #[test]
    fn rehearsal_profiles_load_expected_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let rehearsal_a = Config::defaults_for_profile(ConfigProfile::RehearsalA);
        assert_eq!(rehearsal_a.network_profile, "rehearsal-a");
        assert_eq!(rehearsal_a.chain_id, "pulsedag-rehearsal");
        assert_eq!(rehearsal_a.rpc_bind, "127.0.0.1:18080");
        assert_eq!(rehearsal_a.p2p_listen, "/ip4/0.0.0.0/tcp/18181");
        assert_eq!(rehearsal_a.rocksdb_path, "./data/rehearsal-a/rocksdb");
        assert!(rehearsal_a.p2p_bootstrap.is_empty());

        let rehearsal_b = Config::defaults_for_profile(ConfigProfile::RehearsalB);
        assert_eq!(rehearsal_b.network_profile, "rehearsal-b");
        assert_eq!(rehearsal_b.chain_id, "pulsedag-rehearsal");
        assert_eq!(rehearsal_b.rpc_bind, "127.0.0.1:18081");
        assert_eq!(rehearsal_b.p2p_listen, "/ip4/0.0.0.0/tcp/18182");
        assert_eq!(rehearsal_b.rocksdb_path, "./data/rehearsal-b/rocksdb");
        assert_eq!(rehearsal_b.p2p_bootstrap, vec!["/ip4/127.0.0.1/tcp/18181"]);

        let rehearsal_c = Config::defaults_for_profile(ConfigProfile::RehearsalC);
        assert_eq!(rehearsal_c.network_profile, "rehearsal-c");
        assert_eq!(rehearsal_c.chain_id, "pulsedag-rehearsal");
        assert_eq!(rehearsal_c.rpc_bind, "127.0.0.1:18082");
        assert_eq!(rehearsal_c.p2p_listen, "/ip4/0.0.0.0/tcp/18183");
        assert_eq!(rehearsal_c.rocksdb_path, "./data/rehearsal-c/rocksdb");
        assert_eq!(
            rehearsal_c.p2p_bootstrap,
            vec!["/ip4/127.0.0.1/tcp/18181", "/ip4/127.0.0.1/tcp/18182"]
        );
    }

    #[test]
    fn rehearsal_profiles_share_chain_and_separate_data_dirs() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let rehearsal_a = Config::defaults_for_profile(ConfigProfile::RehearsalA);
        let rehearsal_b = Config::defaults_for_profile(ConfigProfile::RehearsalB);
        let rehearsal_c = Config::defaults_for_profile(ConfigProfile::RehearsalC);

        assert_eq!(rehearsal_a.chain_id, rehearsal_b.chain_id);
        assert_eq!(rehearsal_b.chain_id, rehearsal_c.chain_id);
        assert_ne!(rehearsal_a.rocksdb_path, rehearsal_b.rocksdb_path);
        assert_ne!(rehearsal_b.rocksdb_path, rehearsal_c.rocksdb_path);
        assert_ne!(rehearsal_a.rocksdb_path, rehearsal_c.rocksdb_path);
    }

    #[test]
    fn rehearsal_cli_overrides_preserve_explicit_values() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let mut cfg = Config::defaults_for_profile(ConfigProfile::Dev);
        cfg.apply_cli_args(vec![
            "--network".to_string(),
            "rehearsal-b".to_string(),
            "--rpc-listen".to_string(),
            "127.0.0.1:28081".to_string(),
            "--p2p-listen".to_string(),
            "/ip4/0.0.0.0/tcp/28182".to_string(),
            "--bootnode".to_string(),
            "/ip4/10.0.0.1/tcp/29000".to_string(),
        ])
        .expect("apply cli args");

        assert_eq!(cfg.network_profile, "rehearsal-b");
        assert_eq!(cfg.rpc_bind, "127.0.0.1:28081");
        assert_eq!(cfg.p2p_listen, "/ip4/0.0.0.0/tcp/28182");
        assert_eq!(
            cfg.p2p_bootstrap,
            vec![
                "/ip4/127.0.0.1/tcp/18181".to_string(),
                "/ip4/10.0.0.1/tcp/29000".to_string()
            ]
        );
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

    #[test]
    fn admin_defaults_disabled_by_default() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        std::env::set_var("PULSEDAG_RPC_BIND", "0.0.0.0:8080");
        let err = Config::from_env().expect_err("public bind should require explicit api profile");
        assert!(err
            .to_string()
            .contains("requires explicit PULSEDAG_API_PROFILE"));

        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        let cfg = Config::from_env().expect("config");
        assert!(!cfg.admin_enabled);

        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_RPC_BIND", "127.0.0.1:8080");
        let cfg = Config::from_env().expect("config");
        assert!(!cfg.admin_enabled);
    }

    #[test]
    fn admin_remote_bind_requires_unsafe_override() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_ADMIN_ENABLED", "true");
        let err = Config::from_env().expect_err("remote admin without override should fail");
        assert!(err
            .to_string()
            .contains("PULSEDAG_ADMIN_UNSAFE_ALLOW_REMOTE_NOAUTH=true"));

        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_ADMIN_ENABLED", "true");
        std::env::set_var("PULSEDAG_ADMIN_UNSAFE_ALLOW_REMOTE_NOAUTH", "true");
        let cfg = Config::from_env().expect("override allows startup");
        assert!(cfg.admin_enabled);
    }

    #[test]
    fn default_bind_is_localhost_safe() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        let cfg = Config::from_env().expect("config");
        assert!(is_local_rpc_bind(&cfg.rpc_bind));
    }

    #[test]
    fn public_bind_requires_explicit_api_profile() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        std::env::set_var("PULSEDAG_RPC_BIND", "0.0.0.0:8080");
        let err = Config::from_env().expect_err("public bind should require explicit api profile");
        assert!(err
            .to_string()
            .contains("requires explicit PULSEDAG_API_PROFILE"));

        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        std::env::set_var("PULSEDAG_RPC_BIND", "0.0.0.0:8080");
        std::env::set_var("PULSEDAG_API_PROFILE", "private_operator");
        Config::from_env().expect("explicit profile should allow public bind");
    }

    #[test]
    fn wildcard_cors_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        std::env::set_var("PULSEDAG_ADMIN_ENABLED", "true");
        std::env::set_var("PULSEDAG_RPC_CORS_ALLOWLIST", "*");
        let err = Config::from_env().expect_err("wildcard cors with admin should fail");
        assert!(err.to_string().contains("wildcard origin is not allowed"));
    }
    #[test]
    fn admin_env_override_takes_precedence() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_RPC_BIND", "127.0.0.1:8080");
        std::env::set_var("PULSEDAG_ADMIN_ENABLED", "true");
        let cfg = Config::from_env().expect("config");
        assert!(cfg.admin_enabled);

        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        std::env::set_var("PULSEDAG_ADMIN_ENABLED", "false");
        let cfg = Config::from_env().expect("config");
        assert!(!cfg.admin_enabled);
        clear_test_env();
    }

    #[test]
    fn missing_chain_id_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "dev");
        std::env::set_var("PULSEDAG_CHAIN_ID", "   ");
        let err = Config::from_env().expect_err("missing chain id should fail");
        assert!(err.to_string().contains("PULSEDAG_CHAIN_ID is required"));
    }

    #[test]
    fn short_auth_token_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_OPERATOR_AUTH_TOKEN", "short-token");
        let err = Config::from_env().expect_err("short token should fail");
        assert!(err.to_string().contains("too short"));
    }

    #[test]
    fn public_safe_rate_limit_disabled_requires_explicit_override() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        std::env::set_var("PULSEDAG_CONFIG_PROFILE", "operator");
        std::env::set_var("PULSEDAG_API_PROFILE", "public_safe");
        std::env::set_var("PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE", "0");
        let err = Config::from_env().expect_err("public_safe should require rate limit");
        assert!(err
            .to_string()
            .contains("PULSEDAG_RPC_RATE_LIMIT_UNSAFE_ALLOW_DISABLED=true"));
    }

    #[test]
    fn config_safety_summary_reports_warning_for_public_bind_without_explicit_profile() {
        let _guard = env_lock().lock().expect("env lock");
        clear_test_env();
        let cfg = Config::defaults_for_profile(ConfigProfile::Operator);
        let summary = cfg.config_safety_summary();
        assert!(summary.contains("warning"));
        assert!(summary.contains("0.0.0.0"));
    }
}
