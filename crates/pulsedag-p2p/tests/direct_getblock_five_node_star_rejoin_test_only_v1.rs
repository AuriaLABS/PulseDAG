//! TEST-ONLY five separate real TCP/Noise Swarms and 32 distinct accepted PoW
//! GetBlock recoveries on the isolated fifth node, after a real disconnect.
//! All four leaves stay authenticated to the exact seed identity, no gossip.
//!
//! This intentionally exercises the existing v2 Core acceptance fixture, NOT
//! authoritative native-v3 multiparent/GD-08, monetary settlement or a public
//! testnet. Production direct GetBlock remains Outbound-only and hard OFF.

use std::time::{Duration, Instant};

use libp2p::futures::{future::select_all, StreamExt};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use pulsedag_core::accept::{accept_block, AcceptSource};
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    build_candidate_block, build_coinbase_transaction, dev_mine_header, expected_difficulty,
    refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
};
use pulsedag_p2p::messages::direct_getblock_codec_v1::{
    DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1,
};
use pulsedag_p2p::messages::direct_getblock_pending_v1::{
    DIRECT_GETBLOCK_REQUEST_TTL, MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL,
};
use pulsedag_p2p::messages::direct_getblock_response_channel_v1::DirectGetBlockLibp2pResponseChannelsV1;
use pulsedag_p2p::messages::direct_getblock_swarm_bindings_v1::DirectGetBlockSwarmBindingsV1;
use pulsedag_p2p::{InboundEvent, PeerAddressedBlockRequest};

const CHAIN: &str = "offline-five-real-noise-accepted-pow-test-only";
const BLOCKS: usize = 32;
const ISOLATED: usize = 4;
type DirectSwarm = Swarm<request_response::Behaviour<DirectGetBlockRawCodecV1>>;

fn swarm(key: identity::Keypair, support: ProtocolSupport) -> DirectSwarm {
    SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("real local TCP/Noise/Yamux")
        .with_behaviour(move |_| {
            request_response::Behaviour::with_codec(
                DirectGetBlockRawCodecV1,
                [(
                    StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1),
                    support,
                )],
                request_response::Config::default()
                    .with_request_timeout(DIRECT_GETBLOCK_REQUEST_TTL)
                    .with_max_concurrent_streams(MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL),
            )
        })
        .expect("bounded request-response test behaviour")
        .build()
}

fn issue(
    leaf: &mut DirectSwarm,
    binding: &mut DirectGetBlockSwarmBindingsV1,
    seed: PeerId,
    hash: String,
    sequence: usize,
) {
    let original = PeerAddressedBlockRequest {
        request_id: format!("getblock:five-node:isolated:{sequence}"),
        requested_peer_id: seed.to_string(),
        block_hash: hash,
        sent_at: 0,
    };
    binding
        .issue_daemon_request_with(
            &original,
            leaf.is_connected(&seed),
            Instant::now(),
            |peer, wire| leaf.behaviour_mut().send_request(peer, wire),
        )
        .expect("bind exact authenticated daemon request to Noise transport");
}

