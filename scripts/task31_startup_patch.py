#!/usr/bin/env python3
from pathlib import Path
import re

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    text = text.replace(old, new, 1)


replace_once(
    "mod config;\n",
    "mod config;\nmod startup_protocol;\n",
    "module declaration",
)
replace_once(
    "use config::Config;\n",
    "use config::Config;\nuse startup_protocol::select_startup_protocol;\n",
    "startup protocol import",
)

old_helper = '''fn startup_protocol_restore_identity(
    chain_id: &str,
    consensus_mode: pulsedag_core::ConsensusMode,
) -> Option<pulsedag_core::ProtocolActivationIdentity> {
    if consensus_mode != pulsedag_core::ConsensusMode::Legacy {
        return None;
    }
    let canonical_state = pulsedag_core::genesis::init_chain_state(chain_id.to_string());
    Some(pulsedag_core::ProtocolActivationIdentity::legacy_from_state(&canonical_state))
}

'''
if old_helper in text:
    text = text.replace(old_helper, "", 1)

old_tests = '''#[cfg(test)]
mod protocol_restore_startup_tests {
    use super::*;

    #[test]
    fn protocol_bound_startup_restore_is_legacy_only() {
        let legacy = startup_protocol_restore_identity(
            "pulsedag-testnet",
            pulsedag_core::ConsensusMode::Legacy,
        )
        .expect("legacy startup must derive a protocol restore identity");
        assert_eq!(legacy.chain_id, "pulsedag-testnet");

        assert!(startup_protocol_restore_identity(
            "pulsedag-testnet",
            pulsedag_core::ConsensusMode::GhostdagDev,
        )
        .is_none());
    }
}

'''
if old_tests in text:
    text = text.replace(old_tests, "", 1)

replace_once(
    '''    let storage = Arc::new(Storage::open(&cfg.rocksdb_path)?);
    if let Some(command) = snapshot_bundle_command {
        let protocol_identity =
            startup_protocol_restore_identity(&cfg.chain_id, cfg.consensus_mode);
        run_snapshot_bundle_command(&storage, &cfg.chain_id, protocol_identity.as_ref(), command)?;
        return Ok(());
    }
''',
    '''    let startup_protocol = select_startup_protocol(&cfg.chain_id, cfg.consensus_mode)?;
    let storage = Arc::new(Storage::open(&cfg.rocksdb_path)?);
    if let Some(command) = snapshot_bundle_command {
        run_snapshot_bundle_command(
            &storage,
            &cfg.chain_id,
            startup_protocol.restore_identity.as_ref(),
            command,
        )?;
        return Ok(());
    }
''',
    "startup selection",
)

replace_once(
    '''    let startup_protocol_identity =
        startup_protocol_restore_identity(&cfg.chain_id, cfg.consensus_mode);
    let mut chain_state = match startup_protocol_identity.as_ref() {
        Some(expected) => storage.load_or_init_genesis_for_protocol(expected)?,
        None => storage.load_or_init_genesis(cfg.chain_id.clone())?,
    };
''',
    '''    let mut chain_state = if startup_protocol.activated_v2() {
        let expected = startup_protocol.restore_identity.as_ref().ok_or_else(|| {
            anyhow::anyhow!("activated-v2 startup selection is missing its protocol identity")
        })?;
        storage.load_or_init_activated_v2_p2p_runtime(expected)?.0
    } else {
        match startup_protocol.restore_identity.as_ref() {
            Some(expected) => storage.load_or_init_genesis_for_protocol(expected)?,
            None => storage.load_or_init_genesis(cfg.chain_id.clone())?,
        }
    };
''',
    "protocol-bound genesis load",
)

