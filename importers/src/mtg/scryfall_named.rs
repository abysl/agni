use super::catalog::{LookupError, MtgCard};
use super::Mtg;
use crate::art::art_agent;
use serde::Deserialize;
use std::time::{Duration, Instant};

pub const API_BASE: &str = "https://api.scryfall.com";
pub const THROTTLE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Deserialize)]
pub struct NamedCard {
    pub name: String,
    #[serde(default)]
    pub image_uris: Option<ImageUris>,
    #[serde(default)]
    pub card_faces: Option<Vec<NamedFace>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NamedFace {
    #[serde(default)]
    pub image_uris: Option<ImageUris>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageUris {
    pub normal: String,
}

pub fn image_url(card: &NamedCard) -> Option<String> {
    card.image_uris
        .as_ref()
        .or_else(|| {
            card.card_faces
                .as_ref()
                .and_then(|faces| faces.first())
                .and_then(|face| face.image_uris.as_ref())
        })
        .map(|uris| uris.normal.clone())
}

pub fn card_of(card: &NamedCard) -> MtgCard {
    MtgCard {
        name: card.name.clone(),
        image_url: image_url(card),
    }
}

pub use crate::naming::encode_component;

pub struct Scryfall {
    agent: ureq::Agent,
    base: String,
    last_request: Option<Instant>,
}

impl Default for Scryfall {
    fn default() -> Self {
        Self::new()
    }
}

impl Scryfall {
    pub fn new() -> Self {
        Self::with_base(API_BASE)
    }

    pub fn with_base(base: &str) -> Self {
        Self {
            agent: art_agent(),
            base: base.trim_end_matches('/').to_string(),
            last_request: None,
        }
    }

    fn throttle(&mut self) {
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < THROTTLE {
                std::thread::sleep(THROTTLE - elapsed);
            }
        }
        self.last_request = Some(Instant::now());
    }

    fn named(&mut self, query: &str) -> Result<Option<NamedCard>, LookupError> {
        self.throttle();
        let url = format!("{}/cards/named?{query}", self.base);
        match self.agent.get(&url).call() {
            Ok(response) => response
                .into_json()
                .map(Some)
                .map_err(|error| LookupError(format!("{url}: {error}"))),
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(error) => Err(LookupError(format!("{url}: {error}"))),
        }
    }

    pub fn exact(&mut self, name: &str) -> Result<Option<NamedCard>, LookupError> {
        self.named(&format!("exact={}", encode_component(name)))
    }

    pub fn fuzzy(&mut self, name: &str) -> Result<Option<NamedCard>, LookupError> {
        self.named(&format!("fuzzy={}", encode_component(name)))
    }
}

impl crate::deck::CardLookup<Mtg> for Scryfall {
    fn find(&mut self, name: &String) -> Result<Option<MtgCard>, LookupError> {
        if let Some(card) = self.exact(name)? {
            return Ok(Some(card_of(&card)));
        }
        Ok(self.fuzzy(name)?.as_ref().map(card_of))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(json: &str) -> NamedCard {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_single_faced_card_reads_its_normal_image() {
        let card = decode(
            r#"{"name":"Thornspire Adept","image_uris":{"small":"s","normal":"https://cards.example/adept.jpg"}}"#,
        );
        assert_eq!(
            image_url(&card).as_deref(),
            Some("https://cards.example/adept.jpg")
        );
        assert_eq!(card_of(&card).name, "Thornspire Adept");
    }

    #[test]
    fn a_double_faced_card_falls_back_to_its_front_face() {
        let card = decode(
            r#"{"name":"Mistfen Causeway // Mistfen Deeps","card_faces":[{"image_uris":{"normal":"https://cards.example/front.jpg"}},{"image_uris":{"normal":"https://cards.example/back.jpg"}}]}"#,
        );
        assert_eq!(
            image_url(&card).as_deref(),
            Some("https://cards.example/front.jpg")
        );
    }

    #[test]
    fn an_artless_card_resolves_to_no_url() {
        let card = decode(r#"{"name":"Cinderveil Ward"}"#);
        assert!(image_url(&card).is_none());
        assert!(card_of(&card).image_url.is_none());
    }
}
