//! REAL Noise/TCP/Yamux test for the dormant native-v3 pull coordinator.
//! Both Swarms are local CI fixtures, NOT launched daemons or a public network.
//! Full v3 PoW candidate body is never economically accepted by this test.

use std::time::{Duration, Instant};

use libp2p::futures::StreamExt;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use pulsedag_core::{
    block_envelope_v3::build_block_envelope_v3,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    types::compute_merkle_root,
};
use pulsedag_p2p::messages::{
    direct_getblock_codec_v1::{DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1},
    direct_getblock_v1::decode_direct_getblock_request,
    native_v3_getblock_candidate_v1::{
        encode_native_v3_getblock_candidate_response, NativeV3GetBlockCandidateResponse,
        NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
    },
    native_v3_noise_pull_controller_v1::{
        NativeV3NoisePullErrorV1, NativeV3RealNoisePullControllerV1,
    },
    native_v3_sync_recovery_v1::NativeV3SyncError,
};

const CHAIN: &str = "native-first-noise-pull-real-swarm-test-only";
const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
type TestSwarm = Swarm<request_response::Behaviour<DirectGetBlockRawCodecV1>>;

fn swarm(key: identity::Keypair, mode: ProtocolSupport) -> TestSwarm {
    SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("real Noise test transport")
        .with_behaviour(move |_| {
            request_response::Behaviour::with_codec(
                DirectGetBlockRawCodecV1,
                [(
                    StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1),
                    mode,
                )],
                request_response::Config::default()
                    .with_request_timeout(Duration::from_secs(12))
                    .with_max_concurrent_streams(4),
            )
        })
        .expect("test-only request-response")
        .build()
}

#[tokio::test(flavor = "current_thread")]
async fn real_noise_swarm_rejects_forged_reply_and_retries_v3_without_accepting_money() {
    tokio::time::timeout(Duration::from_secs(70), async {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let txs = vec![build_reward_claim_transaction_v3(
            "pulse1native-noise-sync-test",
            1,
            CHAIN,
        ).unwrap()];
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash.clone()],
            timestamp_ns: GENESIS_NS + 1_000_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: "55".repeat(32),
            blue_score: 1,
            height: 1,
        };
        let nonce = (0_u64..4096)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
            })
            .expect("valid canonical KHeavyHash PoW in bounded fixture");
        header.nonce = nonce;
        let candidate = build_block_envelope_v3(CHAIN, header, txs).unwrap();

        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        let mut server = swarm(server_key, ProtocolSupport::Inbound);
        let mut client = swarm(client_key, ProtocolSupport::Outbound);
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse::<Multiaddr>().unwrap())
            .unwrap();

        let mut sync = NativeV3RealNoisePullControllerV1::new(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            client_id,
        ).unwrap();
        let started = Instant::now();
        sync.enqueue_missing(&candidate.hash, started).unwrap();

        let mut dialed = false;
        let mut connected = false;
        let mut replies_sent = 0usize;
        let mut replies_seen = 0usize;
        let mut previous_transport_id = None;

        while replies_seen < 2 {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if !dialed => {
                        dialed = true;
                        client.dial(address.with(Protocol::P2p(server_id)))
                            .expect("dial authenticated Noise server");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request: raw, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id);
                        let request = decode_direct_getblock_request(
                            &raw,
                            CHAIN,
                            &peer.to_string(),
                            &server_id.to_string(),
                        ).expect("bound native GetBlock request");
                        assert_eq!(request.block_hash, candidate.hash);
                        let mut reply = NativeV3GetBlockCandidateResponse {
                            version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
                            chain_id: CHAIN.to_string(),
                            request_id: request.request_id.clone(),
                            responding_peer_id: server_id.to_string(),
                            block_hash: candidate.hash.clone(),
                            block: Some(candidate.clone()),
                        };
                        let wire = if replies_sent == 0 {
                            // Authenticated transport peer is real, but the
                            // response JSON *claims* the wrong responder.
                            // This must NOT be accepted or count as sync.
                            reply.responding_peer_id = client_id.to_string();
                            serde_json::to_vec(&reply).unwrap()
                        } else {
                            encode_native_v3_getblock_candidate_response(
                                &reply,
                                &request,
                                CHAIN,
                                &peer.to_string(),
                                &server_id.to_string(),
                            ).expect("valid requested native-v3 envelope")
                        };
                        assert!(server.behaviour_mut().send_response(channel, wire).is_ok());
                        replies_sent += 1;
                        assert!(replies_sent <= 2);
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("server transport failure: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !connected => {
                        assert_eq!(peer_id, server_id);
                        connected = true;
                        let issued = sync.drive(
                            &[server_id],
                            started,
                            &|_| false,
                            |target, bytes| client.behaviour_mut().send_request(target, bytes),
                        );
                        assert_eq!(issued.transmitted.len(), 1);
                        assert!(issued.rejected.is_empty());
                        previous_transport_id = issued.transmitted.first().copied();
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id, "real Noise authenticated responder");
                        assert_eq!(Some(request_id), previous_transport_id);
                        if replies_seen == 0 {
                            assert!(matches!(
                                sync.response_from_noise(
                                    &request_id,
                                    &peer,
                                    &response,
                                    started + Duration::from_secs(1),
                                    &|_| false
                                ),
                                Err(NativeV3NoisePullErrorV1::Correlation(_))
                            ));
                            assert_eq!(sync.report().queued, 1);
                            assert_eq!(sync.report().accepted, 0);
                            assert_eq!(sync.transport_inflight_count(), 0);

                            // Real second libp2p OutboundRequestId, never
                            // reuse the peer-controlled JSON correlation.
                            let issued = sync.drive(
                                &[server_id],
                                started + Duration::from_secs(3),
                                &|_| false,
                                |target, bytes| client.behaviour_mut().send_request(target, bytes),
                            );
                            assert_eq!(issued.transmitted.len(), 1);
                            assert_ne!(issued.transmitted.first().copied(), previous_transport_id);
                            previous_transport_id = issued.transmitted.first().copied();
                        } else {
                            sync.response_from_noise(
                                &request_id,
                                &peer,
                                &response,
                                started + Duration::from_secs(4),
                                &|_| false
                            ).expect("real authenticated native-v3 candidate");
                            assert_eq!(sync.report().awaiting_consensus, 1);
                            assert_eq!(sync.report().accepted, 0);
                            assert_eq!(sync.ready_for_daemon_consensus(&|_| false).len(), 1);
                            assert_eq!(
                                sync.accepted_by_daemon_after_atomic_commit(
                                    &candidate.hash,
                                    &|_| false
                                ),
                                Err(NativeV3NoisePullErrorV1::Recovery(
                                    NativeV3SyncError::NotInAcceptedStore
                                ))
                            );
                        }
                        replies_seen += 1;
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("client transport failure: {error:?}");
                    }
                    _ => {}
                }
            }
        }
        assert!(connected);
        assert_eq!(replies_sent, 2);
        assert_eq!(replies_seen, 2);
        assert_eq!(sync.report().accepted, 0);
        assert_eq!(sync.transport_inflight_count(), 0);
    })
    .await
    .expect("genuine Noise identity/retry test must finish without timeouts");
}
