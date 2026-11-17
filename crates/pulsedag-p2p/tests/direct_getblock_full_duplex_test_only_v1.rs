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
        let mut a_inbound = DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, a_id).unwrap();
        let mut b_inbound = DirectGetBlockLibp2pResponseChannelsV1::new(CHAIN, b_id).unwrap();
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
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. })
                    | SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("A real full-duplex Noise stream failed: {error:?}");
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
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. })
                    | SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("B real full-duplex Noise stream failed: {error:?}");
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
