#[cfg(feature = "art")]
pub type IngestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub mod deck;
pub mod naming;

#[cfg(feature = "art")]
pub mod art;

#[cfg(feature = "art")]
pub mod transport;

#[cfg(feature = "art")]
pub mod cards;

#[cfg(feature = "scryfall")]
mod scryfall;
#[cfg(feature = "scryfall")]
pub use scryfall::{ingest, Ingested, Manifest, ManifestCard};

#[cfg(feature = "mtg")]
pub mod mtg;

#[cfg(feature = "riftbound-gateway")]
pub mod asset_gateway;
#[cfg(feature = "riftbound")]
pub mod riftbound;
