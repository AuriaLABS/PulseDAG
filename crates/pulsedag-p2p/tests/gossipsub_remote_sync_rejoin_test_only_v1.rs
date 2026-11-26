//! Evidence-only real TCP/Noise Gossipsub remote subscription rejoin.
//! Not native-v3 five-node recovery. No deployed network or production switch.

use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, MessageAuthenticity, ValidationMode};
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, PeerId, Swarm, SwarmBuilder};
use std::time::Duration;

const SYNC_TOPIC: &str = "offline-noise-rejoin-sync";
type GossipSwarm = Swarm<gossipsub::Behaviour>;

fn authenticated_gossip_swarm(
    key: identity::Keypair,
    topic: &gossipsub::IdentTopic,
) -> GossipSwarm {
    let mut config = gossipsub::ConfigBuilder::default();
    config.validation_mode(ValidationMode::Permissive);
    let gossip = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(key.clone()),
        config.build().expect("valid real Gossipsub config"),
    )
    .expect("signed Gossipsub behaviour");
    let mut swarm = SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .expect("real TCP/Noise/Yamux loopback")
        .with_behaviour(|_| gossip)
        .expect("actual Gossipsub Swarm")
        .build();
    swarm
        .behaviour_mut()
        .subscribe(topic)
        .expect("local sync topic");
    swarm
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_rejoin_observes_fresh_remote_sync_subscriptions_and_delivery() {
    tokio::time::timeout(Duration::from_secs(75), async {
        let a_key = identity::Keypair::generate_ed25519();
        let b_key = identity::Keypair::generate_ed25519();
        let a_id = PeerId::from(a_key.public());
        let b_id = PeerId::from(b_key.public());
        let topic = gossipsub::IdentTopic::new(SYNC_TOPIC);
        let mut a = authenticated_gossip_swarm(a_key, &topic);
        let mut b = authenticated_gossip_swarm(b_key, &topic);

        let mut listener: Option<Multiaddr> = None;
        let mut phase = 0_u8; // 0 first connection, 1 disconnecting, 2 rejoined
        let mut first_a_sub = false;
        let mut first_b_sub = false;
        let mut a_closed = false;
        let mut b_closed = false;
        let mut a_reconnected = false;
        let mut b_reconnected = false;
        let mut after_a_sub = false;
        let mut after_b_sub = false;
        let mut publish_ok = false;
        let mut no_subscriber_failures = 0usize;
        let mut publish_attempts = 0usize;
        let mut delivered = false;
        let mut tick = tokio::time::interval(Duration::from_millis(500));

        a.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .expect("ephemeral localhost only");
        while !delivered {
            tokio::select! {
                event = a.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } if listener.is_none() => {
                        listener = Some(address.clone());
                        b.dial(address.with(Protocol::P2p(a_id)))
                            .expect("dial authenticated A via local Noise");
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        assert_eq!(peer_id, b_id);
                        if phase == 2 { a_reconnected = true; }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } if phase == 1 => {
                        assert_eq!(peer_id, b_id);
                        a_closed = true;
                    }
                    SwarmEvent::Behaviour(gossipsub::Event::Subscribed { peer_id, topic }) => {
                        assert_eq!(peer_id, b_id, "only authenticated remote can subscribe");
                        if topic.as_str() == SYNC_TOPIC {
                            if phase == 0 { first_a_sub = true; }
                            else if phase == 2 && a_reconnected { after_a_sub = true; }
                        }
                    }
                    _ => {}
                },
                event = b.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        assert_eq!(peer_id, a_id);
                        if phase == 2 { b_reconnected = true; }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } if phase == 1 => {
                        assert_eq!(peer_id, a_id);
                        b_closed = true;
                    }
                    SwarmEvent::Behaviour(gossipsub::Event::Subscribed { peer_id, topic }) => {
                        assert_eq!(peer_id, a_id, "only authenticated remote can subscribe");
                        if topic.as_str() == SYNC_TOPIC {
                            if phase == 0 { first_b_sub = true; }
                            else if phase == 2 && b_reconnected { after_b_sub = true; }
                        }
                    }
                    SwarmEvent::Behaviour(gossipsub::Event::Message { propagation_source, message, .. }) => {
                        assert_eq!(propagation_source, a_id, "must arrive via real authenticated peer");
                        assert_eq!(message.topic.as_str(), SYNC_TOPIC);
                        assert!(phase == 2 && a_reconnected && b_reconnected);
                        assert!(after_a_sub && after_b_sub);
                        assert!(message.data.starts_with(b"after-rejoin:"));
                        delivered = true;
                    }
                    _ => {}
                },
                _ = tick.tick() => {
                    if phase == 2 && a_reconnected && b_reconnected && after_a_sub && after_b_sub {
                        publish_attempts += 1;
                        let bytes = format!("after-rejoin:{publish_attempts}").into_bytes();
                        match a.behaviour_mut().publish(topic.clone(), bytes) {
                            Ok(_) => publish_ok = true,
                            Err(error) if format!("{error:?}").contains("NoPeersSubscribed") => {
                                no_subscriber_failures += 1;
                            }
                            Err(error) => panic!("unexpected signed publish error: {error:?}"),
                        }
                    }
                },
            }
            if phase == 0 && first_a_sub && first_b_sub {
                phase = 1;
                b.disconnect_peer_id(a_id).expect("force physical Noise disconnect");
            }
            if phase == 1 && a_closed && b_closed {
                phase = 2;
                b.dial(
                    listener.as_ref().expect("local listener present").clone()
                        .with(Protocol::P2p(a_id)),
                ).expect("real redial, no Gossipsub explicit-peer workaround");
            }
        }
        assert_eq!(phase, 2);
        assert!(a_closed && b_closed && a_reconnected && b_reconnected);
        assert!(first_a_sub && first_b_sub && after_a_sub && after_b_sub);
        assert!(publish_ok && delivered, "remote subscription must deliver signed gossip");
        assert!(no_subscriber_failures <= publish_attempts);
    }).await.expect("real remote sync subscription after rejoin timed out");
}
