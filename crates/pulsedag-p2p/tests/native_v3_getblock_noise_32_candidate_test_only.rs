//! Genuine native-v3 full-envelope GetBlock candidate roundtrip across real
//! TCP + Noise + Yamux authenticated peers. This is an OFFLINE TEST ONLY.
//!
//! The source corpus contains 32 independently mined, chain-bound KHeavyHash
//! `BlockEnvelopeV3` candidates, including a side fork and a two-parent merge.
//! The receiving peer checks the exact authenticated request/response binding
//! and native-v3 body/PoW, never deserializes legacy `types::Block`.
//!
//! Crucially these candidates have placeholder state roots, no independently
//! verified GHOSTDAG K/blue-work, rewards, mature UTXOs or checkpoint proof.
//! They are NOT consensus-admitted, not served from production daemon storage,
//! not a 5-daemon/32-monetary-block acceptance proof and not a live testnet.
//! The production libp2p GetBlock path remains outbound-only and HARD OFF.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use pulsedag_core::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    types::{compute_merkle_root, BlockId},
};
use pulsedag_p2p::messages::{
    direct_getblock_codec_v1::{DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1},
    direct_getblock_v1::{
        decode_direct_getblock_request, encode_direct_getblock_request, DirectGetBlockRequestV1,
        DIRECT_GETBLOCK_PROTOCOL_V1,
    },
    native_v3_getblock_candidate_v1::{
        decode_native_v3_getblock_candidate_response, encode_native_v3_getblock_candidate_response,
        NativeV3GetBlockCandidateResponse, NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
    },
};

const CHAIN: &str = "first-public-v3-noise-32-candidates-test-only";
const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
const BITS: u32 = 0x207f_ffff;
const CANDIDATES: usize = 32;
type DirectSwarm = Swarm<request_response::Behaviour<DirectGetBlockRawCodecV1>>;

fn test_swarm(key: identity::Keypair, mode: ProtocolSupport) -> DirectSwarm {
    SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("genuine TCP/Noise/Yamux test transport")
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
        .expect("test-only protocol, no production Swarm changes")
        .build()
}

fn mine_candidate(parents: Vec<BlockId>, height: u64, salt: u64) -> BlockEnvelopeV3 {
    let transactions =
        vec![
            build_reward_claim_transaction_v3("pulse1native-test-miner", salt, CHAIN)
                .expect("syntactically valid amountless reward claim"),
        ];
    let mut parents = parents;
    parents.sort();
    parents.dedup();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns: GENESIS_NS + height * 1_000_000_000 + salt,
        difficulty: BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        // NOT a validated monetary state root: this is transport proof only.
        state_root: format!("{:064x}", salt + 1),
        blue_score: height,
        height,
    };
    for nonce in 0..4096 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN)
            .expect("real, canonical native-v3 PoW")
            .pow
            .accepted
        {
            return build_block_envelope_v3(CHAIN, header, transactions)
                .expect("genuine hash-committed native-v3 envelope");
        }
    }
    panic!("unexpected inability to mine easy native-v3 test target");
}

fn native_corpus() -> (BTreeMap<BlockId, BlockEnvelopeV3>, Vec<BlockId>) {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS)
        .expect("independently frozen test-only chain genesis");
    let mut parent = genesis.hash.clone();
    let mut side_parent = None;
    let mut map = BTreeMap::new();
    let mut requested = Vec::new();

    // 31 selected-spine blocks plus one competing PoW-valid side branch.
    // The native height-13 header includes TWO authentic parent hashes.
    for height in 1..=31_u64 {
        let mut parents = vec![parent.clone()];
        if height == 13 {
            parents.push(side_parent.clone().expect("side branch present"));
        }
        let preceding_spine_parent = parents[0].clone();
        let block = mine_candidate(parents, height, height);
        parent = block.hash.clone();
        requested.push(block.hash.clone());
        assert!(map.insert(block.hash.clone(), block).is_none());

        if height == 12 {
            let side = mine_candidate(vec![preceding_spine_parent], height, 100 + height);
            // An independently mined sibling at the same height.
            assert_ne!(side.hash, parent);
            side_parent = Some(side.hash.clone());
            requested.push(side.hash.clone());
            assert!(map.insert(side.hash.clone(), side).is_none());
        }
    }
    assert_eq!(map.len(), CANDIDATES);
    assert_eq!(requested.len(), CANDIDATES);
    assert_eq!(
        map.get(requested.last().unwrap()).unwrap().header.height,
        31
    );
    assert_eq!(
        map.get(requested.last().unwrap())
            .unwrap()
            .header
            .parents
            .len(),
        1
    );
    assert!(map.values().any(|b| b.header.parents.len() == 2));
    (map, requested)
}