if "activated-v2 startup consistency check requires protocol-v2 replay" not in text:
    pattern = re.compile(
        r'(?P<i>^[ \t]+)chain_state = match startup_protocol_identity\.as_ref\(\) \{\n'
        r'(?P=i)[ \t]+Some\(expected\) => storage\.replay_blocks_or_init_for_protocol\(expected\)\?,\n'
        r'(?P=i)[ \t]+None => storage\.replay_blocks_or_init\(cfg\.chain_id\.clone\(\)\)\?,\n'
        r'(?P=i)\};',
        re.MULTILINE,
    )
    match = pattern.search(text)
    if match is None:
        raise SystemExit("fail-closed v2 rebuild boundary: semantic anchor not found")
    indent = match.group("i")
    replacement = (
        indent + "if startup_protocol.activated_v2() {\n"
        + indent + "    return Err(anyhow::anyhow!(\n"
        + indent + "        \"activated-v2 startup consistency check requires protocol-v2 replay; refusing legacy rebuild: {reason}\"\n"
        + indent + "    ));\n"
        + indent + "}\n"
        + indent + "chain_state = match startup_protocol.restore_identity.as_ref() {\n"
        + indent + "    Some(expected) => storage.replay_blocks_or_init_for_protocol(expected)?,\n"
        + indent + "    None => storage.replay_blocks_or_init(cfg.chain_id.clone())?,\n"
        + indent + "};"
    )
    text = pattern.sub(replacement, text, count=1)

replace_once(
    '''        };
        if let Ok(status) = stack.handle.status() {
''',
    '''        };
        if let Some(capabilities) = startup_protocol.local_capabilities.clone() {
            stack
                .handle
                .configure_protocol_capabilities_v1(capabilities)?;
        }
        if let Ok(status) = stack.handle.status() {
''',
    "p2p capability configuration",
)

replace_once(
    '''    let startup_local_protocol_capabilities = match p2p.as_ref() {
        Some(p2p_handle) => p2p_handle
            .local_protocol_capabilities_v1()
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed reading local protocol capabilities for activated-v2 runtime restore: {error}"
                )
            })?,
        None => None,
    };
''',
    '''    let startup_local_protocol_capabilities = match p2p.as_ref() {
        Some(p2p_handle) => {
            let observed = p2p_handle
                .local_protocol_capabilities_v1()
                .map_err(|error| {
                    anyhow::anyhow!(
                        "failed reading local protocol capabilities for activated-v2 runtime restore: {error}"
                    )
                })?;
            if observed != startup_protocol.local_capabilities {
                return Err(anyhow::anyhow!(
                    "configured P2P protocol capabilities do not match the explicit startup protocol selection"
                ));
            }
            observed
        }
        None => startup_protocol.local_capabilities.clone(),
    };
''',
    "p2p capability verification",
)

replace_once(
    '''        let chain_id = cfg.chain_id.clone();
        let protocol_restore_identity =
            startup_protocol_restore_identity(&cfg.chain_id, cfg.consensus_mode);
''',
    '''        let chain_id = cfg.chain_id.clone();
        let activated_v2_protocol = startup_protocol.activated_v2();
        let protocol_restore_identity = startup_protocol.restore_identity.clone();
''',
    "auto-prune protocol identity",
)

replace_once(
    '''                        rt.last_snapshot_height,
                        rt.auto_prune_enabled,
                        rt.auto_prune_every_blocks,
''',
    '''                        rt.last_snapshot_height,
                        rt.auto_prune_enabled && !activated_v2_protocol,
                        rt.auto_prune_every_blocks,
''',
    "disable activated-v2 auto-prune",
)

if "startup_protocol_restore_identity" in text:
    raise SystemExit("legacy startup protocol helper reference remains after patch")

path.write_text(text, encoding="utf-8")

# Fix authoritative v2 replay so a clean tx2/header2 chain is never rebuilt
# from the historical tx1/header1 genesis UTXO.
replay_path = Path("crates/pulsedag-core/src/state_replay_v2.rs")
replay = replay_path.read_text(encoding="utf-8")

