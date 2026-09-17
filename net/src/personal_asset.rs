use n0_future::time::timeout;
use parking_lot::Mutex;
use spirit_node::iroh::endpoint::Connection;
use spirit_node::iroh::protocol::{AcceptError, ProtocolHandler};
use spirit_node::iroh::{Endpoint, EndpointId};
use std::sync::Arc;
use std::time::Duration;

pub const ALPN: &[u8] = b"agni-personal-asset/1";
pub const PREFIX: &str = "personal-art/1:";
pub const MAX_BYTES: usize = 4 << 20;
const DEADLINE: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub owner: EndpointId,
    pub capability: [u8; 32],
    pub hash: [u8; 32],
}

fn hex(bytes: &[u8; 32]) -> String {
    blake3::Hash::from_bytes(*bytes).to_hex().to_string()
}

impl Ticket {
    pub fn encode(&self) -> String {
        format!(
            "{PREFIX}{}:{}:{}",
            self.owner,
            hex(&self.capability),
            hex(&self.hash)
        )
    }

    pub fn parse(value: &str) -> Option<Self> {
        let mut fields = value.strip_prefix(PREFIX)?.split(':');
        let result = Self {
            owner: fields.next()?.parse().ok()?,
            capability: *blake3::Hash::from_hex(fields.next()?).ok()?.as_bytes(),
            hash: *blake3::Hash::from_hex(fields.next()?).ok()?.as_bytes(),
        };
        fields.next().is_none().then_some(result)
    }
}

#[derive(Debug)]
struct Published {
    ticket: Ticket,
    bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct PersonalAsset {
    published: Arc<Mutex<Option<Published>>>,
    transfers: Arc<tokio::sync::Semaphore>,
}

impl Default for PersonalAsset {
    fn default() -> Self {
        Self::new()
    }
}

impl PersonalAsset {
    pub fn new() -> Self {
        Self {
            published: Arc::default(),
            transfers: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }

    pub fn publish(&self, owner: EndpointId, bytes: Vec<u8>) -> Result<Ticket, String> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err("personal artwork must contain 1 byte to 4 MiB".into());
        }
        let hash = *blake3::hash(&bytes).as_bytes();
        let mut held = self.published.lock();
        if let Some(current) = held.as_ref().filter(|current| current.ticket.hash == hash) {
            return Ok(current.ticket.clone());
        }
        let mut capability = [0; 32];
        getrandom::fill(&mut capability).map_err(|error| error.to_string())?;
        let ticket = Ticket {
            owner,
            capability,
            hash,
        };
        *held = Some(Published {
            ticket: ticket.clone(),
            bytes: Arc::new(bytes),
        });
        Ok(ticket)
    }

    pub fn clear(&self) {
        *self.published.lock() = None;
    }

    fn lookup(&self, capability: &[u8]) -> Option<Arc<Vec<u8>>> {
        self.published
            .lock()
            .as_ref()
            .filter(|held| held.ticket.capability.as_slice() == capability)
            .map(|held| held.bytes.clone())
    }
}

pub async fn fetch(endpoint: &Endpoint, ticket: &Ticket) -> Result<Vec<u8>, String> {
    fetch_from(endpoint, ticket.owner.into(), ticket).await
}

async fn fetch_from(
    endpoint: &Endpoint,
    address: spirit_node::iroh::EndpointAddr,
    ticket: &Ticket,
) -> Result<Vec<u8>, String> {
    timeout(DEADLINE, async {
        if address.id != ticket.owner {
            return Err("Artwork owner mismatch".into());
        }
        let connection = endpoint
            .connect(address, ALPN)
            .await
            .map_err(|e| e.to_string())?;
        let result = async {
            let (mut send, mut recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
            send.write_all(&ticket.capability)
                .await
                .map_err(|e| e.to_string())?;
            send.finish().map_err(|e| e.to_string())?;
            let bytes = recv
                .read_to_end(MAX_BYTES)
                .await
                .map_err(|e| e.to_string())?;
            if bytes.is_empty() || *blake3::hash(&bytes).as_bytes() != ticket.hash {
                return Err("personal artwork does not match its fingerprint".into());
            }
            Ok(bytes)
        }
        .await;
        connection.close(0u32.into(), b"artwork received");
        result
    })
    .await
    .map_err(|_| "personal artwork transfer timed out".to_string())?
}

impl ProtocolHandler for PersonalAsset {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let Ok(_permit) = self.transfers.try_acquire() else {
            connection.close(0u32.into(), b"busy");
            return Ok(());
        };
        let exchange = async {
            let (mut send, mut recv) = connection.accept_bi().await.map_err(|e| e.to_string())?;
            let capability = recv.read_to_end(32).await.map_err(|e| e.to_string())?;
            let bytes = self.lookup(&capability).ok_or("artwork unavailable")?;
            send.write_all(&bytes).await.map_err(|e| e.to_string())?;
            send.finish().map_err(|e| e.to_string())?;
            let _ = send.stopped().await;
            Ok::<_, String>(())
        };
        let _ = timeout(DEADLINE, exchange).await;
        connection.close(0u32.into(), b"artwork exchange ended");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_revoked_when_replaced_or_cleared() {
        let service = PersonalAsset::new();
        let owner = spirit_node::iroh::SecretKey::generate().public();
        let ticket = service.publish(owner, vec![1, 2, 3]).unwrap();
        assert_eq!(Ticket::parse(&ticket.encode()), Some(ticket.clone()));
        assert!(Ticket::parse(&(ticket.encode() + ":extra")).is_none());
        assert!(Ticket::parse("personal-art/1:bad").is_none());
        assert!(service.lookup(&[0; 32]).is_none());
        assert!(service.lookup(&ticket.capability).is_some());
        assert_eq!(service.publish(owner, vec![1, 2, 3]).unwrap(), ticket);
        service.publish(owner, vec![4]).unwrap();
        assert!(service.lookup(&ticket.capability).is_none());
        service.clear();
        assert!(service.published.lock().is_none());
        assert!(service.publish(owner, Vec::new()).is_err());
        assert!(service.publish(owner, vec![0; MAX_BYTES + 1]).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn direct_transfer_requires_the_current_capability_and_hash() {
        use spirit_node::iroh::{endpoint::presets, protocol::Router};
        let owner = Endpoint::bind(presets::Minimal).await.unwrap();
        let receiver = Endpoint::bind(presets::Minimal).await.unwrap();
        timeout(Duration::from_secs(5), async {
            while owner.addr().addrs.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let service = PersonalAsset::new();
        let router = Router::builder(owner.clone())
            .accept(ALPN, service.clone())
            .spawn();
        let ticket = service
            .publish(owner.id(), b"private picture".to_vec())
            .unwrap();
        assert_eq!(
            fetch_from(&receiver, owner.addr(), &ticket).await.unwrap(),
            b"private picture"
        );
        let mut wrong = ticket.clone();
        wrong.capability = [0; 32];
        assert!(fetch_from(&receiver, owner.addr(), &wrong).await.is_err());
        wrong = ticket.clone();
        wrong.hash = [0; 32];
        assert!(fetch_from(&receiver, owner.addr(), &wrong).await.is_err());
        service.clear();
        assert!(fetch_from(&receiver, owner.addr(), &ticket).await.is_err());
        router.shutdown().await.unwrap();
        receiver.close().await;
    }
}
