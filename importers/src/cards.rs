use serde::{Deserialize, Serialize};
use spirit_core::canonical::CanonError;
use spirit_core::collection::{self, Item};
use spirit_core::record::{Attestation, Cir, Claim, Tdr};
use spirit_core::{
    refs, BlobHash, BlobRef, BlobStore, CiHash, Identity, TdHash, Trust, TrustLevel,
};
use std::collections::BTreeMap;

pub const CARD_KIND: &str = "card";
pub const PRINTING_KIND: &str = "card-printing";
pub const CARD_PREFIX: &str = "cards";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external: BTreeMap<String, String>,
    pub game: String,
    pub name: String,
}

impl Card {
    pub fn new(game: &str, name: &str) -> Self {
        Self {
            external: BTreeMap::new(),
            game: game.into(),
            name: name.into(),
        }
    }

    pub fn external(mut self, key: &str, value: &str) -> Self {
        self.external.insert(key.into(), value.into());
        self
    }

    pub fn cir(&self) -> Result<Cir, CanonError> {
        Cir::new(CARD_KIND, self)
    }

    pub fn ci(&self) -> Result<CiHash, CanonError> {
        self.cir()?.address()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printing {
    pub card: CiHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_number: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external: BTreeMap<String, String>,
    pub game: String,
    pub set: String,
}

impl Printing {
    pub fn new(game: &str, card: CiHash, set: &str) -> Self {
        Self {
            card,
            collector_number: None,
            external: BTreeMap::new(),
            game: game.into(),
            set: set.into(),
        }
    }

    pub fn numbered(mut self, collector_number: &str) -> Self {
        self.collector_number = Some(collector_number.into());
        self
    }

    pub fn external(mut self, key: &str, value: &str) -> Self {
        self.external.insert(key.into(), value.into());
        self
    }

    pub fn cir(&self) -> Result<Cir, CanonError> {
        Cir::new(PRINTING_KIND, self)
    }

    pub fn ci(&self) -> Result<CiHash, CanonError> {
        self.cir()?.address()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<Rules>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mana_cost: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub oracle_text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_line: String,
}

impl Rules {
    pub fn encode(&self) -> Result<Vec<u8>, CanonError> {
        spirit_core::canonical::to_vec(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CanonError> {
        spirit_core::canonical::from_slice(bytes)
    }
}

pub fn catalog_name(game: &str) -> String {
    format!("{CARD_PREFIX}/{game}")
}

pub fn game_name(catalog: &str) -> Option<&str> {
    catalog
        .strip_prefix(CARD_PREFIX)?
        .strip_prefix('/')
        .filter(|game| refs::valid_segment(game))
}

pub struct Minted {
    pub ci: CiHash,
    pub cir: BlobHash,
    pub attestations: Vec<BlobHash>,
}

pub struct Catalog(collection::Builder);

impl Catalog {
    pub fn open(store: &BlobStore, identity: &Identity, game: &str) -> Self {
        let mut builder = collection::Builder::open(store, &catalog_name(game), identity);
        builder.kind("catalog");
        Self(builder)
    }

    pub fn attest(&mut self, attestations: &[BlobHash]) {
        for hash in attestations {
            self.0.attest(*hash);
        }
    }

    pub fn add(&mut self, ci: CiHash, label: Option<String>, records: &[BlobHash]) {
        self.0.add(Item {
            ci,
            label,
            default_td: None,
        });
        for hash in records {
            self.0.record(*hash);
        }
    }

    pub fn items(&self) -> &[Item] {
        self.0.items()
    }

    pub fn attestations(&self) -> &[BlobHash] {
        self.0.attestations()
    }

    pub fn publish(&mut self, store: &BlobStore, identity: &Identity) -> Result<BlobHash, String> {
        self.0.publish(store, identity)
    }
}

pub fn mint_card(
    store: &BlobStore,
    identity: &Identity,
    card: &Card,
    rules: &Rules,
    td: &Tdr,
) -> Result<Minted, String> {
    let cir = card.cir().map_err(|e| e.to_string())?;
    let ci = store
        .put(&cir.encode().map_err(|e| e.to_string())?)
        .map(CiHash::from_hash)
        .map_err(|e| e.to_string())?;
    let rules_blob = store
        .put(&rules.encode().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let attestation = attest(store, identity, ci, td, rules_blob)?;
    Ok(Minted {
        ci,
        cir: ci.hash(),
        attestations: vec![attestation],
    })
}

pub fn mint_printing(
    store: &BlobStore,
    identity: &Identity,
    printing: &Printing,
    art: Option<BlobHash>,
    td: &Tdr,
) -> Result<Minted, String> {
    let cir = printing.cir().map_err(|e| e.to_string())?;
    let ci = store
        .put(&cir.encode().map_err(|e| e.to_string())?)
        .map(CiHash::from_hash)
        .map_err(|e| e.to_string())?;
    let attestations = match art {
        Some(art) => vec![attest(store, identity, ci, td, art)?],
        None => Vec::new(),
    };
    Ok(Minted {
        ci,
        cir: ci.hash(),
        attestations,
    })
}

fn attest(
    store: &BlobStore,
    identity: &Identity,
    ci: CiHash,
    td: &Tdr,
    blob: BlobHash,
) -> Result<BlobHash, String> {
    let td_hash = store
        .put(&td.encode().map_err(|e| e.to_string())?)
        .map(TdHash::from_hash)
        .map_err(|e| e.to_string())?;
    let attestation = Attestation::sign(
        Claim::content(ci, td_hash, BlobRef::from_hash(blob)),
        identity,
    )
    .map_err(|e| e.to_string())?;
    store
        .put(&attestation.encode().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn resolve_content(
    store: &BlobStore,
    trust: &Trust,
    records: &[BlobHash],
    ci: CiHash,
) -> Option<BlobHash> {
    records
        .iter()
        .filter_map(|hash| Attestation::decode(&store.get(*hash).ok()?).ok())
        .filter(|attestation| attestation.claim.ci == ci)
        .filter(|attestation| {
            attestation
                .signer()
                .is_some_and(|dgid| trust.trusts(dgid, TrustLevel::Cache))
        })
        .find_map(|attestation| attestation.claim.blob.map(|blob| blob.hash()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> BlobStore {
        let dir = std::env::temp_dir().join(format!("spirit-cards-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        BlobStore::open(dir).unwrap()
    }

    fn td() -> Tdr {
        Tdr::new("scryfall-image-fetch", &("2026-09-04",)).unwrap()
    }

    fn bolt() -> Card {
        Card::new("mtg", "Lightning Bolt").external("scryfall_oracle_id", "4457ed35")
    }

    #[test]
    fn two_nodes_mint_the_same_card_identity() {
        assert_eq!(bolt().ci().unwrap(), bolt().ci().unwrap());
        assert_ne!(
            bolt().ci().unwrap(),
            Card::new("mtg", "Shock").ci().unwrap()
        );
        assert_ne!(
            bolt().ci().unwrap(),
            Card::new("riftbound", "Lightning Bolt").ci().unwrap()
        );
    }

    #[test]
    fn a_printing_names_the_card_it_depicts() {
        let card = bolt().ci().unwrap();
        let printing = Printing::new("mtg", card, "hob").numbered("57");
        assert_eq!(printing.cir().unwrap().kind, PRINTING_KIND);
        let decoded: Printing = printing.cir().unwrap().body().unwrap();
        assert_eq!(decoded.card, card);
        assert_eq!(decoded.collector_number.as_deref(), Some("57"));
    }

    #[test]
    fn a_rescan_does_not_mint_a_new_printing() {
        let card = bolt().ci().unwrap();
        let first = Printing::new("mtg", card, "hob").numbered("57");
        let second = Printing::new("mtg", card, "hob").numbered("57");
        assert_eq!(first.ci().unwrap(), second.ci().unwrap());
    }

    #[test]
    fn art_resolves_through_a_trusted_attestation_only() {
        let store = scratch("art");
        let me = Identity::from_secret([1; 32]);
        let stranger = Identity::from_secret([2; 32]);
        let card = bolt();
        let card_ci = card.ci().unwrap();
        let art = store.put(b"jpeg bytes").unwrap();
        let printing = Printing::new("mtg", card_ci, "hob").numbered("57");
        let minted = mint_printing(&store, &me, &printing, Some(art), &td()).unwrap();

        let trust = Trust::new().with_own(me.dgid());
        assert_eq!(
            resolve_content(&store, &trust, &minted.attestations, minted.ci),
            Some(art)
        );

        let forged = attest(
            &store,
            &stranger,
            minted.ci,
            &td(),
            store.put(b"wrong art").unwrap(),
        )
        .unwrap();
        assert_eq!(resolve_content(&store, &trust, &[forged], minted.ci), None);
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn a_catalog_round_trips_through_the_store() {
        let store = scratch("catalog");
        let me = Identity::from_secret([3; 32]);
        let card = bolt();
        let rules = Rules {
            name: "Lightning Bolt".into(),
            mana_cost: "{R}".into(),
            type_line: "Instant".into(),
            oracle_text: "Deals 3 damage to any target.".into(),
            faces: Vec::new(),
        };
        let minted = mint_card(&store, &me, &card, &rules, &td()).unwrap();

        let mut catalog = Catalog::open(&store, &me, "mtg");
        catalog.add(minted.ci, Some("Lightning Bolt".into()), &[minted.cir]);
        catalog.attest(&minted.attestations);
        catalog.publish(&store, &me).unwrap();

        let reopened = Catalog::open(&store, &me, "mtg");
        assert_eq!(reopened.items().len(), 1);
        assert_eq!(reopened.items()[0].ci, minted.ci);
        assert_eq!(reopened.items()[0].label.as_deref(), Some("Lightning Bolt"));
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn a_republished_catalog_keeps_one_item_per_identity() {
        let store = scratch("dedup");
        let me = Identity::from_secret([4; 32]);
        let ci = bolt().ci().unwrap();
        let mut catalog = Catalog::open(&store, &me, "mtg");
        catalog.add(ci, None, &[]);
        catalog.publish(&store, &me).unwrap();
        let mut again = Catalog::open(&store, &me, "mtg");
        again.add(ci, Some("relabelled".into()), &[]);
        again.publish(&store, &me).unwrap();
        assert_eq!(Catalog::open(&store, &me, "mtg").items().len(), 1);
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn catalog_names_round_trip() {
        assert_eq!(catalog_name("mtg"), "cards/mtg");
        assert_eq!(game_name("cards/mtg"), Some("mtg"));
        assert_eq!(game_name("modules/mtg"), None);
    }
}
