//! Short-lived, test-only real Noise/TCP/Yamux libp2p GetBlock exchange.
//!
//! Server advertises inbound request-response support ONLY inside this
//! isolated integration test. The shipped PulseBehaviour remains Outbound-only
//! and no daemon, public network, monetary mining or contracts are activated.
//! The server uses the approved same-chain accepted DAG as the sole block
//! authority and a one-shot libp2p ResponseChannel per authenticated peer.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use libp2p::futures::StreamExt;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use pulsedag_core::genesis::init_chain_state;
use pulsedag_p2p::messages::direct_getblock_codec_v1::{
    DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1,
};
use pulsedag_p2p::messages::direct_getblock_outbound_v1::DirectGetBlockOutboundV1;
use pulsedag_p2p::messages::direct_getblock_pending_v1::{
    DIRECT_GETBLOCK_REQUEST_TTL, MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL,
};
use pulsedag_p2p::messages::direct_getblock_response_channel_v1::DirectGetBlockLibp2pResponseChannelsV1;
use pulsedag_p2p::messages::direct_getblock_swarm_bindings_v1::DirectGetBlockSwarmBindingsV1;
use pulsedag_p2p::{InboundEvent, PeerAddressedBlockRequest};

const CHAIN: &str = "offline-noise-two-node-getblock-v3";

type DirectSwarm = Swarm<request_response::Behaviour<DirectGetBlockRawCodecV1>>;

fn swarm(key: identity::Keypair, supported: ProtocolSupport) -> DirectSwarm {
    SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("test Noise TCP transport")
        .with_behaviour(move |_| {
            request_response::Behaviour::with_codec(
                DirectGetBlockRawCodecV1,
                [(
                    StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1),
                    supported,
                )],
                request_response::Config::default()
                    .with_request_timeout(DIRECT_GETBLOCK_REQUEST_TTL)
                    .with_max_concurrent_streams(MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL),
            )
        })
        .expect("test request-response behaviour")
        .build()
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_tcp_noise_getblock_genesis_and_missing_reply_exact_channels() {
    tokio::time::timeout(Duration::from_secs(35), async {
        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        assert_ne!(server_id, client_id);

        let accepted_chain = init_chain_state(CHAIN.into());
        let genesis_hash = accepted_chain.dag.genesis_hash.clone();
        let missing_hash = "a".repeat(64);
        assert_ne!(genesis_hash, missing_hash);
        let mut server = swarm(server_key, ProtocolSupport::Inbound);
        let mut client = swarm(client_key, ProtocolSupport::Outbound);
        let mut inbound =
            DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, server_id)
                .expect("test authenticated server");
        let mut outbound = DirectGetBlockOutboundV1::new(CHAIN, client_id)
            .expect("test authenticated client");
        let mut actual_request_ids: HashMap<OutboundRequestId, String> = HashMap::new();
        let mut connected = false;
        let mut heard_genesis = false;
        let mut heard_missing = false;
        let mut server_received = 0_usize;

        let listen: Multiaddr = "/ip4/127.0.0.1/tcp/0"
            .parse()
            .expect("test ephemeral local port");
        server.listen_on(listen).expect("listen only on ephemeral loopback");

        while !(heard_genesis && heard_missing) {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        // Authenticate the expected Noise PeerId in the dial
                        // address. No external host/port is contacted.
                        if !connected {
                            client.dial(address.with(Protocol::P2p(server_id)))
                                .expect("client dials exact ephemeral authenticated server");
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id, "request must be from Noise-authenticated client");
                        let ticket = inbound
                            .accept(&request, &peer, channel, Instant::now())
                            .expect("accept exact authenticated request");
                        server_received += 1;
                        inbound
                            .respond_from_accepted_dag(
                                ticket.local_token,
                                &peer,
                                &accepted_chain,
                                Instant::now(),
                                |channel, raw| server.behaviour_mut().send_response(channel, raw).is_ok(),
                            )
                            .expect("respond once with accepted DAG block or authenticated miss");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("inbound libp2p failure must not be ignored: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !connected => {
                        assert_eq!(peer_id, server_id);
                        connected = true;
                        for hash in [&genesis_hash, &missing_hash] {
                            let ticket = outbound
                                .prepare(&peer_id, hash, Instant::now())
                                .expect("strict outbound GetBlock ticket");
                            let real_id = client.behaviour_mut().send_request(&peer_id, ticket.wire);
                            assert!(
                                actual_request_ids
                                    .insert(real_id, ticket.request.request_id)
                                    .is_none(),
                                "libp2p OutboundRequestId must never be reused"
                            );
                        }
                        assert_eq!(actual_request_ids.len(), 2);
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id, "only the authentic Noise server can reply");
                        let local_ticket_id = actual_request_ids
                            .remove(&request_id)
                            .expect("unsolicited, duplicated or remote-asserted transport ID");
                        let decoded = outbound
                            .complete_raw(&local_ticket_id, &peer, &response, Instant::now())
                            .expect("exact authenticated chain/hash/request and canonical block JSON");
                        if decoded.block_hash == genesis_hash {
                            assert!(!heard_genesis, "replayed genesis block response");
                            assert_eq!(
                                decoded.block.as_ref().map(|b| b.hash.as_str()),
                                Some(genesis_hash.as_str())
                            );
                            heard_genesis = true;
                        } else if decoded.block_hash == missing_hash {
                            assert!(!heard_missing, "replayed missing-block response");
                            assert!(decoded.block.is_none(), "missing block must not be fabricated");
                            heard_missing = true;
                        } else {
                            panic!("unexpected block hash returned by authenticated responder");
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("outbound libp2p GetBlock failure: {error:?}");
                    }
                    _ => {}
                },
            }
        }

        assert_eq!(server_received, 2, "server must accept two actual authenticated substreams");
        assert!(inbound.is_empty(), "both one-shot response channels must be consumed");
        assert!(outbound.is_empty(), "both exact transport-bound pending tickets must clear");
        assert!(actual_request_ids.is_empty(), "no transport ID may survive completion");
    })
    .await
    .expect("real authenticated two-node GetBlock must finish within CI budget");
}

