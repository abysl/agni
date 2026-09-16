use agni_sim::abi::{decode, encode};
use n0_future::time::{timeout, Instant};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use spirit_node::iroh::endpoint::Connection;
use spirit_node::iroh::protocol::{AcceptError, ProtocolHandler};
use spirit_node::iroh::{Endpoint, EndpointAddr};
use std::sync::Arc;
use std::time::Duration;

pub const ALPN: &[u8] = b"agni-matchmaking/1";
pub const ADVERT_PREFIX: &str = "agni-match/1:";
pub const LEASE: Duration = Duration::from_secs(30);
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub key: [u8; 32],
    pub nonce: [u8; 16],
}

impl Ticket {
    pub fn advert(self) -> String {
        format!("{ADVERT_PREFIX}{}:{}", hex(&self.nonce), hex(&self.key))
    }

    pub fn parse(advert: &str) -> Option<Self> {
        let (nonce, key) = advert.strip_prefix(ADVERT_PREFIX)?.split_once(':')?;
        Some(Self {
            nonce: unhex(nonce)?,
            key: unhex(key)?,
        })
    }
}

pub fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut result = [0; N];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(result)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Preparing,
    Searching,
    Offering(String),
    Reserved(String),
    Joining(String),
    Matched(String),
}

#[derive(Debug, Default)]
struct State {
    ticket: Option<Ticket>,
    phase: Phase,
    expires: Option<Instant>,
}

impl State {
    fn expire(&mut self, now: Instant) {
        if self.expires.is_some_and(|expiry| now >= expiry) {
            self.phase = Phase::Searching;
            self.expires = None;
        }
    }

    fn claim(&mut self, peer: &str, ticket: Ticket, now: Instant) -> bool {
        self.expire(now);
        if self.ticket != Some(ticket) || self.phase != Phase::Searching {
            return false;
        }
        self.phase = Phase::Reserved(peer.into());
        self.expires = Some(now + LEASE);
        true
    }

    fn admit(&mut self, peer: &str, now: Instant) -> bool {
        self.expire(now);
        match &self.phase {
            Phase::Idle => true,
            Phase::Reserved(expected) | Phase::Matched(expected) if expected == peer => {
                self.phase = Phase::Matched(peer.into());
                self.expires = None;
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Matchmaker {
    state: Arc<Mutex<State>>,
}

impl Matchmaker {
    pub fn prepare(&self) {
        *self.state.lock() = State {
            phase: Phase::Preparing,
            ..State::default()
        };
    }

    pub fn begin(&self, key: [u8; 32]) -> Result<Ticket, String> {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|error| error.to_string())?;
        let ticket = Ticket { key, nonce };
        *self.state.lock() = State {
            ticket: Some(ticket),
            phase: Phase::Searching,
            expires: None,
        };
        Ok(ticket)
    }

    pub fn cancel(&self) {
        *self.state.lock() = State::default();
    }

    pub fn phase(&self) -> Phase {
        let mut state = self.state.lock();
        state.expire(Instant::now());
        state.phase.clone()
    }

    pub fn ticket(&self) -> Option<Ticket> {
        self.state.lock().ticket
    }

    pub fn admit(&self, peer: &str) -> bool {
        self.state.lock().admit(peer, Instant::now())
    }

    pub async fn offer(
        &self,
        endpoint: &Endpoint,
        addr: EndpointAddr,
        ticket: Ticket,
        mine: Ticket,
    ) {
        let peer = addr.id.to_string();
        if endpoint.id().to_string() <= peer {
            return;
        }
        {
            let mut state = self.state.lock();
            state.expire(Instant::now());
            if state.ticket != Some(mine)
                || state.phase != Phase::Searching
                || mine.key != ticket.key
            {
                return;
            }
            state.phase = Phase::Offering(peer.clone());
        }
        let accepted = timeout(EXCHANGE_TIMEOUT, exchange(endpoint, addr, ticket))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false);
        let mut state = self.state.lock();
        if state.ticket == Some(mine) && state.phase == Phase::Offering(peer.clone()) {
            state.phase = if accepted {
                Phase::Joining(peer)
            } else {
                Phase::Searching
            };
        }
    }
}

async fn exchange(endpoint: &Endpoint, addr: EndpointAddr, ticket: Ticket) -> Result<bool, String> {
    let connection = endpoint
        .connect(addr, ALPN)
        .await
        .map_err(|error| error.to_string())?;
    let result = async {
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| error.to_string())?;
        send.write_all(&encode(&ticket))
            .await
            .map_err(|error| error.to_string())?;
        send.finish().map_err(|error| error.to_string())?;
        let bytes = recv
            .read_to_end(MAX_BYTES)
            .await
            .map_err(|error| error.to_string())?;
        decode::<bool>(&bytes).ok_or_else(|| "invalid matchmaking response".into())
    }
    .await;
    connection.close(0u32.into(), b"exchange complete");
    result
}

impl ProtocolHandler for Matchmaker {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let peer = connection.remote_id().to_string();
        let exchange = async {
            let (mut send, mut recv) = connection
                .accept_bi()
                .await
                .map_err(|error| error.to_string())?;
            let bytes = recv
                .read_to_end(MAX_BYTES)
                .await
                .map_err(|error| error.to_string())?;
            let accepted = decode::<Ticket>(&bytes)
                .is_some_and(|ticket| self.state.lock().claim(&peer, ticket, Instant::now()));
            send.write_all(&encode(&accepted))
                .await
                .map_err(|error| error.to_string())?;
            send.finish().map_err(|error| error.to_string())?;
            let _ = send.stopped().await;
            Ok::<_, String>(())
        };
        let _ = timeout(EXCHANGE_TIMEOUT, exchange).await;
        connection.close(0u32.into(), b"exchange complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting() -> State {
        State {
            ticket: Some(Ticket {
                key: [1; 32],
                nonce: [2; 16],
            }),
            phase: Phase::Searching,
            expires: None,
        }
    }

