#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]

use agni_net::matchmaking::{Matchmaker, Phase, Ticket, ALPN};
use agni_net::session::{
    decode_host, encode_host, ClientSession, HostMsg, HostSession, WIRE_VERSION,
};
use agni_net::table::{self, HostEvent, JoinEvent, TableProtocol};
use spirit_node::gossip::{self, GossipHandler, TableAdvert, ViewSource};
use spirit_node::iroh::endpoint::presets;
use spirit_node::iroh::protocol::Router;
use spirit_node::iroh::Endpoint;
use spirit_node::mesh::Mesh;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

struct Peer {
    endpoint: Endpoint,
    matcher: Matchmaker,
    mesh: Arc<Mesh>,
    table: TableProtocol,
    router: Router,
    dir: PathBuf,
}

impl Peer {
    async fn new() -> Self {
        let endpoint = Endpoint::bind(presets::Minimal).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while endpoint.addr().addrs.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("direct endpoint address");
        let dir = std::env::temp_dir().join(format!("agni-match-test-{}", endpoint.id()));
        std::fs::create_dir(&dir).unwrap();
        let mesh = Mesh::new(&dir, endpoint.clone());
        let matcher = Matchmaker::default();
        let table = TableProtocol::new();
        let router = Router::builder(endpoint.clone())
            .accept(ALPN, matcher.clone())
            .accept(gossip::ALPN, GossipHandler::new(mesh.clone()))
            .accept(table::ALPN, table.clone())
            .spawn();
        Self {
            endpoint,
            matcher,
            mesh,
            table,
            router,
            dir,
        }
    }

    fn search(&self, key: [u8; 32]) -> Ticket {
        let ticket = self.matcher.begin(key).unwrap();
        self.mesh.set_table(Some(TableAdvert {
            name: ticket.advert(),
        }));
        ticket
    }

