//! Test-only 5-node real TCP/Noise signed Gossipsub star rejoin evidence.
//! Not a native-v3 monetary 5N recovery, no deployed private network.
//! Do NOT change the production GetBlock flags or subscribe semantics.

use libp2p::futures::{future::select_all, StreamExt};
use libp2p::gossipsub::{self, MessageAuthenticity, ValidationMode};
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, Swarm, SwarmBuilder};
use std::time::Duration;

type GossipSwarm = Swarm<gossipsub::Behaviour>;
const TOPIC_NAME: &str = "offline-five-node-noise-star-sync";

fn new_real_gossip_swarm(key: identity::Keypair, topic: &gossipsub::IdentTopic) -> GossipSwarm {
    let mut config = gossipsub::ConfigBuilder::default();
    config.validation_mode(ValidationMode::Permissive);
    let gossip = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(key.clone()),
        config.build().expect("valid real Gossipsub config"),
    )
    .expect("signed Gossipsub");
    let mut swarm = SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("real TCP+Noise+Yamux")
        .with_behaviour(|_| gossip)
        .expect("real Gossipsub Swarm")
        .build();
    swarm
        .behaviour_mut()
        .subscribe(topic)
        .expect("subscribe to the exact chain sync topic");
    swarm
}

#[tokio::test(flavor = "current_thread")]
async fn five_real_noise_peers_restore_remote_sync_subscriptions_and_signed_delivery_after_rejoin()
{
    // Exactly one seed and four followers, each its OWN Swarm and real
    // authenticated TCP/Noise connection. This never uses add_explicit_peer.
    tokio::time::timeout(Duration::from_secs(120), async {
        let topic = gossipsub::IdentTopic::new(TOPIC_NAME);
        let keys = (0..5)
            .map(|_| identity::Keypair::generate_ed25519())
            .collect::<Vec<_>>();
        let peer_ids = keys
            .iter()
            .map(|key| PeerId::from(key.public()))
            .collect::<Vec<_>>();
        let mut swarms = keys
            .into_iter()
            .map(|key| new_real_gossip_swarm(key, &topic))
            .collect::<Vec<_>>();

        let mut listener: Option<Multiaddr> = None;
        let mut seed_subscribed = [false; 5];
        let mut leaf_subscribed = [false; 5];
        let mut pre_delivered = [false; 5];
        let mut post_delivered = [false; 5];
        let mut seed_fourth_closed = false;
        let mut fourth_closed = false;
        let mut fresh_seed_subscribed = false;
        let mut fresh_fourth_subscribed = false;
        let mut seed_fourth_reconnected = false;
        let mut fourth_reconnected = false;
        let mut phase = 0_u8; // 0 discovery, 1 first delivery, 2 close, 3 rejoin, 4 final
        let mut sent = 0_u32;
        let mut publish_successes = 0_u32;
        let mut timer = tokio::time::interval(Duration::from_millis(450));

        swarms[0]
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .expect("ephemeral loopback listener only");

        while !post_delivered[1..].iter().all(|yes| *yes) {
            let next_event = select_all(swarms.iter_mut().enumerate().map(|(idx, swarm)| {
                Box::pin(async move { (idx, swarm.select_next_some().await) })
            }));
            tokio::select! {
                ((idx, event), _, _) = next_event => match event {
                    SwarmEvent::NewListenAddr { address, .. } if idx == 0 && listener.is_none() => {
                        listener = Some(address.clone());
                        for leaf in swarms.iter_mut().skip(1) {
                            leaf.dial(address.clone().with(Protocol::P2p(peer_ids[0])))
                                .expect("dial exact signed Noise seed identity");
                        }
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        if idx == 0 {
                            let peer_index = peer_ids.iter().position(|id| *id == peer_id)
                                .expect("seed connected to unknown Noise peer");
                            assert!(peer_index > 0);
                            if phase == 3 && peer_index == 4 {
                                seed_fourth_reconnected = true;
                            }
                        } else {
                            assert_eq!(peer_id, peer_ids[0], "leaf may connect only to seed");
                            if phase == 3 && idx == 4 {
                                fourth_reconnected = true;
                            }
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } if phase == 2 => {
                        if idx == 0 {
                            assert_eq!(peer_id, peer_ids[4]);
                            seed_fourth_closed = true;
                        } else if idx == 4 {
                            assert_eq!(peer_id, peer_ids[0]);
                            fourth_closed = true;
                        } else {
                            panic!("unrelated follower disconnected during fourth-peer isolation");
                        }
                    }
                    SwarmEvent::Behaviour(gossipsub::Event::Subscribed { peer_id, topic }) => {
                        assert_eq!(topic.as_str(), TOPIC_NAME);
                        if idx == 0 {
                            let peer_index = peer_ids.iter().position(|id| *id == peer_id)
                                .expect("subscriber must be a known authenticated leaf");
                            assert!(peer_index > 0);
                            seed_subscribed[peer_index] = true;
                            if phase == 3 && peer_index == 4 && seed_fourth_reconnected {
                                fresh_seed_subscribed = true;
                            }
                        } else {
                            assert_eq!(peer_id, peer_ids[0], "leaf subscriber must be seed");
                            leaf_subscribed[idx] = true;
                            if phase == 3 && idx == 4 && fourth_reconnected {
                                fresh_fourth_subscribed = true;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(gossipsub::Event::Message {
                        propagation_source, message, ..
                    }) => {
                        assert!(idx > 0, "only seed publishes in this test");
                        assert_eq!(propagation_source, peer_ids[0]);
                        assert_eq!(message.topic.as_str(), TOPIC_NAME);
                        if message.data.starts_with(b"five-star-before:") && (1..=4).contains(&phase) {
                            // Correctly signed packets sent in phase 1 may still be
                            // buffered when another follower completes that phase.
                            // Accept late pre-partition traffic, but never count it
                            // as evidence of successful post-rejoin delivery.
                            if phase == 1 {
                                pre_delivered[idx] = true;
                            }
                        } else if phase == 4 && message.data.starts_with(b"five-star-after:") {
                            post_delivered[idx] = true;
                        } else {
                            panic!("out-of-phase, unexpected or unauthenticated sync payload");
                        }
                    }
                    _ => {}
                },
                _ = timer.tick() => {
                    if phase == 1 || phase == 4 {
                        sent += 1;
                        let payload = if phase == 1 {
                            format!("five-star-before:{sent}")
                        } else {
                            format!("five-star-after:{sent}")
                        };
                        if swarms[0]
                            .behaviour_mut()
                            .publish(topic.clone(), payload.into_bytes())
                            .is_ok()
                        {
                            publish_successes += 1;
                        }
                    }
                },
            }
            if phase == 0 && (1..5).all(|i| seed_subscribed[i] && leaf_subscribed[i]) {
                phase = 1;
            }
            if phase == 1 && pre_delivered[1..].iter().all(|yes| *yes) {
                phase = 2;
                swarms[4]
                    .disconnect_peer_id(peer_ids[0])
                    .expect("physical disconnect of fourth leaf");
            }
            if phase == 2 && seed_fourth_closed && fourth_closed {
                phase = 3;
                swarms[4]
                    .dial(
                        listener
                            .as_ref()
                            .expect("seed listener must be known")
                            .clone()
                            .with(Protocol::P2p(peer_ids[0])),
                    )
                    .expect("redial exact authenticated seed, without explicit mesh peer");
            }
            if phase == 3
                && seed_fourth_reconnected
                && fourth_reconnected
                && fresh_seed_subscribed
                && fresh_fourth_subscribed
            {
                assert!((1..5).all(|i| seed_subscribed[i] && leaf_subscribed[i]));
                phase = 4;
            }
        }

        assert_eq!(phase, 4, "test must pass through real partition and rejoin");
        assert!(seed_fourth_closed && fourth_closed);
        assert!(fresh_seed_subscribed && fresh_fourth_subscribed);
        assert!((1..5).all(|i| pre_delivered[i] && post_delivered[i]));
        assert!(
            publish_successes > 0,
            "signed Gossipsub must genuinely publish"
        );
    })
    .await
    .expect("five real authenticated Gossipsub remote-topic rejoin timed out");
}
