from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"expected patch anchor missing in {path}: {old[:100]!r}")
    if text.count(old) != 1:
        raise SystemExit(f"patch anchor not unique in {path}: {text.count(old)} matches")
    p.write_text(text.replace(old, new, 1))


messages = "crates/pulsedag-p2p/src/messages.rs"
replace_once(
    messages,
    "    pub selected_height: Option<u64>,\n    pub selected_blue_score: Option<u64>,",
    "    pub selected_height: Option<u64>,\n    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n    pub prune_boundary_height: Option<u64>,\n    pub selected_blue_score: Option<u64>,",
)
with open(messages, "a") as f:
    f.write(r'''

#[cfg(test)]
mod prune_boundary_wire_compat_tests {
    use super::*;

    #[test]
    fn legacy_tip_inventory_without_prune_boundary_decodes_as_unknown() {
        let json = r#"{
            "chain_id":"legacy-chain",
            "selected_tip":null,
            "selected_height":42,
            "selected_blue_score":null,
            "ordered_dag_tip":null,
            "state_root_digest":null,
            "observed_at_unix":1,
            "inventory_generation":2
        }"#;
        let decoded: TipInventoryStatus = serde_json::from_str(json).expect("legacy inventory");
        assert_eq!(decoded.prune_boundary_height, None);
    }

    #[test]
    fn archival_tip_inventory_serializes_zero_boundary_explicitly() {
        let inventory = TipInventoryStatus {
            chain_id: "archive-chain".to_string(),
            selected_height: Some(10),
            prune_boundary_height: Some(0),
            ..TipInventoryStatus::default()
        };
        let json = serde_json::to_value(&inventory).expect("inventory json");
        assert_eq!(json.get("prune_boundary_height").and_then(|v| v.as_u64()), Some(0));
    }
}
''')

p2p = "crates/pulsedag-p2p/src/lib.rs"
replace_once(
    p2p,
    "    pub selected_height: u64,\n    pub selected_blue_score: Option<u64>,",
    "    pub selected_height: u64,\n    #[serde(default)]\n    pub prune_boundary_height: Option<u64>,\n    pub selected_blue_score: Option<u64>,",
)
replace_once(
    p2p,
    "                    selected_tip: inventory.selected_tip,\n                    selected_height,\n                    selected_blue_score: inventory.selected_blue_score,",
    "                    selected_tip: inventory.selected_tip,\n                    selected_height,\n                    prune_boundary_height: inventory.prune_boundary_height,\n                    selected_blue_score: inventory.selected_blue_score,",
)

main = "apps/pulsedagd/src/main.rs"
old_local = '''fn local_tip_inventory_status(chain: &pulsedag_core::ChainState) -> TipInventoryStatus {
    let selected_tip = pulsedag_core::preferred_tip_hash(chain);
    let selected_block = selected_tip
        .as_ref()
        .and_then(|tip| chain.dag.blocks.get(tip));
    TipInventoryStatus {
        chain_id: chain.chain_id.clone(),
        selected_tip,
        selected_height: selected_block.map(|block| block.header.height),
        selected_blue_score: selected_block.map(|block| block.header.blue_score),
        ordered_dag_tip: chain.dag.ordered_dag_tip.clone(),
        state_root_digest: chain.dag.ordered_dag_state_root.clone(),
        observed_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        inventory_generation: 0,
    }
}
'''
new_local = '''fn retained_selected_history_boundary(chain: &pulsedag_core::ChainState) -> Option<u64> {
    let mut current = pulsedag_core::preferred_tip_hash(chain)?;

    loop {
        let block = chain.dag.blocks.get(&current)?;
        let boundary_height = block.header.height;
        if current == chain.dag.genesis_hash {
            return Some(0);
        }

        let Some(parent) = chain
            .dag
            .selected_parents
            .get(&current)
            .and_then(|parent| parent.as_ref())
        else {
            return Some(boundary_height);
        };
        let Some(parent_block) = chain.dag.blocks.get(parent) else {
            return Some(boundary_height);
        };
        if parent_block.header.height.saturating_add(1) != block.header.height {
            return Some(boundary_height);
        }
        current = parent.clone();
    }
}

fn local_tip_inventory_status(chain: &pulsedag_core::ChainState) -> TipInventoryStatus {
    let selected_tip = pulsedag_core::preferred_tip_hash(chain);
    let selected_block = selected_tip
        .as_ref()
        .and_then(|tip| chain.dag.blocks.get(tip));
    TipInventoryStatus {
        chain_id: chain.chain_id.clone(),
        selected_tip,
        selected_height: selected_block.map(|block| block.header.height),
        prune_boundary_height: retained_selected_history_boundary(chain),
        selected_blue_score: selected_block.map(|block| block.header.blue_score),
        ordered_dag_tip: chain.dag.ordered_dag_tip.clone(),
        state_root_digest: chain.dag.ordered_dag_state_root.clone(),
        observed_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        inventory_generation: 0,
    }
}
'''
replace_once(main, old_local, new_local)

