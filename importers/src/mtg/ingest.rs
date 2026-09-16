use super::catalog::{MtgCard, StaticCatalog};
use crate::art;
use crate::IngestResult;
use serde::{Deserialize, Serialize};
use spirit_core::{BlobHash, BlobStore};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub use crate::art::{Progress, WantedArt};

pub const REF_NAME: &str = "mtg";
pub const JOURNAL_FILE: &str = "mtg-images";
pub const DECK_THROTTLE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub set: String,
    pub cards: Vec<ManifestCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCard {
    pub name: String,
    #[serde(default)]
    pub mana_cost: String,
    #[serde(default)]
    pub type_line: String,
    #[serde(default)]
    pub oracle_text: String,
    pub image: String,
    #[serde(default)]
    pub image_url: String,
}

pub fn catalog_card(card: &ManifestCard) -> MtgCard {
    MtgCard {
        name: card.name.clone(),
        image_url: if card.image_url.is_empty() {
            None
        } else {
            Some(card.image_url.clone())
        },
    }
}

pub fn publish_manifest(store: &BlobStore, mut cards: Vec<ManifestCard>) -> IngestResult<BlobHash> {
    cards.sort_by(|left, right| left.name.cmp(&right.name));
    let manifest = Manifest {
        set: REF_NAME.to_string(),
        cards,
    };
    let mut encoded = Vec::new();
    ciborium::into_writer(&manifest, &mut encoded)?;
    let hash = store.put(&encoded)?;
    spirit_core::refs::write(store, REF_NAME, hash)?;
    Ok(hash)
}

pub fn load_manifest(dir: &Path) -> IngestResult<Option<Manifest>> {
    let store = BlobStore::open(dir)?;
    let Some(hash) = spirit_core::refs::read(&store, REF_NAME) else {
        return Ok(None);
    };
    let bytes = store.get(hash)?;
    let manifest: Manifest = ciborium::from_reader(bytes.as_slice())?;
    Ok(Some(manifest))
}

pub fn load_catalog(dir: &Path) -> IngestResult<Option<StaticCatalog>> {
    Ok(load_manifest(dir)?
        .map(|manifest| StaticCatalog::new(manifest.cards.iter().map(catalog_card).collect())))
}

pub fn art_from_store(dir: &Path, name: &str) -> Option<Vec<u8>> {
    let manifest = load_manifest(dir).ok()??;
    let card = manifest
        .cards
        .iter()
        .find(|card| card.name.eq_ignore_ascii_case(name))?;
    let hash = BlobHash::parse(&card.image)?;
    BlobStore::open(dir).ok()?.get(hash).ok()
}

fn minimal_record(wanted: &WantedArt, image: String) -> ManifestCard {
    ManifestCard {
        name: wanted.name.clone(),
        mana_cost: String::new(),
        type_line: String::new(),
        oracle_text: String::new(),
        image,
        image_url: wanted.image_url.clone().unwrap_or_default(),
    }
}

pub fn ingest_deck_art(
    dir: &Path,
    wanted: &[WantedArt],
    throttle: Duration,
    progress: impl FnMut(Progress),
) -> IngestResult<Vec<(String, Vec<u8>)>> {
    let store = BlobStore::open(dir)?;
    let mut by_name: BTreeMap<String, ManifestCard> = load_manifest(dir)?
        .map(|manifest| manifest.cards)
        .unwrap_or_default()
        .into_iter()
        .map(|card| (card.name.to_ascii_lowercase(), card))
        .collect();
    let fetched = art::fetch_deck_art(
        &store,
        JOURNAL_FILE,
        throttle,
        wanted,
        |key| {
            by_name
                .get(key)
                .and_then(|card| BlobHash::parse(&card.image))
        },
        progress,
    )?;
    let mut changed = false;
    let mut arts = Vec::new();
    for art in fetched {
        let key = art.key.to_ascii_lowercase();
        let hex = art.hash.to_string();
        match by_name.get_mut(&key) {
            Some(entry) => {
                if entry.image != hex {
                    entry.image = hex;
                    changed = true;
                }
            }
            None => {
                let want = wanted
                    .iter()
                    .find(|want| want.key.eq_ignore_ascii_case(&art.key))
                    .expect("fetched art matches a wanted card");
                by_name.insert(key, minimal_record(want, hex));
                changed = true;
            }
        }
        arts.push((art.key, art.bytes));
    }
    if changed {
        publish_manifest(&store, by_name.into_values().collect())?;
    }
    Ok(arts)
}

