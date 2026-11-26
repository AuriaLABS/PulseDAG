//! Strict TEST-ONLY full-duplex GetBlock exchange over two real Noise Swarms.
//! This file is never used by deployed PulseBehaviour, whose GetBlock
//! request_response protocol remains Outbound-only and sender hard OFF.
//! No public testnet, private deployment, monetary mining or contracts enabled.

use std::time::{Duration, Instant};

use libp2p::futures::StreamExt;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use pulsedag_core::genesis::init_chain_state;
use pulsedag_p2p::messages::direct_getblock_codec_v1::{
    DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1,
};
use pulsedag_p2p::messages::direct_getblock_pending_v1::{
    DIRECT_GETBLOCK_REQUEST_TTL, MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL,
};
use pulsedag_p2p::messages::direct_getblock_response_channel_v1::DirectGetBlockLibp2pResponseChannelsV1;
use pulsedag_p2p::messages::direct_getblock_swarm_bindings_v1::DirectGetBlockSwarmBindingsV1;
use pulsedag_p2p::{InboundEvent, PeerAddressedBlockRequest};

const CHAIN: &str = "offline-two-way-noise-getblock-v3";
type DirectSwarm = Swarm<request_response::Behaviour<DirectGetBlockRawCodecV1>>;

fn full_duplex_test_swarm(key: identity::Keypair) -> DirectSwarm {
    SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("real TCP+Noise+Yamux test transport")
        .with_behaviour(|_| {
            request_response::Behaviour::with_codec(
                DirectGetBlockRawCodecV1,
                [(
                    StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1),
                    ProtocolSupport::Full,
                )],
                request_response::Config::default()
                    .with_request_timeout(DIRECT_GETBLOCK_REQUEST_TTL)
                    .with_max_concurrent_streams(MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL),
            )
        })
        .expect("real request-response test behaviour")
        .build()
}