if "fn replay_base_state_for_genesis" not in replay:
    old_imports = '''    genesis::init_chain_state,
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2, GHOSTDAG_V1_ORDERING_VERSION},
    state::{ChainState, UtxoState},
    types::Hash,
'''
    new_imports = '''    genesis::init_chain_state,
    genesis_v2::init_chain_state_v2,
    ordering_v2::{derive_ordered_dag_v2, OrderedDagV2, GHOSTDAG_V1_ORDERING_VERSION},
    protocol::{BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2},
    state::{ChainState, UtxoState},
    tx::{TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2},
    types::Hash,
'''
    count = replay.count(old_imports)
    if count != 1:
        raise SystemExit(f"v2 replay imports: expected exactly one anchor, found {count}")
    replay = replay.replace(old_imports, new_imports, 1)

    anchor = '''pub struct StateReplayV2 {
    pub utxo: UtxoState,
    pub ordered_dag: OrderedDagV2,
    pub diagnostics: StateReplayV2Diagnostics,
}

'''
    helper = anchor + '''fn replay_base_state_for_genesis(state: &ChainState) -> Result<ChainState, PulseError> {
    let genesis = state
        .dag
        .blocks
        .get(&state.dag.genesis_hash)
        .ok_or_else(|| PulseError::NonDeterministicState("v2 replay genesis block missing".into()))?;
    let first_tx = genesis.transactions.first().ok_or_else(|| {
        PulseError::NonDeterministicState("v2 replay genesis transaction missing".into())
    })?;
    if genesis
        .transactions
        .iter()
        .any(|tx| tx.version != first_tx.version)
    {
        return Err(PulseError::NonDeterministicState(
            "mixed transaction versions inside genesis are not supported".into(),
        ));
    }

    match (genesis.header.version, first_tx.version) {
        (BLOCK_HEADER_VERSION_V1, TRANSACTION_VERSION_V1) => {
            Ok(init_chain_state(state.chain_id.clone()))
        }
        (BLOCK_HEADER_VERSION_V2, TRANSACTION_VERSION_V2) => {
            let rebuilt = init_chain_state_v2(state.chain_id.clone())?;
            if rebuilt.dag.genesis_hash != state.dag.genesis_hash {
                return Err(PulseError::NonDeterministicState(format!(
                    "chain-bound v2 genesis mismatch: expected {}, rebuilt {}",
                    state.dag.genesis_hash, rebuilt.dag.genesis_hash
                )));
            }
            Ok(rebuilt)
        }
        (header_version, transaction_version) => Err(PulseError::NonDeterministicState(format!(
            "unsupported mixed genesis protocol versions: header={header_version} transaction={transaction_version}"
        ))),
    }
}

'''
    count = replay.count(anchor)
    if count != 1:
        raise SystemExit(f"v2 replay helper anchor: expected exactly one, found {count}")
    replay = replay.replace(anchor, helper, 1)

    old_base = '''    let mut rebuilt = init_chain_state(state.chain_id.clone());
    rebuilt.dag.consensus_mode = state.dag.consensus_mode;
'''
    new_base = '''    let mut rebuilt = replay_base_state_for_genesis(state)?;
    rebuilt.dag.consensus_mode = state.dag.consensus_mode;
'''
    count = replay.count(old_base)
    if count != 1:
        raise SystemExit(f"v2 replay base state: expected exactly one anchor, found {count}")
    replay = replay.replace(old_base, new_base, 1)

if "clean_chain_bound_v2_genesis_replays_from_v2_utxo" not in replay:
    test_anchor = '''    #[test]
    fn conflicting_transaction_is_atomic_when_later_input_is_missing() {
'''
    test = '''    #[test]
    fn clean_chain_bound_v2_genesis_replays_from_v2_utxo() {
        let state = crate::genesis_v2::init_chain_state_v2(
            "pulsedag-private-v2.4.0".to_string(),
        )
        .unwrap();
        let replay = rebuild_authoritative_state_v2(&state).unwrap();
        let expected_root = state.utxo.compute_state_root().unwrap();
        let genesis_txid = state.dag.blocks[&state.dag.genesis_hash].transactions[0]
            .txid
            .clone();

        assert_eq!(replay.diagnostics.state_root, expected_root);
        assert!(replay
            .utxo
            .utxos
            .keys()
            .any(|outpoint| outpoint.txid == genesis_txid));
        verify_authoritative_state_snapshot_v2(&state).unwrap();
    }

    #[test]
    fn mixed_genesis_protocol_versions_fail_closed() {
        let mut state = crate::genesis_v2::init_chain_state_v2(
            "pulsedag-private-v2.4.0".to_string(),
        )
        .unwrap();
        let genesis = state.dag.genesis_hash.clone();
        state.dag.blocks.get_mut(&genesis).unwrap().header.version = BLOCK_HEADER_VERSION_V1;

        let error = rebuild_authoritative_state_v2(&state)
            .expect_err("mixed genesis protocol versions must fail closed");
        assert!(error.to_string().contains("mixed genesis protocol versions"));
    }

''' + test_anchor
    count = replay.count(test_anchor)
    if count != 1:
        raise SystemExit(f"v2 replay test anchor: expected exactly one, found {count}")
    replay = replay.replace(test_anchor, test, 1)

replay_path.write_text(replay, encoding="utf-8")