    async fn gossip_to(&self, other: &Self) {
        let reply = gossip::exchange(
            &self.endpoint,
            other.endpoint.addr(),
            &self.mesh.local_view(),
        )
        .await
        .unwrap();
        self.mesh.merge(&reply);
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn cancelling_a_bridge_join_closes_it_and_ignores_unpolled_attempts() {
    use agni_net::bridge::{self, NetToGame};
    tokio::time::timeout(Duration::from_secs(10), async {
        let host = Peer::new().await;
        let guest = Peer::new().await;
        let mut table = host.table.open();
        let join = || {
            bridge::run_join(
                guest.endpoint.clone(),
                guest.mesh.clone(),
                host.endpoint.addr(),
                host.endpoint.id().to_string(),
                "guest".into(),
            )
        };
        let stale = join();
        bridge::cancel_join();
        let current = join();
        stale.await;
        assert!(bridge::drain_events().is_empty());
        let worker = tokio::spawn(current);
        loop {
            if bridge::drain_events()
                .into_iter()
                .any(|event| matches!(event, NetToGame::Connected))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        bridge::cancel_join();
        worker.await.unwrap();
        loop {
            if let HostEvent::Left(_, _) = table.next().await.unwrap() {
                break;
            }
        }
        assert!(bridge::drain_events().is_empty());
        host.router.shutdown().await.unwrap();
        guest.router.shutdown().await.unwrap();
    })
    .await
    .expect("cancelled transport closes promptly");
}

#[tokio::test]
async fn gossip_discovers_a_match_and_reservation_seats_exactly_two_players() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut peers = vec![Peer::new().await, Peer::new().await, Peer::new().await];
        peers.sort_by_key(|peer| peer.endpoint.id().to_string());
        let host = &peers[0];
        let guest = &peers[1];
        let third = &peers[2];
        let ticket = host.search([1; 32]);
        guest.search([1; 32]);
        third.search([1; 32]);
        host.gossip_to(third).await;
        guest.gossip_to(third).await;
        let discovered = guest
            .mesh
            .open_tables()
            .into_iter()
            .find(|table| table.host == host.endpoint.id().to_string())
            .expect("host relayed through a third gossip peer");
        assert_eq!(Ticket::parse(&discovered.name), Some(ticket));
        let addr = guest.mesh.addr_of(&discovered.host).unwrap();
        guest
            .matcher
            .offer(
                &guest.endpoint,
                addr,
                ticket,
                guest.matcher.ticket().unwrap(),
            )
            .await;
        assert_eq!(
            guest.matcher.phase(),
            Phase::Joining(host.endpoint.id().to_string())
        );
        assert_eq!(
            host.matcher.phase(),
            Phase::Reserved(guest.endpoint.id().to_string())
        );
        third
            .matcher
            .offer(
                &third.endpoint,
                host.endpoint.addr(),
                ticket,
                third.matcher.ticket().unwrap(),
            )
            .await;
        assert_eq!(third.matcher.phase(), Phase::Searching);
        assert!(!host.matcher.admit(&third.endpoint.id().to_string()));
        let mut hosted = host.table.open();
        let mut link = table::join_via(&guest.endpoint, host.endpoint.addr())
            .await
            .unwrap();
        link.send(vec![0]);
        let (conn, remote) = loop {
            if let HostEvent::Joined(conn, remote) = hosted.next().await.unwrap() {
                break (conn, remote);
            }
        };
        assert!(host.matcher.admit(&remote));
        let mut session = HostSession::new("host");
        let (seat, _) = session.join_as(&remote, "guest").unwrap();
        hosted.send(
            conn,
            encode_host(&HostMsg::Welcome {
                version: WIRE_VERSION,
                seat,
                roster: session.roster(),
                log: session.log().to_vec(),
            }),
        );
        let JoinEvent::Frame(bytes) = link.next().await.unwrap() else {
            panic!("welcome frame")
        };
        let Some(HostMsg::Welcome {
            seat, roster, log, ..
        }) = decode_host(&bytes)
        else {
            panic!("welcome")
        };
        let client = ClientSession::from_welcome(seat, roster, log);
        assert_eq!(client.log(), session.log());
        assert_eq!(session.seat_count(), 2);
        assert!(host.matcher.admit(&remote));
        assert!(!host.matcher.admit(&third.endpoint.id().to_string()));
        for peer in &peers {
            peer.router.shutdown().await.unwrap();
        }
    })
    .await
    .expect("gossip and matching finish without public relays");
}

#[tokio::test]
async fn mismatched_and_cancelled_searches_cannot_reserve_a_table() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut peers = vec![Peer::new().await, Peer::new().await];
        peers.sort_by_key(|peer| peer.endpoint.id().to_string());
        let host = &peers[0];
        let guest = &peers[1];
        let old = host.search([1; 32]);
        guest.search([2; 32]);
        guest
            .matcher
            .offer(
                &guest.endpoint,
                host.endpoint.addr(),
                old,
                guest.matcher.ticket().unwrap(),
            )
            .await;
        assert_eq!(host.matcher.phase(), Phase::Searching);
        assert_eq!(guest.matcher.phase(), Phase::Searching);
        guest.search([1; 32]);
        host.matcher.cancel();
        host.search([1; 32]);
        guest
            .matcher
            .offer(
                &guest.endpoint,
                host.endpoint.addr(),
                old,
                guest.matcher.ticket().unwrap(),
            )
            .await;
        assert_eq!(host.matcher.phase(), Phase::Searching);
        assert_eq!(guest.matcher.phase(), Phase::Searching);
        let stale_guest = guest.matcher.ticket().unwrap();
        guest.matcher.cancel();
        guest.search([1; 32]);
        guest
            .matcher
            .offer(
                &guest.endpoint,
                host.endpoint.addr(),
                host.matcher.ticket().unwrap(),
                stale_guest,
            )
            .await;
        assert_eq!(host.matcher.phase(), Phase::Searching);
        assert_eq!(guest.matcher.phase(), Phase::Searching);
        for peer in &peers {
            peer.router.shutdown().await.unwrap();
        }
    })
    .await
    .expect("negative exchanges finish");
}