#[tokio::test(flavor = "current_thread")]
async fn both_authenticated_noise_peers_request_and_serve_accepted_dag_without_broadcast() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let a_key = identity::Keypair::generate_ed25519();
        let b_key = identity::Keypair::generate_ed25519();
        let a_id = PeerId::from(a_key.public());
        let b_id = PeerId::from(b_key.public());
        assert_ne!(a_id, b_id);

        let accepted = init_chain_state(CHAIN.to_string());
        let genesis = accepted.dag.genesis_hash.clone();
        let mut a = full_duplex_test_swarm(a_key);
        let mut b = full_duplex_test_swarm(b_key);
        let mut a_inbound =
            DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, a_id).unwrap();
        let mut b_inbound =
            DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, b_id).unwrap();
        let mut a_outbound = DirectGetBlockSwarmBindingsV1::new(CHAIN, a_id).unwrap();
        let mut b_outbound = DirectGetBlockSwarmBindingsV1::new(CHAIN, b_id).unwrap();

        let mut a_started = false;
        let mut b_started = false;
        let mut a_received = false;
        let mut b_received = false;
        let mut a_requests_served = 0usize;
        let mut b_requests_served = 0usize;
        let listen: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        a.listen_on(listen).expect("test-only ephemeral loopback listener");

        while !(a_received && b_received) {
            tokio::select! {
                event = a.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if !b_started => {
                        // Connect to the exact authenticated Noise PeerId only.
                        b.dial(address.with(Protocol::P2p(a_id)))
                            .expect("B dials exact authentic A on loopback");
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !a_started => {
                        assert_eq!(peer_id, b_id);
                        a_started = true;
                        let original = PeerAddressedBlockRequest {
                            request_id: "getblock:daemon-full:a".to_string(),
                            requested_peer_id: b_id.to_string(),
                            block_hash: genesis.clone(),
                            sent_at: 0,
                        };
                        a_outbound
                            .issue_daemon_request_with(
                                &original,
                                a.is_connected(&b_id),
                                Instant::now(),
                                |peer, raw| a.behaviour_mut().send_request(peer, raw),
                            )
                            .expect("A issues one bounded authenticated request to B");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, b_id);
                        a_requests_served += 1;
                        let ticket = a_inbound
                            .accept(&request, &peer, channel, Instant::now())
                            .expect("A admits only authenticated B");
                        a_inbound
                            .respond_from_accepted_dag(
                                ticket.local_token,
                                &peer,
                                &accepted,
                                Instant::now(),
                                |channel, raw| a.behaviour_mut().send_response(channel, raw).is_ok(),
                            )
                            .expect("A serves from accepted DAG only");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, b_id);
                        assert!(!a_received, "A must receive its response once");
                        let ingress = a_outbound
                            .deliver_bound_response_to_daemon(
                                &request_id, &peer, &response, Instant::now(),
                            )
                            .expect("A accepts exact B-authenticated response");
                        match ingress {
                            InboundEvent::Block { peer_id: Some(actual_peer), block } => {
                                assert_eq!(actual_peer, b_id.to_string());
                                assert_eq!(block.hash, genesis);
                                a_received = true;
                            }
                            _ => panic!("A must receive a real accepted-DAG block"),
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("A real full-duplex Noise outbound stream failed: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("A real full-duplex Noise inbound stream failed: {error:?}");
                    }
                    _ => {}
                },
                event = b.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !b_started => {
                        assert_eq!(peer_id, a_id);
                        b_started = true;
                        let original = PeerAddressedBlockRequest {
                            request_id: "getblock:daemon-full:b".to_string(),
                            requested_peer_id: a_id.to_string(),
                            block_hash: genesis.clone(),
                            sent_at: 0,
                        };
                        b_outbound
                            .issue_daemon_request_with(
                                &original,
                                b.is_connected(&a_id),
                                Instant::now(),
                                |peer, raw| b.behaviour_mut().send_request(peer, raw),
                            )
                            .expect("B issues one bounded authenticated request to A");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, a_id);
                        b_requests_served += 1;
                        let ticket = b_inbound
                            .accept(&request, &peer, channel, Instant::now())
                            .expect("B admits only authenticated A");
                        b_inbound
                            .respond_from_accepted_dag(
                                ticket.local_token,
                                &peer,
                                &accepted,
                                Instant::now(),
                                |channel, raw| b.behaviour_mut().send_response(channel, raw).is_ok(),
                            )
                            .expect("B serves from accepted DAG only");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, a_id);
                        assert!(!b_received, "B must receive its response once");
                        let ingress = b_outbound
                            .deliver_bound_response_to_daemon(
                                &request_id, &peer, &response, Instant::now(),
                            )
                            .expect("B accepts exact A-authenticated response");
                        match ingress {
                            InboundEvent::Block { peer_id: Some(actual_peer), block } => {
                                assert_eq!(actual_peer, a_id.to_string());
                                assert_eq!(block.hash, genesis);
                                b_received = true;
                            }
                            _ => panic!("B must receive a real accepted-DAG block"),
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("B real full-duplex Noise outbound stream failed: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("B real full-duplex Noise inbound stream failed: {error:?}");
                    }
                    _ => {}
                },
            }
        }

        assert_eq!(a_requests_served, 1, "A must serve B exactly once");
        assert_eq!(b_requests_served, 1, "B must serve A exactly once");
        assert!(a_inbound.is_empty(), "A one-shot response channel consumed");
        assert!(b_inbound.is_empty(), "B one-shot response channel consumed");
        assert!(a_outbound.is_empty(), "A outbound correlation consumed");
        assert!(b_outbound.is_empty(), "B outbound correlation consumed");
    })
    .await
    .expect("two-way authenticated Noise exchange must finish under timeout");
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_noise_reconnect_uses_fresh_daemon_bound_getblock_without_gossip() {
    // An isolated live TCP/Noise/Yamux reconnection regression, NOT the
    // native-v3 five-node acceptance drill. Production still stays Outbound-only.
    tokio::time::timeout(Duration::from_secs(55), async {
        use libp2p::request_response::OutboundRequestId;
        use pulsedag_p2p::messages::direct_getblock_swarm_bindings_v1::DirectGetBlockSwarmBindingErrorV1;

        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        let accepted = init_chain_state(CHAIN.to_string());
        let genesis_hash = accepted.dag.genesis_hash.clone();
        let mut server = full_duplex_test_swarm(server_key);
        let mut client = full_duplex_test_swarm(client_key);
        let mut inbound = DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, server_id).unwrap();
        let mut outbound = DirectGetBlockSwarmBindingsV1::new(CHAIN, client_id).unwrap();

        let mut server_listen: Option<Multiaddr> = None;
        let mut issued = 0usize;
        let mut received = 0usize;
        let mut served = 0usize;
        let mut redialled = false;
        let mut first_transport_id: Option<OutboundRequestId> = None;
        let mut first_response: Option<Vec<u8>> = None;

        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .expect("only ephemeral local TCP listener");

        while received < 2 {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if server_listen.is_none() => {
                        server_listen = Some(address.clone());
                        client.dial(address.with(Protocol::P2p(server_id)))
                            .expect("dial exactly authenticated local server");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id, "Noise must authenticate the requesting peer");
                        let ticket = inbound.accept(&request, &peer, channel, Instant::now())
                            .expect("bounded chain/domain/peer verified inbound request");
                        served += 1;
                        inbound.respond_from_accepted_dag(
                            ticket.local_token,
                            &peer,
                            &accepted,
                            Instant::now(),
                            |channel, raw| server.behaviour_mut().send_response(channel, raw).is_ok(),
                        ).expect("answer from accepted DAG with one-shot response channel");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("authenticated server inbound failure: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("server unexpected outbound failure: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        assert_eq!(peer_id, server_id, "only exact Noise server allowed");
                        assert!(issued < 2, "must not issue duplicate GetBlock after reconnect");
                        issued += 1;
                        let original = PeerAddressedBlockRequest {
                            request_id: format!("getblock:reconnect-local:{issued}"),
                            requested_peer_id: peer_id.to_string(),
                            block_hash: genesis_hash.clone(),
                            sent_at: 0,
                        };
                        let transport_id = outbound.issue_daemon_request_with(
                            &original,
                            client.is_connected(&peer_id),
                            Instant::now(),
                            |peer, raw| client.behaviour_mut().send_request(peer, raw),
                        ).expect("real Noise send binds a fresh daemon-local request ID");
                        if issued == 1 {
                            first_transport_id = Some(transport_id);
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id);
                        let accepted_event = outbound.deliver_bound_response_to_daemon(
                            &request_id,
                            &peer,
                            &response,
                            Instant::now(),
                        ).expect("exact real transport ID and authenticated response correlation");
                        match accepted_event {
                            InboundEvent::Block { peer_id: Some(authenticated), block } => {
                                assert_eq!(authenticated, server_id.to_string());
                                assert_eq!(block.hash, genesis_hash);
                            }
                            _ => panic!("only authenticated accepted-DAG block is admissible"),
                        }
                        received += 1;
                        assert!(outbound.is_empty(), "response must consume exact request book");
                        if received == 1 {
                            first_response = Some(response);
                            client.disconnect_peer_id(server_id)
                                .expect("force first authenticated TCP/Noise disconnect");
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } if received == 1 && !redialled => {
                        assert_eq!(peer_id, server_id);
                        outbound.last_connection_closed(&peer_id);
                        let old_id = first_transport_id
                            .as_ref()
                            .expect("first transport identity was recorded");
                        let old_reply = first_response
                            .as_ref()
                            .expect("first authenticated response was recorded");
                        assert!(matches!(
                            outbound.deliver_bound_response_to_daemon(
                                old_id, &peer_id, old_reply, Instant::now()
                            ),
                            Err(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)
                        ), "replayed pre-disconnect transport response must fail closed");
                        let address = server_listen
                            .as_ref()
                            .expect("server first advertised its exact local listener")
                            .clone()
                            .with(Protocol::P2p(server_id));
                        client.dial(address)
                            .expect("reconnect to the same Noise identity, no gossip fallback");
                        redialled = true;
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("client Noise GetBlock transport failed: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("client unexpected inbound failure: {error:?}");
                    }
                    _ => {}
                },
            }
        }
        assert!(redialled, "test must include real disconnect and reconnect");
        assert_eq!(issued, 2);
        assert_eq!(received, 2);
        assert_eq!(served, 2, "server answered once per authenticated request");
        assert!(inbound.is_empty(), "one-shot real response channels consumed");
        assert!(outbound.is_empty(), "no orphaned request tickets after reconnect");
    })
    .await
    .expect("real authenticated Noise rejoin regression timed out");
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_noise_rejoin_delivers_32_fresh_getblock_requests_without_gossip() {
    // Transport-only stress: 32 distinct daemon-local requests after one real
    // reconnect, all for the same accepted genesis. This does NOT establish
    // native-v3 consensus convergence or 32 independently accepted blocks.
    const POST_REJOIN_REQUESTS: usize = 32;
    const TOTAL: usize = POST_REJOIN_REQUESTS + 1;

    tokio::time::timeout(Duration::from_secs(55), async {
        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        let accepted = init_chain_state(CHAIN.to_string());
        let genesis = accepted.dag.genesis_hash.clone();
        let mut server = full_duplex_test_swarm(server_key);
        let mut client = full_duplex_test_swarm(client_key);
        let mut inbound = DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, server_id).unwrap();
        let mut outbound = DirectGetBlockSwarmBindingsV1::new(CHAIN, client_id).unwrap();

        let mut listen: Option<Multiaddr> = None;
        let mut issued = 0usize;
        let mut served = 0usize;
        let mut received = 0usize;
        let mut reconnect_dialled = false;
        let mut reconnected = false;

        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .expect("ephemeral loopback TCP listener only");

        while received < TOTAL {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if listen.is_none() => {
                        listen = Some(address.clone());
                        client.dial(address.with(Protocol::P2p(server_id)))
                            .expect("dial exact authenticated Noise server");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id, "Noise request peer must be authenticated");
                        assert!(served < TOTAL, "no unexpected extra GetBlock requests");
                        let ticket = inbound.accept(&request, &peer, channel, Instant::now())
                            .expect("accept bounded unique daemon request");
                        inbound.respond_from_accepted_dag(
                            ticket.local_token,
                            &peer,
                            &accepted,
                            Instant::now(),
                            |channel, bytes| {
                                server.behaviour_mut().send_response(channel, bytes).is_ok()
                            },
                        ).expect("serve only accepted-DAG block over one-shot Noise channel");
                        served += 1;
                        assert!(inbound.is_empty(), "no retained inbound channel after response");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("server inbound Noise failure: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("server unexpected outbound Noise failure: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        assert_eq!(peer_id, server_id);
                        if issued == 0 || (reconnect_dialled && !reconnected) {
                            if reconnect_dialled {
                                reconnected = true;
                                assert_eq!(received, 1, "rejoin must follow initial response");
                            }
                            issued += 1;
                            let original = PeerAddressedBlockRequest {
                                request_id: format!("getblock:rejoin-stress:{issued}"),
                                requested_peer_id: server_id.to_string(),
                                block_hash: genesis.clone(),
                                sent_at: 0,
                            };
                            outbound.issue_daemon_request_with(
                                &original,
                                client.is_connected(&server_id),
                                Instant::now(),
                                |peer, bytes| client.behaviour_mut().send_request(peer, bytes),
                            ).expect("send bounded daemon-local request on authenticated Noise");
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } if received == 1 && !reconnect_dialled => {
                        assert_eq!(peer_id, server_id);
                        assert!(outbound.is_empty(), "no first-connection request may survive");
                        outbound.last_connection_closed(&peer_id);
                        let address = listen.as_ref().expect("server listener advertised")
                            .clone().with(Protocol::P2p(server_id));
                        client.dial(address).expect("redial same Noise peer without Gossipsub");
                        reconnect_dialled = true;
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id);
                        assert!(received < issued, "no unsolicited or duplicate responses");
                        let delivered = outbound.deliver_bound_response_to_daemon(
                            &request_id,
                            &peer,
                            &response,
                            Instant::now(),
                        ).expect("correlate exact real Noise transport ID and daemon request");
                        match delivered {
                            InboundEvent::Block { peer_id: Some(authenticated), block } => {
                                assert_eq!(authenticated, server_id.to_string());
                                assert_eq!(block.hash, genesis);
                            }
                            _ => panic!("reject missing, forged or unbound GetBlock response"),
                        }
                        received += 1;
                        assert!(outbound.is_empty(), "each completed request consumes its ticket");
                        if received == 1 {
                            client.disconnect_peer_id(server_id)
                                .expect("disconnect real initial Noise connection");
                        } else if received < TOTAL {
                            assert!(reconnected, "stress requests run only after Noise rejoin");
                            issued += 1;
                            let original = PeerAddressedBlockRequest {
                                request_id: format!("getblock:rejoin-stress:{issued}"),
                                requested_peer_id: server_id.to_string(),
                                block_hash: genesis.clone(),
                                sent_at: 0,
                            };
                            outbound.issue_daemon_request_with(
                                &original,
                                client.is_connected(&server_id),
                                Instant::now(),
                                |target, bytes| client.behaviour_mut().send_request(target, bytes),
                            ).expect("send next fresh daemon request through same real Noise link");
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("client authenticated Noise request failed: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("client unexpected Noise inbound failure: {error:?}");
                    }
                    _ => {}
                },
            }
        }

        assert!(reconnected, "the second real Noise connection must have been established");
        assert_eq!(issued, TOTAL);
        assert_eq!(received, TOTAL);
        assert_eq!(served, TOTAL, "all GetBlock requests physically reached the peer");
        assert!(outbound.is_empty(), "no orphaned request correlation after 32 replies");
        assert!(inbound.is_empty(), "no server response channels retained");
    })
    .await
    .expect("32 real post-reconnect GetBlock Noise exchanges timed out");
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_noise_rejoin_delivers_32_distinct_accepted_pow_blocks_in_order() {
    // Two real isolated Noise Swarms and an ACTUAL Core-accepted PoW chain;
    // no native-v3 monetary testnet, no 5-node runtime or production Full.
    use pulsedag_core::accept::{accept_block, AcceptSource};
    use pulsedag_core::{
        build_candidate_block, build_coinbase_transaction, dev_mine_header, expected_difficulty,
        refresh_block_consensus_ids, refresh_block_consensus_ids_with_state,
    };

    const DISTINCT_BLOCKS: usize = 32;

    let mut accepted = init_chain_state(CHAIN.to_string());
    let mut hashes = Vec::with_capacity(DISTINCT_BLOCKS);
    for height in 1..=DISTINCT_BLOCKS {
        let parent = hashes
            .last()
            .cloned()
            .unwrap_or_else(|| accepted.dag.genesis_hash.clone());
        let parent_time = accepted
            .dag
            .blocks
            .get(&parent)
            .expect("parent was accepted before child")
            .header
            .timestamp;
        let mut block = build_candidate_block(
            vec![parent],
            height as u64,
            expected_difficulty(&accepted),
            vec![build_coinbase_transaction(
                "noise-rejoin-miner",
                50,
                height as u64,
            )],
        );
        block.header.timestamp = parent_time.saturating_add(60).max(1);
        refresh_block_consensus_ids_with_state(&mut block, &accepted)
            .expect("compute exact post-state root before PoW");
        let (mined_header, mined, _, _) = dev_mine_header(block.header.clone(), 200_000);
        assert!(mined, "fixture must satisfy actual consensus PoW");
        block.header = mined_header;
        refresh_block_consensus_ids(&mut block);
        accept_block(block.clone(), &mut accepted, AcceptSource::LocalMining)
            .expect("source DAG must accept all 32 genuinely mined blocks");
        hashes.push(block.hash);
    }
    assert_eq!(accepted.dag.best_height, DISTINCT_BLOCKS as u64);
    let distinct = hashes.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(
        distinct.len(),
        DISTINCT_BLOCKS,
        "not repeated genesis requests"
    );
    assert!(
        !accepted.contracts.config.enabled,
        "contract guard stays inactive"
    );

    tokio::time::timeout(Duration::from_secs(100), async {
        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        let mut server = full_duplex_test_swarm(server_key);
        let mut client = full_duplex_test_swarm(client_key);
        let mut inbound = DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, server_id).unwrap();
        let mut outbound = DirectGetBlockSwarmBindingsV1::new(CHAIN, client_id).unwrap();
        let mut restored = init_chain_state(CHAIN.to_string());
        let genesis = restored.dag.genesis_hash.clone();

        let mut listen: Option<Multiaddr> = None;
        let mut issued = 0usize;
        let mut served = 0usize;
        let mut received = 0usize;
        let mut redialled = false;
        let mut reconnected = false;
        let expected_total = DISTINCT_BLOCKS + 1;

        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .expect("test-only ephemeral localhost listener");
        while received < expected_total {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if listen.is_none() => {
                        listen = Some(address.clone());
                        client.dial(address.with(Protocol::P2p(server_id)))
                            .expect("dial authenticated Noise test server");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id, "Noise remote identity must match");
                        assert!(served < expected_total, "no extra request");
                        let ticket = inbound.accept(&request, &peer, channel, Instant::now())
                            .expect("accept authenticated, quota-bound GetBlock");
                        inbound.respond_from_accepted_dag(
                            ticket.local_token,
                            &peer,
                            &accepted,
                            Instant::now(),
                            |channel, bytes| {
                                server.behaviour_mut().send_response(channel, bytes).is_ok()
                            },
                        ).expect("return exact accepted block over real one-shot channel");
                        served += 1;
                        assert!(inbound.is_empty(), "server cannot retain spent response channels");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("authenticated server request failed: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("server unexpectedly became a GetBlock client: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        assert_eq!(peer_id, server_id);
                        if issued == 0 || (redialled && !reconnected) {
                            if redialled {
                                reconnected = true;
                                assert_eq!(received, 1, "no child before authenticated reconnect");
                            }
                            let hash = if issued == 0 {
                                genesis.clone()
                            } else {
                                hashes[0].clone()
                            };
                            issued += 1;
                            let original = PeerAddressedBlockRequest {
                                request_id: format!("getblock:distinct-rejoin:{issued}"),
                                requested_peer_id: server_id.to_string(),
                                block_hash: hash,
                                sent_at: 0,
                            };
                            outbound.issue_daemon_request_with(
                                &original,
                                client.is_connected(&server_id),
                                Instant::now(),
                                |peer, bytes| client.behaviour_mut().send_request(peer, bytes),
                            ).expect("bind distinct fresh daemon ID to real Noise transport");
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. }
                        if received == 1 && !redialled => {
                        assert_eq!(peer_id, server_id);
                        outbound.last_connection_closed(&peer_id);
                        assert!(outbound.is_empty(), "pre-rejoin requests cannot leak");
                        client.dial(
                            listen.as_ref().expect("server local listener").clone()
                                .with(Protocol::P2p(server_id)),
                        ).expect("reconnect same authenticated Noise identity");
                        redialled = true;
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id);
                        assert!(received < issued, "no unsolicited or replayed response");
                        let delivered = outbound.deliver_bound_response_to_daemon(
                            &request_id,
                            &peer,
                            &response,
                            Instant::now(),
                        ).expect("response matches authenticated Noise peer and transport ID");
                        let block = match delivered {
                            InboundEvent::Block { peer_id: Some(authenticated), block } => {
                                assert_eq!(authenticated, server_id.to_string());
                                block
                            }
                            _ => panic!("no missing/forged/other-chain response accepted"),
                        };
                        if received == 0 {
                            assert_eq!(block.hash, genesis, "first handshake fetches genesis");
                        } else {
                            assert!(reconnected, "distinct blocks arrive only after reconnect");
                            assert_eq!(block.hash, hashes[received - 1], "ordered unique block hash");
                            assert_eq!(block.header.height, received as u64);
                            accept_block(block, &mut restored, AcceptSource::P2p)
                                .expect("receiving Core must really accept next PoW block");
                        }
                        received += 1;
                        assert!(outbound.is_empty(), "each response consumes its local ticket");
                        if received == 1 {
                            client.disconnect_peer_id(server_id)
                                .expect("force real TCP/Noise disconnect before 32-block recovery");
                        } else if received < expected_total {
                            let next_hash = hashes[received - 1].clone();
                            issued += 1;
                            let original = PeerAddressedBlockRequest {
                                request_id: format!("getblock:distinct-rejoin:{issued}"),
                                requested_peer_id: server_id.to_string(),
                                block_hash: next_hash,
                                sent_at: 0,
                            };
                            outbound.issue_daemon_request_with(
                                &original,
                                client.is_connected(&server_id),
                                Instant::now(),
                                |target, bytes| client.behaviour_mut().send_request(target, bytes),
                            ).expect("fetch next unique accepted PoW block via authenticated Noise");
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("client Noise transport failed: {error:?}");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("client unexpected request: {error:?}");
                    }
                    _ => {}
                },
            }
        }
        assert!(reconnected, "physical disconnect/reconnect must be exercised");
        assert_eq!(served, expected_total, "all 33 requests physically reached server");
        assert_eq!(issued, expected_total);
        assert_eq!(received, expected_total);
        assert_eq!(restored.dag.best_height, 32, "client must accept 32 distinct blocks");
        assert_eq!(restored.dag.selected_chain, accepted.dag.selected_chain);
        assert_eq!(restored.utxo.compute_state_root().unwrap(), accepted.utxo.compute_state_root().unwrap());
        assert!(outbound.is_empty(), "no unacknowledged real transport response");
        assert!(inbound.is_empty(), "no unspent authenticated inbound channel");
        assert!(!restored.contracts.config.enabled, "contract guard still off");
    })
    .await
    .expect("actual 32 accepted-block Noise rejoin test timed out");
}