    #[test]
    fn adverts_round_trip_and_reject_malformed_input() {
        let ticket = waiting().ticket.unwrap();
        assert_eq!(Ticket::parse(&ticket.advert()), Some(ticket));
        for invalid in [
            "friends",
            "agni-match/1:",
            "agni-match/1:é:abc",
            "agni-match/2:ab:cd",
        ] {
            assert_eq!(Ticket::parse(invalid), None);
        }
    }

    #[test]
    fn only_one_peer_can_reserve_and_join_and_reconnect() {
        let mut state = waiting();
        let ticket = state.ticket.unwrap();
        let now = Instant::now();
        assert!(!state.admit("a", now));
        assert!(state.claim("a", ticket, now));
        assert!(!state.claim("b", ticket, now));
        assert!(!state.admit("b", now));
        assert!(state.admit("a", now));
        assert!(state.admit("a", now + LEASE));
        assert!(!state.admit("b", now + LEASE));
    }

    #[test]
    fn expired_reservations_cannot_join_and_can_be_replaced() {
        let mut state = waiting();
        let ticket = state.ticket.unwrap();
        let now = Instant::now();
        assert!(state.claim("a", ticket, now));
        assert!(!state.admit("a", now + LEASE));
        assert!(state.claim("b", ticket, now + LEASE));
    }

    #[test]
    fn stale_settings_and_searches_are_rejected() {
        let mut state = waiting();
        let mut ticket = state.ticket.unwrap();
        ticket.key[0] ^= 1;
        assert!(!state.claim("a", ticket, Instant::now()));
        ticket = state.ticket.unwrap();
        ticket.nonce[0] ^= 1;
        assert!(!state.claim("a", ticket, Instant::now()));
    }

    #[test]
    fn an_outgoing_offer_excludes_incoming_claims() {
        let mut state = waiting();
        state.phase = Phase::Offering("a".into());
        assert!(!state.claim("b", state.ticket.unwrap(), Instant::now()));
        assert!(!state.admit("b", Instant::now()));
    }

    #[test]
    fn cancellation_and_restart_change_the_search_identity() {
        let matcher = Matchmaker::default();
        matcher.prepare();
        assert!(!matcher.admit("a"));
        let first = matcher.begin([1; 32]).unwrap();
        matcher.cancel();
        let second = matcher.begin([1; 32]).unwrap();
        assert_ne!(first.nonce, second.nonce);
        assert!(!matcher.state.lock().claim("a", first, Instant::now()));
    }
}