fn request(
    hash: &str,
    sequence: usize,
    client: &PeerId,
    server: &PeerId,
) -> DirectGetBlockRequestV1 {
    DirectGetBlockRequestV1 {
        version: DIRECT_GETBLOCK_PROTOCOL_V1,
        chain_id: CHAIN.into(),
        request_id: format!("getblock:nativev3:{sequence}:{hash}"),
        requesting_peer_id: client.to_string(),
        requested_peer_id: server.to_string(),
        block_hash: hash.into(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn thirty_two_real_pow_native_v3_envelopes_cross_authenticated_noise_without_admission() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (candidate_cache, requested) = native_corpus();
        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        assert_ne!(server_id, client_id);

        let mut server = test_swarm(server_key, ProtocolSupport::Inbound);
        let mut client = test_swarm(client_key, ProtocolSupport::Outbound);
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse::<Multiaddr>().unwrap())
            .unwrap();
        let mut started = false;
        let mut served = 0_usize;
        let mut received = 0_usize;
        let mut inflight: HashMap<OutboundRequestId, DirectGetBlockRequestV1> = HashMap::new();
        let mut seen = HashSet::new();

        while received < CANDIDATES {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if !started => {
                        client.dial(address.with(Protocol::P2p(server_id)))
                            .expect("dial exact authenticated test-only Noise endpoint");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request: raw, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id, "requester identity is Noise-authenticated");
                        let verified = decode_direct_getblock_request(
                            &raw, CHAIN, &peer.to_string(), &server_id.to_string(),
                        )
                        .expect("exact chain, peer, request and canonical hash");
                        let block = candidate_cache.get(&verified.block_hash).cloned();
                        assert!(block.is_some(), "all 32 requests are in bounded candidate cache");
                        // Deliberately NOT an accepted-DAG/daemon authority!
                        let response = NativeV3GetBlockCandidateResponse {
                            version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
                            chain_id: CHAIN.to_string(),
                            request_id: verified.request_id.clone(),
                            responding_peer_id: server_id.to_string(),
                            block_hash: verified.block_hash.clone(),
                            block,
                        };
                        let raw_response = encode_native_v3_getblock_candidate_response(
                            &response, &verified, CHAIN, &peer.to_string(), &server_id.to_string(),
                        )
                        .expect("validate v3 envelope, real PoW and exact authenticated request");
                        assert!(server.behaviour_mut().send_response(channel, raw_response).is_ok());
                        served += 1;
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("no Noise inbound failure is acceptable: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !started => {
                        assert_eq!(peer_id, server_id);
                        started = true;
                        let first = request(&requested[0], 0, &client_id, &server_id);
                        let raw = encode_direct_getblock_request(
                            &first, CHAIN, &client_id.to_string(), &server_id.to_string()
                        ).unwrap();
                        let transport_id = client.behaviour_mut().send_request(&server_id, raw);
                        assert!(inflight.insert(transport_id, first).is_none());
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id, "response is Noise-authenticated");
                        let pending = inflight.remove(&request_id)
                            .expect("reject unknown, replayed or unsolicited transport ticket");
                        assert_eq!(pending.block_hash, requested[received]);
                        let verified = decode_native_v3_getblock_candidate_response(
                            &response, &pending, CHAIN, &client_id.to_string(), &peer.to_string(),
                        )
                        .expect("exact native-v3 candidate PoW and request/response correlation");
                        let candidate = verified.block.expect("requested candidate must exist");
                        assert_eq!(candidate.hash, pending.block_hash);
                        assert_eq!(candidate.header, candidate_cache[&pending.block_hash].header);
                        assert!(seen.insert(candidate.hash), "no duplicate native-v3 candidate");
                        received += 1;
                        if received < CANDIDATES {
                            let next = request(
                                &requested[received], received, &client_id, &server_id,
                            );
                            let raw = encode_direct_getblock_request(
                                &next, CHAIN, &client_id.to_string(), &server_id.to_string()
                            ).unwrap();
                            let transport_id = client.behaviour_mut().send_request(&server_id, raw);
                            assert!(inflight.insert(transport_id, next).is_none());
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("native-v3 candidate transport must not fail: {error:?}");
                    }
                    _ => {}
                }
            }
        }

        assert_eq!(served, CANDIDATES);
        assert_eq!(received, CANDIDATES);
        assert_eq!(seen.len(), CANDIDATES);
        assert!(inflight.is_empty(), "no local request ticket may survive");
        // No monetary replay, state-root acceptance or mining activation occurred.
    })
    .await
    .expect("32 real native-v3 envelopes must traverse Noise within fixed timeout");
}