#[cfg(test)]
mod tests {
    use super::super::catalog::CardLookup;
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("agni-mtg-ingest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample(name: &str, image: &str) -> ManifestCard {
        ManifestCard {
            name: name.into(),
            mana_cost: "{1}{G}".into(),
            type_line: "Creature — Invented Adept".into(),
            oracle_text: "Invent a card.".into(),
            image: image.into(),
            image_url: format!("https://cards.example/{name}.jpg"),
        }
    }

    #[test]
    fn a_published_manifest_reloads_as_a_catalog() {
        let dir = scratch("catalog");
        let store = BlobStore::open(&dir).unwrap();
        let art = store.put(b"adept art").unwrap();
        publish_manifest(
            &store,
            vec![
                sample("Thornspire Adept", &art.to_string()),
                sample("Mistfen Causeway", ""),
            ],
        )
        .unwrap();
        let manifest = load_manifest(&dir).unwrap().unwrap();
        assert_eq!(manifest.set, "mtg");
        assert_eq!(manifest.cards.len(), 2);
        assert_eq!(manifest.cards[0].name, "Mistfen Causeway");
        let mut catalog = load_catalog(&dir).unwrap().unwrap();
        assert_eq!(
            catalog
                .by_name("thornspire adept")
                .unwrap()
                .unwrap()
                .image_url
                .as_deref(),
            Some("https://cards.example/Thornspire Adept.jpg")
        );
        assert_eq!(
            art_from_store(&dir, "Thornspire Adept"),
            Some(b"adept art".to_vec())
        );
        assert!(art_from_store(&dir, "Mistfen Causeway").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn journaled_art_merges_into_the_manifest_without_a_fetch() {
        let dir = scratch("merge");
        let store = BlobStore::open(&dir).unwrap();
        let hash = store.put(b"causeway art").unwrap();
        std::fs::write(
            store.root().join(JOURNAL_FILE),
            format!("mistfen causeway {hash}\n"),
        )
        .unwrap();
        let wanted = vec![
            WantedArt {
                key: "Mistfen Causeway".into(),
                name: "Mistfen Causeway".into(),
                image_url: Some("https://cards.example/causeway.jpg".into()),
            },
            WantedArt {
                key: "Cinderveil Ward".into(),
                name: "Cinderveil Ward".into(),
                image_url: None,
            },
        ];
        let arts = ingest_deck_art(&dir, &wanted, Duration::from_millis(0), |_| {}).unwrap();
        assert_eq!(arts.len(), 1);
        assert_eq!(arts[0].0, "Mistfen Causeway");
        assert_eq!(arts[0].1, b"causeway art");
        let manifest = load_manifest(&dir).unwrap().unwrap();
        assert_eq!(manifest.cards.len(), 1);
        assert_eq!(manifest.cards[0].name, "Mistfen Causeway");
        assert_eq!(manifest.cards[0].image, hash.to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_manifest_without_art_urls_still_decodes() {
        let dir = scratch("defaults");
        let store = BlobStore::open(&dir).unwrap();
        let mut encoded = Vec::new();
        #[derive(Serialize)]
        struct Terse {
            set: String,
            cards: Vec<TerseCard>,
        }
        #[derive(Serialize)]
        struct TerseCard {
            name: String,
            image: String,
        }
        ciborium::into_writer(
            &Terse {
                set: "mtg".into(),
                cards: vec![TerseCard {
                    name: "Cinderveil Ward".into(),
                    image: String::new(),
                }],
            },
            &mut encoded,
        )
        .unwrap();
        let hash = store.put(&encoded).unwrap();
        spirit_core::refs::write(&store, REF_NAME, hash).unwrap();
        let manifest = load_manifest(&dir).unwrap().unwrap();
        assert_eq!(manifest.cards[0].mana_cost, "");
        assert!(catalog_card(&manifest.cards[0]).image_url.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