#[tokio::test(flavor = "current_thread")]
async fn real_noise_swarms_bind_transport_ids_to_exact_daemon_ingress_requests() {
    tokio::time::timeout(Duration::from_secs(35), async {
        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        let accepted_chain = init_chain_state(CHAIN.into());
        let genesis_hash = accepted_chain.dag.genesis_hash.clone();
        let missing_hash = "e".repeat(64);
        let mut server = swarm(server_key, ProtocolSupport::Inbound);
        let mut client = swarm(client_key, ProtocolSupport::Outbound);
        let mut inbound =
            DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, server_id).unwrap();
        let mut bindings = DirectGetBlockSwarmBindingsV1::new(CHAIN, client_id).unwrap();
        // Only count real libp2p transport IDs for replay detection. The
        // complete daemon request is bound by DirectGetBlockSwarmBindingsV1
        // itself, never reconstructed from a second runtime HashMap.
        let mut observed_transport_ids = std::collections::HashSet::<OutboundRequestId>::new();
        let mut started = false;
        let mut heard_accepted = false;
        let mut heard_missing = false;
        let mut inbound_count = 0_usize;
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();

        while !(heard_accepted && heard_missing) {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        if !started {
                            client.dial(address.with(Protocol::P2p(server_id))).unwrap();
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id);
                        let ticket = inbound
                            .accept(&request, &peer, channel, Instant::now())
                            .expect("exact authenticated inbound ticket");
                        inbound_count += 1;
                        inbound
                            .respond_from_accepted_dag(
                                ticket.local_token,
                                &peer,
                                &accepted_chain,
                                Instant::now(),
                                |channel, raw| server.behaviour_mut().send_response(channel, raw).is_ok(),
                            )
                            .expect("accepted-only one-shot responder");
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("unexpected Noise server inbound failure: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !started => {
                        assert_eq!(peer_id, server_id);
                        started = true;
                        for (index, hash) in [&genesis_hash, &missing_hash].iter().enumerate() {
                            let now = Instant::now();
                            let original = PeerAddressedBlockRequest {
                                request_id: format!("getblock:daemon-local:{index}"),
                                requested_peer_id: peer_id.to_string(),
                                block_hash: (*hash).clone(),
                                sent_at: 0,
                            };
                            let transport_id = bindings
                                .issue_daemon_request_with(
                                    &original,
                                    client.is_connected(&peer_id),
                                    now,
                                    |peer, wire| client.behaviour_mut().send_request(peer, wire),
                                )
                                .expect("atomic daemon ticket must use real Noise transport ID");
                            assert!(
                                observed_transport_ids.insert(transport_id),
                                "transport IDs must be unique"
                            );
                        }
                        assert_eq!(observed_transport_ids.len(), 2);
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id, "Noise transport must authenticate responder");
                        assert!(
                            observed_transport_ids.remove(&request_id),
                            "unexpected or replayed real transport ID"
                        );
                        let event = bindings
                            .deliver_bound_response_to_daemon(
                                &request_id,
                                &peer,
                                &response,
                                Instant::now(),
                            )
                            .expect("strict single-book Noise response and daemon correlation");
                        match event {
                            InboundEvent::Block {
                                peer_id: Some(actual_peer),
                                block,
                            } => {
                                assert!(!heard_accepted);
                                assert_eq!(actual_peer, server_id.to_string());
                                assert_eq!(block.hash, genesis_hash);
                                heard_accepted = true;
                            }
                            InboundEvent::BlockDataMissing {
                                hash: Some(actual_hash),
                                peer_id: Some(actual_peer),
                                request_id: Some(actual_request_id),
                            } => {
                                assert!(!heard_missing);
                                assert_eq!(actual_peer, server_id.to_string());
                                assert_eq!(actual_hash, missing_hash);
                                assert_eq!(actual_request_id, "getblock:daemon-local:1");
                                heard_missing = true;
                            }
                            _ => panic!("validated response must only yield standard daemon ingress"),
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("unexpected Noise client outbound failure: {error:?}");
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(inbound_count, 2);
        assert!(observed_transport_ids.is_empty());
        assert!(inbound.is_empty());
        assert!(bindings.is_empty());
    })
    .await
    .expect("real Noise two-node daemon-correlated roundtrip must complete before timeout");
}