/// A forged response crossing an otherwise authentic TCP/Noise stream must
/// never become a native-v3 candidate solely because the transport is valid.
/// All four messages use the SAME hash but distinct locally minted tickets.
#[tokio::test(flavor = "current_thread")]
async fn authenticated_noise_rejects_spoofed_peer_hash_and_body_before_valid_native_v3() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let candidate = mine_candidate(vec![genesis.hash], 1, 1);
        let hash = candidate.hash.clone();
        let server_key = identity::Keypair::generate_ed25519();
        let client_key = identity::Keypair::generate_ed25519();
        let server_id = PeerId::from(server_key.public());
        let client_id = PeerId::from(client_key.public());
        let mut server = test_swarm(server_key, ProtocolSupport::Inbound);
        let mut client = test_swarm(client_key, ProtocolSupport::Outbound);
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse::<Multiaddr>().unwrap())
            .unwrap();

        let mut dialed = false;
        let mut connected = false;
        let mut served = 0_usize;
        let mut received = 0_usize;
        let mut inflight: HashMap<OutboundRequestId, DirectGetBlockRequestV1> = HashMap::new();

        while received < 4 {
            tokio::select! {
                event = server.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if !dialed => {
                        dialed = true;
                        client.dial(address.with(Protocol::P2p(server_id)))
                            .expect("dial exact Noise peer once");
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request: raw, channel, .. },
                        ..
                    }) => {
                        assert_eq!(peer, client_id);
                        let verified = decode_direct_getblock_request(
                            &raw, CHAIN, &peer.to_string(), &server_id.to_string(),
                        ).expect("authenticated exact-hash request");
                        assert_eq!(verified.block_hash, hash);
                        let mut reply = NativeV3GetBlockCandidateResponse {
                            version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
                            chain_id: CHAIN.to_owned(),
                            request_id: verified.request_id.clone(),
                            responding_peer_id: server_id.to_string(),
                            block_hash: hash.clone(),
                            block: Some(candidate.clone()),
                        };
                        let wire = match served {
                            0 => {
                                reply.responding_peer_id = client_id.to_string();
                                serde_json::to_vec(&reply).unwrap()
                            }
                            1 => {
                                reply.block_hash = "f".repeat(64);
                                serde_json::to_vec(&reply).unwrap()
                            }
                            2 => {
                                reply.block.as_mut().unwrap().header.nonce ^= 1;
                                serde_json::to_vec(&reply).unwrap()
                            }
                            3 => encode_native_v3_getblock_candidate_response(
                                &reply, &verified, CHAIN, &peer.to_string(),
                                &server_id.to_string(),
                            ).expect("valid candidate after three forged replies"),
                            _ => panic!("server must not receive extra requests"),
                        };
                        assert!(server.behaviour_mut().send_response(channel, wire).is_ok());
                        served += 1;
                    }
                    SwarmEvent::Behaviour(request_response::Event::InboundFailure { error, .. }) => {
                        panic!("real Noise server unexpectedly failed: {error:?}");
                    }
                    _ => {}
                },
                event = client.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if !connected => {
                        assert_eq!(peer_id, server_id);
                        connected = true;
                        let first = request(&hash, 100, &client_id, &server_id);
                        let wire = encode_direct_getblock_request(
                            &first, CHAIN, &client_id.to_string(), &server_id.to_string(),
                        ).unwrap();
                        let id = client.behaviour_mut().send_request(&server_id, wire);
                        assert!(inflight.insert(id, first).is_none());
                    }
                    SwarmEvent::Behaviour(request_response::Event::Message {
                        peer,
                        message: request_response::Message::Response { request_id, response },
                        ..
                    }) => {
                        assert_eq!(peer, server_id);
                        let original = inflight.remove(&request_id)
                            .expect("no unsolicited or replayed real transport ticket");
                        let decoded = decode_native_v3_getblock_candidate_response(
                            &response, &original, CHAIN,
                            &client_id.to_string(), &peer.to_string(),
                        );
                        if received < 3 {
                            assert!(
                                decoded.is_err(),
                                "forged peer/hash/PoW commitment must fail closed"
                            );
                        } else {
                            let valid = decoded.expect("genuine response must remain usable");
                            assert_eq!(valid.block.unwrap().hash, hash);
                        }
                        received += 1;
                        if received < 4 {
                            let next = request(&hash, 100 + received, &client_id, &server_id);
                            let wire = encode_direct_getblock_request(
                                &next, CHAIN, &client_id.to_string(), &server_id.to_string(),
                            ).unwrap();
                            let id = client.behaviour_mut().send_request(&server_id, wire);
                            assert!(inflight.insert(id, next).is_none());
                        }
                    }
                    SwarmEvent::Behaviour(request_response::Event::OutboundFailure { error, .. }) => {
                        panic!("real Noise client unexpectedly failed: {error:?}");
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(served, 4);
        assert_eq!(received, 4);
        assert!(inflight.is_empty());
    })
    .await
    .expect("four authenticated Noise replies must complete without promoting forgeries");
}
