use crate::art::{art_agent, fetch_image};
use crate::IngestResult;
use serde::{Deserialize, Serialize};
use spirit_core::{BlobHash, BlobStore};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

const COURTESY_DELAY: Duration = Duration::from_millis(100);

#[derive(Deserialize)]
struct SearchPage {
    data: Vec<ScryfallCard>,
    has_more: bool,
    next_page: Option<String>,
}

#[derive(Deserialize)]
struct ScryfallCard {
    name: String,
    #[serde(default)]
    mana_cost: Option<String>,
    #[serde(default)]
    type_line: Option<String>,
    #[serde(default)]
    oracle_text: Option<String>,
    #[serde(default)]
    image_uris: Option<ImageUris>,
    #[serde(default)]
    card_faces: Option<Vec<CardFace>>,
}

#[derive(Deserialize)]
struct CardFace {
    #[serde(default)]
    mana_cost: Option<String>,
    #[serde(default)]
    type_line: Option<String>,
    #[serde(default)]
    oracle_text: Option<String>,
    #[serde(default)]
    image_uris: Option<ImageUris>,
}

#[derive(Deserialize)]
struct ImageUris {
    normal: String,
}

#[derive(Serialize)]
pub struct Manifest {
    pub set: String,
    pub cards: Vec<ManifestCard>,
}

#[derive(Serialize)]
pub struct ManifestCard {
    pub name: String,
    pub mana_cost: String,
    pub type_line: String,
    pub oracle_text: String,
    pub image: String,
}

fn front(card: &ScryfallCard) -> Option<&CardFace> {
    card.card_faces.as_ref().and_then(|faces| faces.first())
}

fn image_url(card: &ScryfallCard) -> Option<String> {
    card.image_uris
        .as_ref()
        .or_else(|| front(card).and_then(|face| face.image_uris.as_ref()))
        .map(|uris| uris.normal.clone())
}

fn text_field(
    card: &ScryfallCard,
    own: &Option<String>,
    from_face: impl Fn(&CardFace) -> Option<String>,
) -> String {
    own.clone()
        .or_else(|| front(card).and_then(from_face))
        .unwrap_or_default()
}

pub struct Ingested {
    pub manifest: BlobHash,
    pub cards: usize,
    pub skipped: usize,
}

pub fn ingest(
    set: &str,
    dir: &Path,
    mut progress: impl FnMut(usize, usize),
) -> IngestResult<Ingested> {
    let store = BlobStore::open(dir)?;
    let agent = art_agent();

    let mut cards = Vec::new();
    let mut url = format!("https://api.scryfall.com/cards/search?q=set%3A{set}&order=name");
    loop {
        let page: SearchPage = agent.get(&url).call()?.into_json()?;
        cards.extend(page.data);
        match (page.has_more, page.next_page) {
            (true, Some(next)) => {
                url = next;
                sleep(COURTESY_DELAY);
            }
            _ => break,
        }
    }

    let total = cards.len();
    let mut entries = Vec::new();
    let mut skipped = 0usize;
    for (index, card) in cards.iter().enumerate() {
        let Some(image_url) = image_url(card) else {
            skipped += 1;
            continue;
        };
        sleep(COURTESY_DELAY);
        let image_hash = store.put(&fetch_image(&agent, &image_url)?)?;
        entries.push(ManifestCard {
            name: card.name.clone(),
            mana_cost: text_field(card, &card.mana_cost, |face| face.mana_cost.clone()),
            type_line: text_field(card, &card.type_line, |face| face.type_line.clone()),
            oracle_text: text_field(card, &card.oracle_text, |face| face.oracle_text.clone()),
            image: image_hash.to_string(),
        });
        progress(index + 1, total);
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    let manifest = Manifest {
        set: set.to_string(),
        cards: entries,
    };
    let mut encoded = Vec::new();
    ciborium::into_writer(&manifest, &mut encoded)?;
    let manifest_hash = store.put(&encoded)?;

    spirit_core::refs::write(&store, set, manifest_hash)?;

    Ok(Ingested {
        manifest: manifest_hash,
        cards: manifest.cards.len(),
        skipped,
    })
}