old_priority = '''fn selected_locator_peer_for_priority_gap(
    status: &P2pStatus,
    local_height: u64,
    minimum_gap: u64,
    excluded_peers: &HashSet<String>,
) -> Option<(String, u64)> {
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| !excluded_peers.contains(&remote.peer_id))
        .filter(|remote| remote.selected_height.saturating_sub(local_height) >= minimum_gap)
        .max_by_key(|remote| remote.selected_height)
        .map(|remote| (remote.peer_id.clone(), remote.selected_height))
}
'''
new_priority = '''fn selected_history_peer_compatible(
    remote: &pulsedag_p2p::RemoteSelectedTipStatus,
    local_height: u64,
) -> bool {
    remote
        .prune_boundary_height
        .map_or(true, |boundary| boundary <= local_height.saturating_add(1))
}

fn selected_locator_peer_for_priority_gap(
    status: &P2pStatus,
    local_height: u64,
    minimum_gap: u64,
    excluded_peers: &HashSet<String>,
) -> Option<(String, u64)> {
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| !excluded_peers.contains(&remote.peer_id))
        .filter(|remote| remote.selected_height.saturating_sub(local_height) >= minimum_gap)
        .filter(|remote| selected_history_peer_compatible(remote, local_height))
        .max_by_key(|remote| (remote.prune_boundary_height.is_some(), remote.selected_height))
        .map(|remote| (remote.peer_id.clone(), remote.selected_height))
}
'''
replace_once(main, old_priority, new_priority)

old_reconcile = '''fn selected_locator_peer_for_reconcile(
    status: &P2pStatus,
    local: &TipInventoryStatus,
    excluded_peers: &HashSet<String>,
) -> Option<String> {
    let local_height = local.selected_height.unwrap_or_default();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| !excluded_peers.contains(&remote.peer_id))
        .filter(|remote| {
            remote.selected_height > local_height
                || (remote.selected_height == local_height
                    && (remote.selected_tip != local.selected_tip
                        || remote.ordered_dag_tip != local.ordered_dag_tip
                        || remote.state_root_digest != local.state_root_digest))
        })
        .max_by_key(|remote| remote.selected_height)
        .map(|remote| remote.peer_id.clone())
}
'''
new_reconcile = '''fn selected_locator_peer_for_reconcile(
    status: &P2pStatus,
    local: &TipInventoryStatus,
    excluded_peers: &HashSet<String>,
) -> Option<String> {
    let local_height = local.selected_height.unwrap_or_default();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| !excluded_peers.contains(&remote.peer_id))
        .filter(|remote| selected_history_peer_compatible(remote, local_height))
        .filter(|remote| {
            remote.selected_height > local_height
                || (remote.selected_height == local_height
                    && (remote.selected_tip != local.selected_tip
                        || remote.ordered_dag_tip != local.ordered_dag_tip
                        || remote.state_root_digest != local.state_root_digest))
        })
        .max_by_key(|remote| (remote.prune_boundary_height.is_some(), remote.selected_height))
        .map(|remote| remote.peer_id.clone())
}
'''
replace_once(main, old_reconcile, new_reconcile)

with open(main, "a") as f:
    f.write(r'''

#[cfg(test)]
mod prune_boundary_peer_selection_tests {
    use super::*;

    fn remote(
        peer_id: &str,
        selected_height: u64,
        prune_boundary_height: Option<u64>,
    ) -> pulsedag_p2p::RemoteSelectedTipStatus {
        pulsedag_p2p::RemoteSelectedTipStatus {
            peer_id: peer_id.to_string(),
            selected_height,
            prune_boundary_height,
            connected: true,
            direct_request_capable: true,
            ..Default::default()
        }
    }

    #[test]
    fn prune_boundary_priority_selection_rejects_incompatible_and_prefers_known_compatible() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("incompatible", 500, Some(100)),
                remote("legacy-unknown", 450, None),
                remote("archival", 300, Some(0)),
            ],
            ..Default::default()
        };
        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 0, 1, &HashSet::new()),
            Some(("archival".to_string(), 300))
        );
    }

    #[test]
    fn prune_boundary_priority_selection_keeps_unknown_as_fallback() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![remote("legacy-unknown", 50, None)],
            ..Default::default()
        };
        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 0, 1, &HashSet::new()),
            Some(("legacy-unknown".to_string(), 50))
        );
    }

    #[test]
    fn prune_boundary_priority_selection_accepts_pruned_peer_with_overlap() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![remote("overlap", 250, Some(151))],
            ..Default::default()
        };
        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 150, 1, &HashSet::new()),
            Some(("overlap".to_string(), 250))
        );
    }

    #[test]
    fn prune_boundary_reconcile_uses_same_compatibility_rule() {
        let local = TipInventoryStatus {
            selected_height: Some(0),
            ..Default::default()
        };
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("incompatible", 500, Some(100)),
                remote("legacy-unknown", 450, None),
                remote("archival", 300, Some(0)),
            ],
            ..Default::default()
        };
        assert_eq!(
            selected_locator_peer_for_reconcile(&status, &local, &HashSet::new()),
            Some("archival".to_string())
        );
    }
}
''')

Path(".github/workflows/apply-824-patch.yml").unlink(missing_ok=True)
Path("scripts/apply_824_patch.py").unlink(missing_ok=True)
