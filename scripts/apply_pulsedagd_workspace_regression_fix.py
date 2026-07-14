from pathlib import Path


def replace_once(source: str, old: str, new: str, label: str) -> str:
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one {label}, found {count}")
    return source.replace(old, new, 1)


config_path = Path("apps/pulsedagd/src/config.rs")
config = config_path.read_text()

helper_anchor = '''    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }
'''
helper_replacement = '''    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
'''
config = replace_once(config, helper_anchor, helper_replacement, "env_lock helper")

lock_call = 'env_lock().lock().expect("env lock")'
lock_count = config.count(lock_call)
if lock_count < 10:
    raise SystemExit(f"expected many direct env lock calls, found {lock_count}")
config = config.replace(lock_call, "env_guard()")

old_test = '''    #[test]
    fn experimental_flags_unlock_millisecond_cadence_and_limits() {
        let _guard = env_guard();
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
'''
new_test = '''    #[test]
    fn experimental_flags_keep_fast_cadence_guarded_and_apply_limits() {
        let _guard = env_guard();
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
        assert_eq!(cfg.consensus_mode, ConsensusMode::GhostdagDev);
        assert!(!cfg.experimental_fast_cadence);
        assert_eq!(
            cfg.target_block_interval_ms,
            pulsedag_core::CONSENSUS_TARGET_BLOCK_INTERVAL_SECS * 1_000
        );
        assert_eq!(
            cfg.target_block_interval_secs,
            pulsedag_core::CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
        );
        assert_eq!(cfg.max_parallel_tips, 8);
        assert_eq!(cfg.max_merge_set_size, 64);
        assert_eq!(cfg.max_orphan_count, 2048);
        assert_eq!(cfg.max_pending_missing_parents, 1024);
        assert_eq!(cfg.max_block_mass, 2_000_000);
        assert_eq!(cfg.max_template_age_ms, 5_000);
    }
'''
config = replace_once(config, old_test, new_test, "stale fast-cadence test")
config_path.write_text(config)

main_path = Path("apps/pulsedagd/src/main.rs")
main = main_path.read_text()
old_order = '''        if self.observed_network_gap < self.configured_min_gap
            || self.canonical_network_gap < self.configured_min_gap
        {
            return Err("network_gap_below_configured_minimum");
        }
        if self.observed_network_gap != self.canonical_network_gap {
            return Err("canonical_gap_disagrees_with_harness_gap");
        }
'''
new_order = '''        if self.observed_network_gap != self.canonical_network_gap {
            return Err("canonical_gap_disagrees_with_harness_gap");
        }
        if self.observed_network_gap < self.configured_min_gap
            || self.canonical_network_gap < self.configured_min_gap
        {
            return Err("network_gap_below_configured_minimum");
        }
'''
main = replace_once(main, old_order, new_order, "lag gap validation ordering block")

old_assertions = '''        evidence.configured_min_gap = 96;
        assert_eq!(
            evidence.validate(),
            Err("canonical_gap_disagrees_with_harness_gap")
        );
        evidence.canonical_network_gap = 96;
        evidence.transition_events = vec!["remote_inventory_accepted"];
'''
new_assertions = '''        evidence.configured_min_gap = 96;
        assert_eq!(
            evidence.validate(),
            Err("canonical_gap_disagrees_with_harness_gap")
        );

        evidence.observed_network_gap = 95;
        evidence.canonical_network_gap = 95;
        assert_eq!(
            evidence.validate(),
            Err("network_gap_below_configured_minimum")
        );

        evidence.observed_network_gap = 97;
        evidence.canonical_network_gap = 96;
        assert_eq!(
            evidence.validate(),
            Err("canonical_gap_disagrees_with_harness_gap")
        );

        evidence.observed_network_gap = 96;
        evidence.canonical_network_gap = 96;
        evidence.transition_events = vec!["remote_inventory_accepted"];
'''
main = replace_once(main, old_assertions, new_assertions, "lag evidence assertion sequence")
main_path.write_text(main)
