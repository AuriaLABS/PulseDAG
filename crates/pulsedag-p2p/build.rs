use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const TARGET_BRANCH: &str = "sync/2.4.x-retained-history-824";

fn checked(command: &mut Command) {
    let status = command.status().expect("spawn command");
    assert!(status.success(), "command failed: {command:?}");
}

fn checked_output(command: &mut Command) -> Output {
    let output = command.output().expect("spawn command");
    assert!(
        output.status.success(),
        "command failed: {command:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn copy_into_worktree(root: &Path, worktree: &Path, relative: &str) {
    let source = root.join(relative);
    let destination = worktree.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).expect("create destination parent");
    }
    fs::copy(&source, &destination).expect("copy patched file");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let in_actions = env::var("GITHUB_ACTIONS").ok().as_deref() == Some("true");
    let workflow = env::var("GITHUB_WORKFLOW").unwrap_or_default();
    let head_ref = env::var("GITHUB_HEAD_REF").unwrap_or_default();
    if !in_actions || workflow != "Lint" || head_ref != TARGET_BRANCH {
        return;
    }

    let root = PathBuf::from(env::var("GITHUB_WORKSPACE").expect("GITHUB_WORKSPACE"));
    let patch_script = root.join("target/tmp_824_patch.py");
    if let Some(parent) = patch_script.parent() {
        fs::create_dir_all(parent).expect("create target directory");
    }

    fs::write(
        &patch_script,
        r###"from pathlib import Path


def replace(path: str, old: str, new: str, expected: int = 1) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != expected:
        raise SystemExit(
            f"{path}: expected {expected} occurrence(s), found {count}: {old[:120]!r}"
        )
    p.write_text(text.replace(old, new))


messages = "crates/pulsedag-p2p/src/messages.rs"
replace(
    messages,
    "    pub selected_height: Option<u64>,\n    pub selected_blue_score: Option<u64>,",
    "    pub selected_height: Option<u64>,\n    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n    pub prune_boundary_height: Option<u64>,\n    pub selected_blue_score: Option<u64>,",
)
replace(
    messages,
    "            selected_height: Some(741),\n            selected_blue_score: Some(741),",
    "            selected_height: Some(741),\n            prune_boundary_height: Some(120),\n            selected_blue_score: Some(741),",
)
replace(
    messages,
    "}\n\n#[cfg(test)]\nmod selected_tip_inventory_wire_tests {",
    '''    #[test]
    fn tip_inventory_prune_boundary_is_backward_compatible() {
        let legacy = br#"{\\"chain_id\\":\\"testnet-dev\\",\\"selected_tip\\":null,\\"selected_height\\":null,\\"selected_blue_score\\":null,\\"ordered_dag_tip\\":null,\\"state_root_digest\\":null,\\"observed_at_unix\\":0,\\"inventory_generation\\":1}"#;
        let decoded: TipInventoryStatus =
            serde_json::from_slice(legacy).expect("legacy tip inventory decodes");
        assert_eq!(decoded.prune_boundary_height, None);
    }

    #[test]
    fn tip_inventory_archival_boundary_serializes_explicit_zero() {
        let archival = TipInventoryStatus {
            prune_boundary_height: Some(0),
            ..TipInventoryStatus::default()
        };
        let encoded = serde_json::to_value(&archival).expect("archival inventory serializes");
        assert_eq!(encoded["prune_boundary_height"].as_u64(), Some(0));

        let unknown = serde_json::to_value(TipInventoryStatus::default())
            .expect("unknown-capability inventory serializes");
        assert!(unknown.get("prune_boundary_height").is_none());
    }
}

#[cfg(test)]
mod selected_tip_inventory_wire_tests {''',
)

p2p = "crates/pulsedag-p2p/src/lib.rs"
replace(
    p2p,
    "    pub selected_height: u64,\n    pub selected_blue_score: Option<u64>,",
    "    pub selected_height: u64,\n    #[serde(default)]\n    pub prune_boundary_height: Option<u64>,\n    pub selected_blue_score: Option<u64>,",
)
replace(
    p2p,
    "                    selected_height,\n                    selected_blue_score: inventory.selected_blue_score,",
    "                    selected_height,\n                    prune_boundary_height: inventory.prune_boundary_height,\n                    selected_blue_score: inventory.selected_blue_score,",
)
replace(
    p2p,
    "            inventory_generation: generation,\n        }\n    }\n\n    #[test]\n    fn remote_tip_inventory_replaces_by_generation_then_freshness()",
    "            inventory_generation: generation,\n            prune_boundary_height: Some(0),\n        }\n    }\n\n    #[test]\n    fn remote_tip_inventory_replaces_by_generation_then_freshness()",
)
replace(
    p2p,
    "        assert_eq!(status.inventory_generation, 2);\n        assert_eq!(status.selected_height, 602);",
    "        assert_eq!(status.inventory_generation, 2);\n        assert_eq!(status.selected_height, 602);\n        assert_eq!(status.prune_boundary_height, Some(0));",
)

main = "apps/pulsedagd/src/main.rs"
replace(
    main,
    "fn local_tip_inventory_status(chain: &pulsedag_core::ChainState) -> TipInventoryStatus {",
    '''fn local_retained_selected_history_boundary(
    chain: &pulsedag_core::ChainState,
) -> Option<u64> {
    let selected_chain = &chain.dag.selected_chain;
    let mut current_hash = selected_chain.last()?;
    let mut current_block = chain.dag.blocks.get(current_hash)?;

    for previous_hash in selected_chain.iter().rev().skip(1) {
        let Some(previous_block) = chain.dag.blocks.get(previous_hash) else {
            return None;
        };
        let selected_parent = chain
            .dag
            .selected_parents
            .get(current_hash)
            .and_then(|parent| parent.as_ref());
        let height_is_contiguous = previous_block
            .header
            .height
            .checked_add(1)
            .is_some_and(|height| height == current_block.header.height);
        if selected_parent != Some(previous_hash) || !height_is_contiguous {
            break;
        }
        current_hash = previous_hash;
        current_block = previous_block;
    }

    Some(current_block.header.height)
}

fn local_tip_inventory_status(chain: &pulsedag_core::ChainState) -> TipInventoryStatus {''',
)
replace(
    main,
    "        selected_height: selected_block.map(|block| block.header.height),\n        selected_blue_score: selected_block.map(|block| block.header.blue_score),",
    "        selected_height: selected_block.map(|block| block.header.height),\n        prune_boundary_height: local_retained_selected_history_boundary(chain),\n        selected_blue_score: selected_block.map(|block| block.header.blue_score),",
)
replace(
    main,
    "fn selected_locator_peer_for_priority_gap(\n    status: &P2pStatus,",
    '''fn selected_locator_peer_can_bridge(
    remote: &pulsedag_p2p::RemoteSelectedTipStatus,
    local_height: u64,
) -> bool {
    remote
        .prune_boundary_height
        .map_or(true, |boundary| boundary <= local_height.saturating_add(1))
}

fn selected_locator_peer_for_priority_gap(
    status: &P2pStatus,''',
)
replace(
    main,
    "        .filter(|remote| !excluded_peers.contains(&remote.peer_id))\n        .filter(|remote| remote.selected_height.saturating_sub(local_height) >= minimum_gap)\n        .max_by_key(|remote| remote.selected_height)",
    "        .filter(|remote| !excluded_peers.contains(&remote.peer_id))\n        .filter(|remote| selected_locator_peer_can_bridge(remote, local_height))\n        .filter(|remote| remote.selected_height.saturating_sub(local_height) >= minimum_gap)\n        .max_by_key(|remote| (remote.prune_boundary_height.is_some(), remote.selected_height))",
)
replace(
    main,
    "        .filter(|remote| !excluded_peers.contains(&remote.peer_id))\n        .filter(|remote| {\n            remote.selected_height > local_height",
    "        .filter(|remote| !excluded_peers.contains(&remote.peer_id))\n        .filter(|remote| selected_locator_peer_can_bridge(remote, local_height))\n        .filter(|remote| {\n            remote.selected_height > local_height",
)
replace(
    main,
    "        .max_by_key(|remote| remote.selected_height)\n        .map(|remote| remote.peer_id.clone())",
    "        .max_by_key(|remote| (remote.prune_boundary_height.is_some(), remote.selected_height))\n        .map(|remote| remote.peer_id.clone())",
)
replace(
    main,
    "            selected_height: Some(128),\n            selected_blue_score: Some(128),",
    "            selected_height: Some(128),\n            prune_boundary_height: Some(0),\n            selected_blue_score: Some(128),",
    expected=2,
)
replace(
    main,
    "    #[test]\n    fn tip_inventory_priority_selects_peer_before_generic_tip_fetch() {",
    '''    #[test]
    fn retained_history_boundary_tracks_continuous_selected_suffix() {
        let mut empty = pulsedag_core::genesis::init_chain_state("empty-boundary".to_string());
        empty.dag.selected_chain.clear();
        assert_eq!(local_retained_selected_history_boundary(&empty), None);

        let mut chain = pulsedag_core::genesis::init_chain_state("retained-boundary".to_string());
        let genesis = chain.dag.genesis_hash.clone();
        assert_eq!(local_retained_selected_history_boundary(&chain), Some(0));

        let b1 = test_orphan("b1", vec![&genesis], 1);
        let b2 = test_orphan("b2", vec!["b1"], 2);
        let b3 = test_orphan("b3", vec!["b2"], 3);
        for block in [&b1, &b2, &b3] {
            chain.dag.blocks.insert(block.hash.clone(), block.clone());
        }
        chain
            .dag
            .selected_parents
            .insert("b1".to_string(), Some(genesis.clone()));
        chain
            .dag
            .selected_parents
            .insert("b2".to_string(), Some("b1".to_string()));
        chain
            .dag
            .selected_parents
            .insert("b3".to_string(), Some("b2".to_string()));
        chain.dag.selected_chain = vec![
            genesis.clone(),
            "b1".to_string(),
            "b2".to_string(),
            "b3".to_string(),
        ];
        assert_eq!(local_retained_selected_history_boundary(&chain), Some(0));

        // A retained historical anchor below a gap must not make a pruned node
        // claim continuous history from genesis.
        chain.dag.blocks.remove("b1");
        chain.dag.selected_chain = vec![genesis.clone(), "b2".to_string(), "b3".to_string()];
        chain
            .dag
            .selected_parents
            .insert("b2".to_string(), Some(genesis));
        assert_eq!(local_retained_selected_history_boundary(&chain), Some(2));

        // Compact snapshots explicitly sever the selected-parent link at the
        // retained boundary; that must advertise the same boundary.
        chain.dag.selected_parents.insert("b2".to_string(), None);
        assert_eq!(local_retained_selected_history_boundary(&chain), Some(2));
    }

    #[test]
    fn retained_history_peer_selection_filters_incompatible_and_prefers_explicit() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                pulsedag_p2p::RemoteSelectedTipStatus {
                    peer_id: "legacy-higher".to_string(),
                    selected_height: 300,
                    prune_boundary_height: None,
                    connected: true,
                    direct_request_capable: true,
                    ..Default::default()
                },
                pulsedag_p2p::RemoteSelectedTipStatus {
                    peer_id: "pruned-compatible".to_string(),
                    selected_height: 220,
                    prune_boundary_height: Some(121),
                    connected: true,
                    direct_request_capable: true,
                    ..Default::default()
                },
                pulsedag_p2p::RemoteSelectedTipStatus {
                    peer_id: "pruned-incompatible".to_string(),
                    selected_height: 400,
                    prune_boundary_height: Some(122),
                    connected: true,
                    direct_request_capable: true,
                    ..Default::default()
                },
            ],
            ..P2pStatus::default()
        };

        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 120, 64, &HashSet::new()),
            Some(("pruned-compatible".to_string(), 220))
        );

        let all_incompatible = P2pStatus {
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "cannot-bridge".to_string(),
                selected_height: 400,
                prune_boundary_height: Some(122),
                connected: true,
                direct_request_capable: true,
                ..Default::default()
            }],
            ..P2pStatus::default()
        };
        assert_eq!(
            selected_locator_peer_for_priority_gap(
                &all_incompatible,
                120,
                64,
                &HashSet::new(),
            ),
            None
        );

        let legacy_only = P2pStatus {
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "legacy-fallback".to_string(),
                selected_height: 240,
                prune_boundary_height: None,
                connected: true,
                direct_request_capable: true,
                ..Default::default()
            }],
            ..P2pStatus::default()
        };
        assert_eq!(
            selected_locator_peer_for_priority_gap(&legacy_only, 120, 64, &HashSet::new()),
            Some(("legacy-fallback".to_string(), 240))
        );
    }

    #[test]
    fn reconcile_uses_same_retained_history_compatibility_rule() {
        let local = TipInventoryStatus {
            chain_id: "test-chain".to_string(),
            selected_tip: Some("local-tip".to_string()),
            selected_height: Some(120),
            prune_boundary_height: Some(0),
            selected_blue_score: Some(120),
            ordered_dag_tip: Some("local-tip".to_string()),
            state_root_digest: Some("local-root".to_string()),
            observed_at_unix: 1,
            inventory_generation: 1,
        };
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                pulsedag_p2p::RemoteSelectedTipStatus {
                    peer_id: "legacy-higher".to_string(),
                    selected_height: 300,
                    prune_boundary_height: None,
                    connected: true,
                    direct_request_capable: true,
                    ..Default::default()
                },
                pulsedag_p2p::RemoteSelectedTipStatus {
                    peer_id: "pruned-compatible".to_string(),
                    selected_height: 220,
                    prune_boundary_height: Some(121),
                    connected: true,
                    direct_request_capable: true,
                    ..Default::default()
                },
                pulsedag_p2p::RemoteSelectedTipStatus {
                    peer_id: "pruned-incompatible".to_string(),
                    selected_height: 400,
                    prune_boundary_height: Some(122),
                    connected: true,
                    direct_request_capable: true,
                    ..Default::default()
                },
            ],
            ..P2pStatus::default()
        };

        assert_eq!(
            selected_locator_peer_for_reconcile(&status, &local, &HashSet::new()),
            Some("pruned-compatible".to_string())
        );

        let incompatible_only = P2pStatus {
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "cannot-bridge".to_string(),
                selected_height: 400,
                prune_boundary_height: Some(122),
                connected: true,
                direct_request_capable: true,
                ..Default::default()
            }],
            ..P2pStatus::default()
        };
        assert_eq!(
            selected_locator_peer_for_reconcile(&incompatible_only, &local, &HashSet::new()),
            None
        );
    }

    #[test]
    fn tip_inventory_priority_selects_peer_before_generic_tip_fetch() {''',
)
"###,
    )
    .expect("write patch script");

    checked(
        Command::new("python3")
            .arg(&patch_script)
            .current_dir(&root),
    );
    checked(Command::new("cargo").args(["fmt", "--all"]).current_dir(&root));
    checked(Command::new("git").args(["diff", "--check"]).current_dir(&root));

    checked(
        Command::new("git")
            .args(["fetch", "origin", TARGET_BRANCH])
            .current_dir(&root),
    );
    let parent = String::from_utf8(
        checked_output(
            Command::new("git")
                .args(["rev-parse", "FETCH_HEAD"])
                .current_dir(&root),
        )
        .stdout,
    )
    .expect("utf8 parent sha");
    let parent = parent.trim();

    let worktree = env::temp_dir().join("pulsedag-824-publish");
    if worktree.exists() {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&worktree)
            .current_dir(&root)
            .status();
        let _ = fs::remove_dir_all(&worktree);
    }
    checked(
        Command::new("git")
            .args(["worktree", "add", "--detach"])
            .arg(&worktree)
            .arg(parent)
            .current_dir(&root),
    );

    for relative in [
        "crates/pulsedag-p2p/src/messages.rs",
        "crates/pulsedag-p2p/src/lib.rs",
        "apps/pulsedagd/src/main.rs",
    ] {
        copy_into_worktree(&root, &worktree, relative);
    }
    fs::remove_file(worktree.join("crates/pulsedag-p2p/build.rs"))
        .expect("remove temporary build script from published commit");

    checked(
        Command::new("git")
            .args([
                "add",
                "crates/pulsedag-p2p/src/messages.rs",
                "crates/pulsedag-p2p/src/lib.rs",
                "apps/pulsedagd/src/main.rs",
                "crates/pulsedag-p2p/build.rs",
            ])
            .current_dir(&worktree),
    );
    checked(
        Command::new("git")
            .args(["diff", "--cached", "--check"])
            .current_dir(&worktree),
    );
    checked(
        Command::new("git")
            .args(["config", "user.name", "github-actions[bot]"])
            .current_dir(&worktree),
    );
    checked(
        Command::new("git")
            .args([
                "config",
                "user.email",
                "41898282+github-actions[bot]@users.noreply.github.com",
            ])
            .current_dir(&worktree),
    );
    checked(
        Command::new("git")
            .args(["commit", "-m", "sync: advertise retained-history boundary (#824)"])
            .current_dir(&worktree),
    );
    checked(
        Command::new("git")
            .args(["push", "origin"])
            .arg(format!("HEAD:refs/heads/{TARGET_BRANCH}"))
            .current_dir(&worktree),
    );

    let _ = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(&worktree)
        .current_dir(&root)
        .status();
    println!("cargo:warning=#824 patch published to {TARGET_BRANCH}");
}