#[tokio::test(flavor = "current_thread")]
async fn five_authenticated_noise_peers_rejoin_and_recover_32_distinct_accepted_pow_blocks() {
    // The fifth peer must be physically isolated BEFORE any post-genesis blocks are mined.
    // This remains a legacy Core fixture, not native-v3 production daemon proof.
    let mut accepted = init_chain_state(CHAIN.to_string());
    let mut hashes: Vec<String> = Vec::with_capacity(BLOCKS);
    assert!(
        !accepted.contracts.config.enabled,
        "contracts must stay inactive"
    );
    assert_eq!(accepted.dag.best_height, 0, "only genesis before isolation");

    tokio::time::timeout(Duration::from_secs(145), async {
        let keys = (0..5)
            .map(|_| identity::Keypair::generate_ed25519())
            .collect::<Vec<_>>();
        let peers = keys
            .iter()
            .map(|key| PeerId::from(key.public()))
            .collect::<Vec<_>>();
        let mut swarms = keys
            .into_iter()
            .enumerate()
            .map(|(index, key)| {
                swarm(
                    key,
                    if index == 0 {
                        ProtocolSupport::Inbound
                    } else {
                        ProtocolSupport::Outbound
                    },
                )
            })
            .collect::<Vec<_>>();

        let mut inbound = DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, peers[0])
            .expect("seed accepted-DAG authority");
        let mut outbound = DirectGetBlockSwarmBindingsV1::new(CHAIN, peers[ISOLATED])
            .expect("fifth peer daemon request correlation");
        let mut restored = init_chain_state(CHAIN.to_string());
        let genesis = restored.dag.genesis_hash.clone();
        let mut seed_up = [false; 5];
        let mut leaf_up = [false; 5];
        let mut listener: Option<Multiaddr> = None;
        let mut state = 0_u8; // 0 connect, 1 genesis, 2 disconnect, 3 rejoin, 4 recover
        let mut requested = 0_usize;
        let mut served = 0_usize;
        let mut delivered = 0_usize;
        let mut physically_closed_seed = false;
        let mut physically_closed_leaf = false;

        swarms[0]
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .expect("ephemeral loopback only, no live deployment");

        while delivered < BLOCKS + 1 {
            let next_event = select_all(swarms.iter_mut().enumerate().map(|(index, swarm)| {
                Box::pin(async move { (index, swarm.select_next_some().await) })
            }));
            let ((index, event), _, _) = next_event.await;
            match event {
                SwarmEvent::NewListenAddr { address, .. } if index == 0 && listener.is_none() => {
                    listener = Some(address.clone());
                    for leaf in swarms.iter_mut().skip(1) {
                        leaf.dial(address.clone().with(Protocol::P2p(peers[0])))
                            .expect("each leaf dials exact Noise seed PeerId");
                    }
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    if index == 0 {
                        let leaf = peers
                            .iter()
                            .position(|id| *id == peer_id)
                            .expect("unrecognized authenticated seed peer");
                        assert!(leaf > 0);
                        seed_up[leaf] = true;
                    } else {
                        assert_eq!(peer_id, peers[0], "leaf must connect only to seed");
                        leaf_up[index] = true;
                    }
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    if index == 0 {
                        assert_eq!(peer_id, peers[ISOLATED], "only fifth peer may disconnect");
                        seed_up[ISOLATED] = false;
                        physically_closed_seed = true;
                    } else if index == ISOLATED {
                        assert_eq!(peer_id, peers[0]);
                        leaf_up[ISOLATED] = false;
                        physically_closed_leaf = true;
                        outbound.last_connection_closed(&peer_id);
                        assert!(outbound.is_empty(), "disconnect cannot leak a bound ticket");
                    } else {
                        panic!("healthy leaf {index} unexpectedly disconnected");
                    }
                }
                SwarmEvent::Behaviour(request_response::Event::Message {
                    peer,
                    message:
                        request_response::Message::Request {
                            request, channel, ..
                        },
                    ..
                }) => {
                    assert_eq!(index, 0, "only test seed serves inbound GetBlock");
                    assert_eq!(peer, peers[ISOLATED], "no unauthorized request author");
                    let ticket = inbound
                        .accept(&request, &peer, channel, Instant::now())
                        .expect("accept actual authenticated Noise inbound request");
                    inbound
                        .respond_from_accepted_dag(
                            ticket.local_token,
                            &peer,
                            &accepted,
                            Instant::now(),
                            |channel, bytes| {
                                swarms[0]
                                    .behaviour_mut()
                                    .send_response(channel, bytes)
                                    .is_ok()
                            },
                        )
                        .expect("respond from exact accepted Core DAG only");
                    assert!(inbound.is_empty(), "no leaked one-shot response channel");
                    served += 1;
                }
                SwarmEvent::Behaviour(request_response::Event::Message {
                    peer,
                    message:
                        request_response::Message::Response {
                            request_id,
                            response,
                        },
                    ..
                }) => {
                    assert_eq!(index, ISOLATED, "only isolated leaf requested blocks");
                    assert_eq!(peer, peers[0], "response must be from true seed");
                    let response_event = outbound
                        .deliver_bound_response_to_daemon(
                            &request_id,
                            &peer,
                            &response,
                            Instant::now(),
                        )
                        .expect("exact peer/hash/transport correlation required");
                    let block = match response_event {
                        InboundEvent::Block {
                            peer_id: Some(authenticated),
                            block,
                        } => {
                            assert_eq!(authenticated, peers[0].to_string());
                            block
                        }
                        other => panic!("no missing, forwarded or unsolicited response: {other:?}"),
                    };
                    if delivered == 0 {
                        assert_eq!(state, 1);
                        assert_eq!(block.hash, genesis, "first request checks genesis");
                        swarms[ISOLATED]
                            .disconnect_peer_id(peers[0])
                            .expect("force actual TCP/Noise disconnect");
                        state = 2;
                    } else {
                        assert_eq!(state, 4, "block recovery requires physical rejoin");
                        assert_eq!(
                            block.hash,
                            hashes[delivered - 1],
                            "recovery order must be exact and unique"
                        );
                        assert_eq!(block.header.height, delivered as u64);
                        accept_block(block, &mut restored, AcceptSource::P2p)
                            .expect("fifth Core must accept real PoW over Noise");
                    }
                    delivered += 1;
                    assert!(
                        outbound.is_empty(),
                        "one response consumes one local ticket"
                    );
                    if state == 4 && delivered < BLOCKS + 1 {
                        requested += 1;
                        issue(
                            &mut swarms[ISOLATED],
                            &mut outbound,
                            peers[0],
                            hashes[delivered - 1].clone(),
                            requested,
                        );
                    }
                }
                SwarmEvent::Behaviour(request_response::Event::InboundFailure {
                    peer,
                    error,
                    ..
                }) => {
                    panic!("unexpected Noise inbound failure from {peer}: {error:?}");
                }
                SwarmEvent::Behaviour(request_response::Event::OutboundFailure {
                    peer,
                    error,
                    ..
                }) => {
                    panic!("unexpected Noise outbound failure to {peer}: {error:?}");
                }
                _ => {}
            }

            if state == 0
                && seed_up[1..].iter().all(|connected| *connected)
                && leaf_up[1..].iter().all(|connected| *connected)
            {
                requested += 1;
                issue(
                    &mut swarms[ISOLATED],
                    &mut outbound,
                    peers[0],
                    genesis.clone(),
                    requested,
                );
                state = 1;
            }
            if state == 2 && physically_closed_seed && physically_closed_leaf {
                assert!(!seed_up[ISOLATED] && !leaf_up[ISOLATED]);
                assert!(
                    hashes.is_empty(),
                    "no blocks can be pre-mined before physical isolation"
                );
                for leaf in 1..ISOLATED {
                    assert!(swarms[0].is_connected(&peers[leaf]));
                    assert!(swarms[leaf].is_connected(&peers[0]));
                }
                // Mine 32 genuine legacy Core-accepted PoW blocks only while
                // peer 5 is disconnected on BOTH authenticated Noise ends.
                for height in 1..=BLOCKS {
                    let parent = hashes
                        .last()
                        .cloned()
                        .unwrap_or_else(|| accepted.dag.genesis_hash.clone());
                    let parent_time = accepted
                        .dag
                        .blocks
                        .get(&parent)
                        .expect("parent already accepted")
                        .header
                        .timestamp;
                    let mut block = build_candidate_block(
                        vec![parent],
                        height as u64,
                        expected_difficulty(&accepted),
                        vec![build_coinbase_transaction(
                            "five-noise-rejoin-miner",
                            50,
                            height as u64,
                        )],
                    );
                    block.header.timestamp = parent_time.saturating_add(60).max(1);
                    refresh_block_consensus_ids_with_state(&mut block, &accepted)
                        .expect("exact expected state root before PoW");
                    let (mined_header, found, _, _) =
                        dev_mine_header(block.header.clone(), 200_000);
                    assert!(found, "must mine the distinct actual PoW block");
                    block.header = mined_header;
                    refresh_block_consensus_ids(&mut block);
                    accept_block(block.clone(), &mut accepted, AcceptSource::LocalMining)
                        .expect("source Core must really admit each block");
                    hashes.push(block.hash);
                }
                assert_eq!(accepted.dag.best_height, BLOCKS as u64);
                assert_eq!(
                    hashes
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                    BLOCKS,
                    "all requested hashes must be distinct, not one repeated genesis"
                );

                swarms[ISOLATED]
                    .dial(
                        listener
                            .as_ref()
                            .expect("known ephemeral seed listener")
                            .clone()
                            .with(Protocol::P2p(peers[0])),
                    )
                    .expect("redial exact Noise identity after isolation");
                state = 3;
            }
            if state == 3 && seed_up[ISOLATED] && leaf_up[ISOLATED] {
                assert_eq!(delivered, 1, "no post-isolation block before rejoin");
                requested += 1;
                issue(
                    &mut swarms[ISOLATED],
                    &mut outbound,
                    peers[0],
                    hashes[0].clone(),
                    requested,
                );
                state = 4;
            }
        }

        assert!(physically_closed_seed && physically_closed_leaf);
        assert_eq!(state, 4);
        assert_eq!(requested, BLOCKS + 1);
        assert_eq!(served, BLOCKS + 1, "all real Noise requests reach seed");
        assert_eq!(delivered, BLOCKS + 1);
        assert!(outbound.is_empty());
        assert!(inbound.is_empty());
        for leaf in 1..5 {
            assert!(swarms[0].is_connected(&peers[leaf]));
            assert!(swarms[leaf].is_connected(&peers[0]));
        }
        assert_eq!(restored.dag.best_height, BLOCKS as u64);
        assert_eq!(restored.dag.selected_chain, accepted.dag.selected_chain);
        assert_eq!(
            restored.utxo.compute_state_root().unwrap(),
            accepted.utxo.compute_state_root().unwrap(),
            "recovered Core UTXO must match source after 32 unique blocks"
        );
        assert!(!restored.contracts.config.enabled);
    })
    .await
    .expect("test-only five authenticated Noise peers did not rejoin and recover");
}
