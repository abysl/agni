use super::catalog::{
    CardKind, CatalogCard, StaticCatalog, SUPERTYPE_CHAMPION, SUPERTYPE_SIGNATURE,
};
use super::resolve::canonical_name;
use super::riftcodex::{ApiCard, Riftcodex, THROTTLE};
use crate::art::{self, art_agent, fetch_image, Journal};
use crate::IngestResult;
use serde::{Deserialize, Serialize};
use spirit_core::{BlobHash, BlobStore};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub use crate::art::{Progress, WantedArt};

pub const REF_NAME: &str = "riftbound";
pub const JOURNAL_FILE: &str = "riftbound-images";
pub const DECK_THROTTLE: Duration = Duration::from_millis(250);

#[derive(Serialize, Deserialize)]
pub struct RiftboundManifest {
    pub set: String,
    pub cards: Vec<RiftboundCard>,
}

#[derive(Serialize, Deserialize)]
pub struct RiftboundCard {
    pub name: String,
    pub image: String,
    pub riftbound_id: String,
    pub card_type: String,
    pub supertype: String,
    pub rarity: String,
    pub domain: Vec<String>,
    pub energy: Option<i64>,
    pub might: Option<i64>,
    pub power: Option<i64>,
    pub text: String,
    pub set_id: String,
    pub set_label: String,
    pub collector_number: Option<i64>,
    pub clean_name: String,
    pub image_url: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn record(card: &ApiCard, image: String) -> RiftboundCard {
    RiftboundCard {
        name: canonical_name(&card.riftbound_id, &card.name),
        image,
        riftbound_id: card.riftbound_id.clone(),
        card_type: card.classification.card_type.clone().unwrap_or_default(),
        supertype: card.classification.supertype.clone().unwrap_or_default(),
        rarity: card.classification.rarity.clone().unwrap_or_default(),
        domain: card.classification.domain.clone(),
        energy: card.attributes.energy,
        might: card.attributes.might,
        power: card.attributes.power,
        text: card.text.plain.clone().unwrap_or_default(),
        set_id: card.set.set_id.clone(),
        set_label: card.set.label.clone().unwrap_or_default(),
        collector_number: card.collector_number,
        clean_name: card.metadata.clean_name.clone().unwrap_or_default(),
        image_url: card.media.image_url.clone().unwrap_or_default(),
        tags: card.tags.clone(),
    }
}

pub fn small(value: Option<i64>) -> Option<u8> {
    value.and_then(|value| u8::try_from(value).ok())
}

pub fn catalog_card(card: &RiftboundCard) -> CatalogCard {
    CatalogCard {
        name: canonical_name(&card.riftbound_id, &card.name),
        riftbound_id: card.riftbound_id.clone(),
        kind: CardKind::parse(&card.card_type),
        champion: card.supertype == SUPERTYPE_CHAMPION,
        energy: small(card.energy),
        power: small(card.power),
        might: small(card.might),
        image_url: if card.image_url.is_empty() {
            None
        } else {
            Some(card.image_url.clone())
        },
        domain: card.domain.clone(),
        tags: card.tags.clone(),
        signature: card.supertype == SUPERTYPE_SIGNATURE,
        set_id: if card.set_id.is_empty() {
            None
        } else {
            Some(card.set_id.clone())
        },
        text: if card.text.is_empty() {
            None
        } else {
            Some(card.text.clone())
        },
    }
}

pub struct Ingested {
    pub manifest: BlobHash,
    pub cards: usize,
    pub images_fetched: usize,
    pub images_reused: usize,
    pub skipped: usize,
}

fn publish_manifest(store: &BlobStore, mut entries: Vec<RiftboundCard>) -> IngestResult<BlobHash> {
    entries.sort_by(|left, right| left.riftbound_id.cmp(&right.riftbound_id));
    let manifest = RiftboundManifest {
        set: REF_NAME.to_string(),
        cards: entries,
    };
    let mut encoded = Vec::new();
    ciborium::into_writer(&manifest, &mut encoded)?;
    let manifest_hash = store.put(&encoded)?;
    spirit_core::refs::write(store, REF_NAME, manifest_hash)?;
    Ok(manifest_hash)
}

pub fn wanted_art(card: &CatalogCard) -> WantedArt {
    WantedArt {
        key: card.riftbound_id.clone(),
        name: card.name.clone(),
        image_url: card.image_url.clone(),
    }
}

pub fn record_of(card: &CatalogCard, image: String) -> RiftboundCard {
    let supertype = if card.champion {
        SUPERTYPE_CHAMPION
    } else if card.signature {
        SUPERTYPE_SIGNATURE
    } else {
        ""
    };
    RiftboundCard {
        name: card.name.clone(),
        image,
        riftbound_id: card.riftbound_id.clone(),
        card_type: match card.kind {
            CardKind::Other => String::new(),
            kind => kind.as_str().to_string(),
        },
        supertype: supertype.to_string(),
        rarity: String::new(),
        domain: card.domain.clone(),
        energy: card.energy.map(i64::from),
        might: card.might.map(i64::from),
        power: card.power.map(i64::from),
        text: card.text.clone().unwrap_or_default(),
        set_id: card.set_id.clone().unwrap_or_default(),
        set_label: String::new(),
        collector_number: None,
        clean_name: String::new(),
        image_url: card.image_url.clone().unwrap_or_default(),
        tags: card.tags.clone(),
    }
}

pub fn ingest_deck_art(
    dir: &Path,
    cards: &[CatalogCard],
    throttle: Duration,
    progress: impl FnMut(Progress),
) -> IngestResult<Vec<(String, Vec<u8>)>> {
    let store = BlobStore::open(dir)?;
    let mut by_id: BTreeMap<String, RiftboundCard> = load_manifest(dir)?
        .map(|manifest| manifest.cards)
        .unwrap_or_default()
        .into_iter()
        .map(|card| (card.riftbound_id.to_ascii_lowercase(), card))
        .collect();
    let wanted: Vec<WantedArt> = cards.iter().map(wanted_art).collect();
    let fetched = art::fetch_deck_art(
        &store,
        JOURNAL_FILE,
        throttle,
        &wanted,
        |key| by_id.get(key).and_then(|card| BlobHash::parse(&card.image)),
        progress,
    )?;
    let mut changed = false;
    let mut arts = Vec::new();
    for art in fetched {
        let key = art.key.to_ascii_lowercase();
        let hex = art.hash.to_string();
        match by_id.get_mut(&key) {
            Some(entry) => {
                if entry.image != hex {
                    entry.image = hex;
                    changed = true;
                }
            }
            None => {
                if let Some(card) = cards
                    .iter()
                    .find(|card| card.riftbound_id.eq_ignore_ascii_case(&art.key))
                {
                    by_id.insert(key, record_of(card, hex));
                    changed = true;
                }
            }
        }
        arts.push((art.key, art.bytes));
    }
    if changed {
        publish_manifest(&store, by_id.into_values().collect())?;
    }
    Ok(arts)
}

pub struct IngestedIds {
    pub manifest: Option<BlobHash>,
    pub fetched: Vec<String>,
    pub reused: Vec<String>,
    pub unknown: Vec<String>,
    pub artless: Vec<String>,
}

fn stored_art(
    store: &BlobStore,
    by_id: &BTreeMap<String, RiftboundCard>,
    journal: &Journal,
    key: &str,
) -> Option<BlobHash> {
    by_id
        .get(key)
        .and_then(|card| BlobHash::parse(&card.image))
        .filter(|hash| store.has(*hash))
        .or_else(|| journal.get(store, key))
}

pub fn audit(dir: &Path, ids: &[String]) -> IngestResult<Vec<String>> {
    let store = BlobStore::open(dir)?;
    let by_id: BTreeMap<String, RiftboundCard> = load_manifest(dir)?
        .map(|manifest| manifest.cards)
        .unwrap_or_default()
        .into_iter()
        .map(|card| (card.riftbound_id.to_ascii_lowercase(), card))
        .collect();
    let journal = Journal::open(&store, JOURNAL_FILE);
    Ok(ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .filter(|key| stored_art(&store, &by_id, &journal, key).is_none())
        .collect())
}

pub fn ingest_ids(
    dir: &Path,
    ids: &[String],
    progress: impl FnMut(Progress),
) -> IngestResult<IngestedIds> {
    let mut api = Riftcodex::new();
    let agent = art_agent();
    ingest_ids_with(
        dir,
        &mut api,
        |url| {
            std::thread::sleep(THROTTLE);
            fetch_image(&agent, url)
        },
        ids,
        progress,
    )
}

fn step(progress: &mut impl FnMut(Progress), done: usize, total: usize) {
    progress(Progress {
        done,
        total,
        stage: "images",
    });
}

pub fn ingest_ids_with(
    dir: &Path,
    api: &mut Riftcodex,
    mut fetch: impl FnMut(&str) -> IngestResult<Vec<u8>>,
    ids: &[String],
    mut progress: impl FnMut(Progress),
) -> IngestResult<IngestedIds> {
    let store = BlobStore::open(dir)?;
    let mut by_id: BTreeMap<String, RiftboundCard> = load_manifest(dir)?
        .map(|manifest| manifest.cards)
        .unwrap_or_default()
        .into_iter()
        .map(|card| (card.riftbound_id.to_ascii_lowercase(), card))
        .collect();
    let mut journal = Journal::open(&store, JOURNAL_FILE);
    let mut outcome = IngestedIds {
        manifest: None,
        fetched: Vec::new(),
        reused: Vec::new(),
        unknown: Vec::new(),
        artless: Vec::new(),
    };
    let mut changed = false;
    let total = ids.len();
    for (index, id) in ids.iter().enumerate() {
        let key = id.to_ascii_lowercase();
        let stored = stored_art(&store, &by_id, &journal, &key);
        if let (Some(hash), Some(entry)) = (stored, by_id.get_mut(&key)) {
            let hex = hash.to_string();
            if entry.image != hex {
                entry.image = hex;
                changed = true;
            }
            outcome.reused.push(key);
            step(&mut progress, index + 1, total);
            continue;
        }
        let items = api.by_partial_id(&key).map_err(|error| error.to_string())?;
        let Some(card) = items
            .iter()
            .find(|card| card.riftbound_id.eq_ignore_ascii_case(&key))
        else {
            outcome.unknown.push(key);
            step(&mut progress, index + 1, total);
            continue;
        };
        let hash = match stored {
            Some(hash) => {
                outcome.reused.push(key.clone());
                hash
            }
            None => match card.media.image_url.as_deref() {
                None | Some("") => {
                    outcome.artless.push(key);
                    step(&mut progress, index + 1, total);
                    continue;
                }
                Some(url) => {
                    let bytes = fetch(url)?;
                    let hash = journal.put(&store, &key, &bytes)?;
                    outcome.fetched.push(key.clone());
                    hash
                }
            },
        };
        by_id.insert(key, record(card, hash.to_string()));
        changed = true;
        step(&mut progress, index + 1, total);
    }
    if changed {
        outcome.manifest = Some(publish_manifest(&store, by_id.into_values().collect())?);
    }
    Ok(outcome)
}

pub fn ingest(dir: &Path, mut progress: impl FnMut(Progress)) -> IngestResult<Ingested> {
    let store = BlobStore::open(dir)?;
    let mut api = Riftcodex::new();
    let mut journal = Journal::open(&store, JOURNAL_FILE);

    let mut cards = Vec::new();
    let mut page_number = 1;
    loop {
        let page = api.page(page_number)?;
        let pages = page.pages;
        cards.extend(page.items);
        progress(Progress {
            done: page_number,
            total: pages,
            stage: "pages",
        });
        if page_number >= pages {
            break;
        }
        page_number += 1;
    }

    let agent = art_agent();
    let total = cards.len();
    let mut entries = Vec::new();
    let mut fetched = 0usize;
    let mut reused = 0usize;
    let mut skipped = 0usize;
    for (index, card) in cards.iter().enumerate() {
        let image = match card.media.image_url.as_deref() {
            None | Some("") => {
                skipped += 1;
                String::new()
            }
            Some(url) => match journal.get(&store, &card.riftbound_id) {
                Some(hash) => {
                    reused += 1;
                    hash.to_string()
                }
                None => {
                    std::thread::sleep(THROTTLE);
                    let bytes = fetch_image(&agent, url)?;
                    let hash = journal.put(&store, &card.riftbound_id, &bytes)?;
                    fetched += 1;
                    hash.to_string()
                }
            },
        };
        entries.push(record(card, image));
        progress(Progress {
            done: index + 1,
            total,
            stage: "images",
        });
    }
    let cards = entries.len();
    let manifest_hash = publish_manifest(&store, entries)?;

    Ok(Ingested {
        manifest: manifest_hash,
        cards,
        images_fetched: fetched,
        images_reused: reused,
        skipped,
    })
}

pub fn load_manifest(dir: &Path) -> IngestResult<Option<RiftboundManifest>> {
    let store = BlobStore::open(dir)?;
    let Some(hash) = spirit_core::refs::read(&store, REF_NAME) else {
        return Ok(None);
    };
    let bytes = store.get(hash)?;
    let manifest: RiftboundManifest = ciborium::from_reader(bytes.as_slice())?;
    Ok(Some(manifest))
}

pub fn load_catalog(dir: &Path) -> IngestResult<Option<StaticCatalog>> {
    Ok(load_manifest(dir)?
        .map(|manifest| StaticCatalog::new(manifest.cards.iter().map(catalog_card).collect())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agni-riftbound-ingest-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_card(name: &str, id: &str, card_type: &str) -> RiftboundCard {
        RiftboundCard {
            name: name.into(),
            image: String::new(),
            riftbound_id: id.into(),
            card_type: card_type.into(),
            supertype: String::new(),
            rarity: "Common".into(),
            domain: vec!["Calm".into()],
            energy: Some(2),
            might: Some(1),
            power: None,
            text: "When you play me, invent a card.".into(),
            set_id: "OGN".into(),
            set_label: "Origins".into(),
            collector_number: Some(7),
            clean_name: name.into(),
            image_url: String::new(),
            tags: vec!["Ionia".into()],
        }
    }

    #[test]
    fn a_stored_manifest_loads_back_as_a_catalog() {
        let dir = scratch("catalog");
        let store = BlobStore::open(&dir).unwrap();
        let manifest = RiftboundManifest {
            set: REF_NAME.into(),
            cards: vec![
                sample_card("Emberwing Scout", "ogn-007-298", "Unit"),
                sample_card("Ember Rune", "ogn-042-298", "Rune"),
            ],
        };
        let mut encoded = Vec::new();
        ciborium::into_writer(&manifest, &mut encoded).unwrap();
        let hash = store.put(&encoded).unwrap();
        spirit_core::refs::write(&store, REF_NAME, hash).unwrap();

        let catalog = load_catalog(&dir).unwrap().unwrap();
        assert_eq!(catalog.len(), 2);
        let dir_without = scratch("empty");
        assert!(load_catalog(&dir_without).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir_without);
    }

    #[test]
    fn a_manifest_written_before_tags_loads_with_empty_tags_and_signature_from_supertype() {
        #[derive(Serialize)]
        struct OldCard {
            name: String,
            image: String,
            riftbound_id: String,
            card_type: String,
            supertype: String,
            rarity: String,
            domain: Vec<String>,
            energy: Option<i64>,
            might: Option<i64>,
            power: Option<i64>,
            text: String,
            set_id: String,
            set_label: String,
            collector_number: Option<i64>,
            clean_name: String,
            image_url: String,
        }
        #[derive(Serialize)]
        struct OldManifest {
            set: String,
            cards: Vec<OldCard>,
        }
        let old = |name: &str, id: &str, supertype: &str| OldCard {
            name: name.into(),
            image: String::new(),
            riftbound_id: id.into(),
            card_type: "Unit".into(),
            supertype: supertype.into(),
            rarity: "Rare".into(),
            domain: vec!["Calm".into()],
            energy: Some(3),
            might: Some(3),
            power: None,
            text: "Some rules text.".into(),
            set_id: "UNL".into(),
            set_label: "Unleashed".into(),
            collector_number: Some(82),
            clean_name: name.into(),
            image_url: String::new(),
        };
        let manifest = OldManifest {
            set: REF_NAME.into(),
            cards: vec![
                old("Lillia - Fae Fawn", "unl-082-219", "Champion"),
                old("Daisy!", "unl-196-219", "Signature"),
            ],
        };
        let mut encoded = Vec::new();
        ciborium::into_writer(&manifest, &mut encoded).unwrap();
        let decoded: RiftboundManifest = ciborium::from_reader(encoded.as_slice()).unwrap();
        assert!(decoded.cards.iter().all(|card| card.tags.is_empty()));
        let cards: Vec<CatalogCard> = decoded.cards.iter().map(catalog_card).collect();
        assert!(cards[0].champion);
        assert!(!cards[0].signature);
        assert!(cards[0].tags.is_empty());
        assert_eq!(cards[0].set_id.as_deref(), Some("UNL"));
        assert_eq!(cards[0].text.as_deref(), Some("Some rules text."));
        assert!(cards[1].signature);
        assert!(!cards[1].champion);
        let fresh = catalog_card(&sample_card("Emberwing Scout", "ogn-007-298", "Unit"));
        assert_eq!(fresh.tags, vec!["Ionia".to_string()]);
        let padded = catalog_card(&record_of(
            &CatalogCard {
                name: "Padded".into(),
                riftbound_id: "ogn-999-298".into(),
                ..Default::default()
            },
            String::new(),
        ));
        assert_eq!(padded.set_id, None);
        assert_eq!(padded.text, None);
        assert_eq!(padded.kind, CardKind::Other);
        let full = catalog_card(&record_of(
            &CatalogCard {
                name: "Lillia - Fae Fawn".into(),
                riftbound_id: "unl-082-219".into(),
                kind: CardKind::Unit,
                champion: true,
                energy: Some(2),
                might: Some(2),
                domain: vec!["Calm".into()],
                tags: vec!["Fae".into(), "Lillia".into()],
                set_id: Some("UNL".into()),
                text: Some("Some rules text.".into()),
                ..Default::default()
            },
            String::new(),
        ));
        assert_eq!(full.kind, CardKind::Unit);
        assert!(full.champion);
        assert_eq!(full.tags, vec!["Fae".to_string(), "Lillia".to_string()]);
        assert_eq!(full.domain, vec!["Calm".to_string()]);
        assert_eq!(full.energy, Some(2));
        assert_eq!(full.set_id.as_deref(), Some("UNL"));
        assert_eq!(full.text.as_deref(), Some("Some rules text."));
    }

    #[test]
    fn the_manifest_stays_readable_as_the_generic_gateway_shape() {
        #[derive(Deserialize)]
        struct Generic {
            set: String,
            cards: Vec<GenericCard>,
        }
        #[derive(Deserialize)]
        struct GenericCard {
            name: String,
            image: String,
        }
        let manifest = RiftboundManifest {
            set: REF_NAME.into(),
            cards: vec![sample_card("Emberwing Scout", "ogn-007-298", "Unit")],
        };
        let mut encoded = Vec::new();
        ciborium::into_writer(&manifest, &mut encoded).unwrap();
        let generic: Generic = ciborium::from_reader(encoded.as_slice()).unwrap();
        assert_eq!(generic.set, "riftbound");
        assert_eq!(generic.cards[0].name, "Emberwing Scout");
        assert_eq!(generic.cards[0].image, "");
    }

    #[test]
    fn deck_scoped_ingest_reuses_the_store_and_publishes_a_merged_manifest() {
        let dir = scratch("deck-art");
        let store = BlobStore::open(&dir).unwrap();
        let art = b"jpeg bytes for the scout".to_vec();
        let hash = store.put(&art).unwrap();
        std::fs::write(
            store.root().join(JOURNAL_FILE),
            format!("OGN-007-298 {hash}\n"),
        )
        .unwrap();
        let wanted = vec![
            CatalogCard {
                name: "Emberwing Scout".into(),
                riftbound_id: "ogn-007-298".into(),
                kind: CardKind::Unit,
                domain: vec!["Fury".into()],
                tags: vec!["Ionia".into()],
                image_url: Some("https://img.example/ogn-007-298.png".into()),
                ..Default::default()
            },
            CatalogCard {
                name: "Unfetchable".into(),
                riftbound_id: "ogn-999-298".into(),
                ..Default::default()
            },
        ];
        let mut seen = Vec::new();
        let arts = ingest_deck_art(&dir, &wanted, Duration::from_millis(0), |progress| {
            seen.push((progress.done, progress.total));
        })
        .unwrap();
        assert_eq!(seen, vec![(1, 2), (2, 2)]);
        assert_eq!(arts.len(), 1);
        assert_eq!(arts[0].0, "ogn-007-298");
        assert_eq!(arts[0].1, art);
        let manifest = load_manifest(&dir).unwrap().unwrap();
        assert_eq!(manifest.cards.len(), 1);
        assert_eq!(manifest.cards[0].riftbound_id, "ogn-007-298");
        assert_eq!(manifest.cards[0].image, hash.to_string());
        assert_eq!(manifest.cards[0].name, "Emberwing Scout");
        assert_eq!(manifest.cards[0].card_type, "Unit");
        assert_eq!(manifest.cards[0].domain, vec!["Fury".to_string()]);
        assert_eq!(manifest.cards[0].tags, vec!["Ionia".to_string()]);
        let again = ingest_deck_art(&dir, &wanted, Duration::from_millis(0), |_| {}).unwrap();
        assert_eq!(again.len(), 1);
        let catalog = load_catalog(&dir).unwrap().unwrap();
        assert_eq!(catalog.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_journal_survives_garbage_lines() {
        let dir = scratch("journal");
        let store = BlobStore::open(&dir).unwrap();
        let hash = store.put(b"png bytes").unwrap();
        std::fs::write(
            store.root().join(JOURNAL_FILE),
            format!("ogn-007-298 {hash}\nbroken line without hash zzz\n\n"),
        )
        .unwrap();
        let journal = art::load_journal(&store, JOURNAL_FILE);
        assert_eq!(journal.len(), 1);
        assert_eq!(journal.get("ogn-007-298"), Some(&hash));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn api_card(name: &str, id: &str, image_url: Option<&str>) -> serde_json::Value {
        let mut card = serde_json::json!({
            "name": name,
            "riftbound_id": id,
            "classification": { "type": "Unit", "supertype": "Champion", "domain": ["Body"] },
            "attributes": { "energy": 4, "might": 4 },
            "text": { "plain": "Hunt 2." },
            "set": { "set_id": "UNL", "label": "Unleashed" },
        });
        if let Some(url) = image_url {
            card["media"] = serde_json::json!({ "image_url": url });
        }
        card
    }

    struct Scripted(std::collections::VecDeque<String>);

    impl crate::transport::Transport for Scripted {
        fn get(
            &mut self,
            _url: &str,
            _accept: &str,
        ) -> Result<String, crate::transport::TransportError> {
            self.0
                .pop_front()
                .ok_or(crate::transport::TransportError::Status(404))
        }
    }

    fn scripted(responses: Vec<serde_json::Value>) -> Riftcodex {
        let script = responses
            .into_iter()
            .map(|value| value.to_string())
            .collect();
        Riftcodex::with_transport("http://riftcodex.test", Box::new(Scripted(script)))
            .retrying(crate::transport::Retry::immediate(1))
            .throttled(Duration::ZERO)
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn the_audit_names_every_id_without_a_stored_blob() {
        let dir = scratch("audit");
        let store = BlobStore::open(&dir).unwrap();
        let held = store.put(b"art for the scout").unwrap();
        let journaled = store.put(b"art for the rune").unwrap();
        let mut scout = sample_card("Emberwing Scout", "ogn-007-298", "Unit");
        scout.image = held.to_string();
        let mut gone = sample_card("Gone", "ogn-008-298", "Unit");
        gone.image = "0".repeat(64);
        let manifest = RiftboundManifest {
            set: REF_NAME.into(),
            cards: vec![scout, gone],
        };
        let mut encoded = Vec::new();
        ciborium::into_writer(&manifest, &mut encoded).unwrap();
        let hash = store.put(&encoded).unwrap();
        spirit_core::refs::write(&store, REF_NAME, hash).unwrap();
        std::fs::write(
            store.root().join(JOURNAL_FILE),
            format!("ogn-042-298 {journaled}\n"),
        )
        .unwrap();
        let missing = audit(
            &dir,
            &ids(&["OGN-007-298", "ogn-008-298", "ogn-042-298", "ven-192-166"]),
        )
        .unwrap();
        assert_eq!(missing, ids(&["ogn-008-298", "ven-192-166"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingesting_ids_fetches_the_missing_art_and_publishes_full_records() {
        let dir = scratch("ids");
        let store = BlobStore::open(&dir).unwrap();
        let journaled = store.put(b"art for the rune").unwrap();
        std::fs::write(
            store.root().join(JOURNAL_FILE),
            format!("ven-r01 {journaled}\n"),
        )
        .unwrap();
        let mut api = scripted(vec![
            serde_json::json!([api_card(
                "Master Yi - Tempered (Starter)",
                "unl-113-219",
                Some("https://img.test/unl-113-219.png")
            )]),
            serde_json::json!([api_card("Tempest Rune", "ven-r01", None)]),
            serde_json::json!([api_card("Bare", "sfd-001-221", None)]),
            serde_json::json!([]),
        ]);
        let mut fetched_urls = Vec::new();
        let mut seen = Vec::new();
        let outcome = ingest_ids_with(
            &dir,
            &mut api,
            |url| {
                fetched_urls.push(url.to_string());
                Ok(b"png bytes for yi".to_vec())
            },
            &ids(&["UNL-113-219", "ven-r01", "sfd-001-221", "ogn-999-298"]),
            |progress| seen.push((progress.done, progress.total)),
        )
        .unwrap();
        assert_eq!(seen, vec![(1, 4), (2, 4), (3, 4), (4, 4)]);
        assert_eq!(fetched_urls, vec!["https://img.test/unl-113-219.png"]);
        assert_eq!(outcome.fetched, ids(&["unl-113-219"]));
        assert_eq!(outcome.reused, ids(&["ven-r01"]));
        assert_eq!(outcome.artless, ids(&["sfd-001-221"]));
        assert_eq!(outcome.unknown, ids(&["ogn-999-298"]));
        assert!(outcome.manifest.is_some());
        let manifest = load_manifest(&dir).unwrap().unwrap();
        let names: Vec<&str> = manifest
            .cards
            .iter()
            .map(|card| card.name.as_str())
            .collect();
        assert_eq!(names, vec!["Master Yi - Tempered", "Tempest Rune"]);
        let yi = &manifest.cards[0];
        assert_eq!(yi.supertype, "Champion");
        assert_eq!(yi.energy, Some(4));
        assert_eq!(yi.text, "Hunt 2.");
        assert_eq!(
            store.get(BlobHash::parse(&yi.image).unwrap()).unwrap(),
            b"png bytes for yi"
        );
        assert_eq!(manifest.cards[1].image, journaled.to_string());
        assert!(audit(&dir, &ids(&["unl-113-219", "ven-r01"]))
            .unwrap()
            .is_empty());

        let mut again = scripted(Vec::new());
        let outcome = ingest_ids_with(
            &dir,
            &mut again,
            |_| panic!("stored art is never fetched twice"),
            &ids(&["unl-113-219", "ven-r01"]),
            |_| {},
        )
        .unwrap();
        assert!(outcome.fetched.is_empty());
        assert_eq!(outcome.reused, ids(&["unl-113-219", "ven-r01"]));
        assert!(outcome.manifest.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_and_catalog_rows_carry_base_names() {
        let starter: ApiCard = serde_json::from_value(api_card(
            "Master Yi - Wuju Bladesman (Starter)",
            "ogs-019-024",
            None,
        ))
        .unwrap();
        assert_eq!(
            record(&starter, String::new()).name,
            "Master Yi - Wuju Bladesman"
        );
        let renamed = sample_card("Curator of the Sands", "ven-192-166", "Unit");
        assert_eq!(catalog_card(&renamed).name, "Nasus - Curator of the Sands");
        let kept = sample_card("Teemo - Scout (GG EZ)", "ogn-001-298", "Unit");
        assert_eq!(catalog_card(&kept).name, "Teemo - Scout (GG EZ)");
    }
}
