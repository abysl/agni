use super::catalog::{
    normalize_name, CardKind, CatalogCard, LookupError, SUPERTYPE_CHAMPION, SUPERTYPE_SIGNATURE,
};
use super::resolve::canonical_name;
use super::{Identifier, Riftbound};
use crate::art::USER_AGENT;
use crate::transport::{GaveUp, Retry, Transport, TransportError, UreqTransport};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

pub const API_BASE: &str = "https://api.riftcodex.com";
pub const PAGE_SIZE: usize = 100;
pub const THROTTLE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    pub items: Vec<ApiCard>,
    pub total: usize,
    pub page: usize,
    pub pages: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiCard {
    pub name: String,
    pub riftbound_id: String,
    #[serde(default)]
    pub collector_number: Option<i64>,
    pub classification: Classification,
    #[serde(default)]
    pub attributes: Attributes,
    #[serde(default)]
    pub text: CardText,
    pub set: SetInfo,
    #[serde(default)]
    pub media: Media,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Classification {
    #[serde(rename = "type")]
    #[serde(default)]
    pub card_type: Option<String>,
    #[serde(default)]
    pub supertype: Option<String>,
    #[serde(default)]
    pub rarity: Option<String>,
    #[serde(default)]
    pub domain: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Attributes {
    #[serde(default)]
    pub energy: Option<i64>,
    #[serde(default)]
    pub might: Option<i64>,
    #[serde(default)]
    pub power: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CardText {
    #[serde(default)]
    pub plain: Option<String>,
    #[serde(default)]
    pub flavour: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetInfo {
    pub set_id: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Media {
    #[serde(default)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub clean_name: Option<String>,
}

pub fn catalog_card(card: &ApiCard) -> CatalogCard {
    CatalogCard {
        name: canonical_name(&card.riftbound_id, &card.name),
        riftbound_id: card.riftbound_id.clone(),
        kind: card
            .classification
            .card_type
            .as_deref()
            .map(CardKind::parse)
            .unwrap_or(CardKind::Other),
        champion: card.classification.supertype.as_deref() == Some(SUPERTYPE_CHAMPION),
        energy: crate::riftbound::ingest::small(card.attributes.energy),
        power: crate::riftbound::ingest::small(card.attributes.power),
        might: crate::riftbound::ingest::small(card.attributes.might),
        image_url: card.media.image_url.clone(),
        domain: card.classification.domain.clone(),
        tags: card.tags.clone(),
        signature: card.classification.supertype.as_deref() == Some(SUPERTYPE_SIGNATURE),
        set_id: Some(card.set.set_id.clone()).filter(|set| !set.is_empty()),
        text: card.text.plain.clone().filter(|text| !text.is_empty()),
    }
}

pub use crate::naming::encode_component;

pub struct Riftcodex {
    transport: Box<dyn Transport>,
    retry: Retry,
    throttle: Duration,
    base: String,
    last_request: Option<Instant>,
}

impl Default for Riftcodex {
    fn default() -> Self {
        Self::new()
    }
}

fn throttle(last_request: &mut Option<Instant>, gap: Duration) {
    if let Some(last) = *last_request {
        let elapsed = last.elapsed();
        if elapsed < gap {
            std::thread::sleep(gap - elapsed);
        }
    }
    *last_request = Some(Instant::now());
}

impl Riftcodex {
    pub fn new() -> Self {
        Self::with_base(API_BASE)
    }

    pub fn with_base(base: &str) -> Self {
        Self::with_transport(base, Box::new(UreqTransport::new(USER_AGENT)))
    }

    pub fn with_transport(base: &str, transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            retry: Retry::default(),
            throttle: THROTTLE,
            base: base.trim_end_matches('/').to_string(),
            last_request: None,
        }
    }

    pub fn retrying(mut self, retry: Retry) -> Self {
        self.retry = retry;
        self
    }

    pub fn throttled(mut self, throttle: Duration) -> Self {
        self.throttle = throttle;
        self
    }

    fn get<T: serde::de::DeserializeOwned>(
        &mut self,
        path: &str,
    ) -> Result<Option<T>, LookupError> {
        let url = format!("{}{path}", self.base);
        let Self {
            transport,
            retry,
            throttle: gap,
            last_request,
            ..
        } = self;
        let fetched = retry.run(|| {
            throttle(last_request, *gap);
            transport.get(&url, "application/json")
        });
        match fetched {
            Ok(body) => serde_json::from_str(&body)
                .map(Some)
                .map_err(|error| LookupError(format!("{url}: {error}"))),
            Err(GaveUp {
                error: TransportError::Status(404),
                ..
            }) => Ok(None),
            Err(gave_up) => Err(LookupError(format!("{url}: {gave_up}"))),
        }
    }

    pub fn page(&mut self, page: usize) -> Result<Page, LookupError> {
        self.get(&format!("/cards?size={PAGE_SIZE}&page={page}"))?
            .ok_or_else(|| LookupError(format!("card page {page} does not exist")))
    }

    pub fn fuzzy(&mut self, name: &str) -> Result<Vec<ApiCard>, LookupError> {
        #[derive(Deserialize)]
        struct Items {
            items: Vec<ApiCard>,
        }
        let path = format!("/cards/name?fuzzy={}", encode_component(name));
        Ok(self
            .get::<Items>(&path)?
            .map(|i| i.items)
            .unwrap_or_default())
    }

    pub fn by_partial_id(&mut self, fragment: &str) -> Result<Vec<ApiCard>, LookupError> {
        let path = format!("/cards/riftbound/{}", encode_component(fragment));
        Ok(self.get::<Vec<ApiCard>>(&path)?.unwrap_or_default())
    }
}

fn named(item: &ApiCard, wanted: &str) -> bool {
    normalize_name(&item.name) == wanted
        || normalize_name(&canonical_name(&item.riftbound_id, &item.name)) == wanted
        || item
            .metadata
            .clean_name
            .as_deref()
            .map(|clean| normalize_name(clean) == wanted)
            .unwrap_or(false)
}

fn pick_by_name(items: &[ApiCard], name: &str) -> Option<CatalogCard> {
    let wanted = normalize_name(name);
    let exact = items
        .iter()
        .filter(|item| named(item, &wanted))
        .min_by_key(|item| item.riftbound_id.to_ascii_lowercase());
    match exact {
        Some(item) => Some(catalog_card(item)),
        None if items.len() == 1 => Some(catalog_card(&items[0])),
        None => None,
    }
}

fn pick_by_fragment(items: &[ApiCard], fragment: &str) -> Option<CatalogCard> {
    let fragment = fragment.to_ascii_lowercase();
    if let Some(item) = items
        .iter()
        .find(|item| item.riftbound_id.eq_ignore_ascii_case(&fragment))
    {
        return Some(catalog_card(item));
    }
    let prefix = format!("{fragment}-");
    let mut seen = BTreeSet::new();
    let mut candidates = items
        .iter()
        .filter(|item| item.riftbound_id.to_ascii_lowercase().starts_with(&prefix))
        .filter(|item| seen.insert(item.riftbound_id.to_ascii_lowercase()));
    match (candidates.next(), candidates.next()) {
        (Some(item), None) => Some(catalog_card(item)),
        _ => None,
    }
}

impl crate::deck::CardLookup<Riftbound> for Riftcodex {
    fn find(&mut self, identifier: &Identifier) -> Result<Option<CatalogCard>, LookupError> {
        match identifier {
            Identifier::Name(name) => {
                let items = self.fuzzy(name)?;
                Ok(pick_by_name(&items, name))
            }
            Identifier::Code(code) => {
                let fragment = code.id_fragment();
                let items = self.by_partial_id(&fragment)?;
                Ok(pick_by_fragment(&items, &fragment))
            }
            Identifier::Id(id) => {
                let items = self.by_partial_id(id)?;
                Ok(pick_by_fragment(&items, id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::card_code::CardCode;
    use super::*;
    use crate::deck::CardLookup;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn api_card(name: &str, id: &str, card_type: &str, clean: Option<&str>) -> ApiCard {
        ApiCard {
            name: name.into(),
            riftbound_id: id.into(),
            collector_number: None,
            classification: Classification {
                card_type: Some(card_type.into()),
                supertype: None,
                rarity: None,
                domain: Vec::new(),
            },
            attributes: Attributes::default(),
            text: CardText::default(),
            set: SetInfo {
                set_id: "OGN".into(),
                label: None,
            },
            media: Media::default(),
            tags: Vec::new(),
            metadata: Metadata {
                clean_name: clean.map(String::from),
            },
        }
    }

    #[test]
    fn fuzzy_picks_prefer_exact_then_unique() {
        let items = vec![
            api_card("Emberwing Scout", "ogn-007-298", "Unit", None),
            api_card(
                "Emberwing Scout (Alternate Art)",
                "ogn-007a-298",
                "Unit",
                None,
            ),
        ];
        let hit = pick_by_name(&items, "emberwing scout").unwrap();
        assert_eq!(hit.riftbound_id, "ogn-007-298");
        assert!(pick_by_name(&items, "emberwing").is_none());
        let single = vec![api_card("Gloomvale Trickster", "ogn-101-298", "Unit", None)];
        assert!(pick_by_name(&single, "gloomvale").is_some());
    }

    #[test]
    fn clean_names_match_when_punctuation_differs() {
        let items = vec![api_card(
            "Vi - Piltover Patroller (Signature)",
            "unl-229*-219",
            "Legend",
            Some("Vi Piltover Patroller Signature"),
        )];
        assert!(pick_by_name(&items, "Vi Piltover Patroller Signature").is_some());
    }

    #[test]
    fn fragments_match_exact_ids_and_set_size_suffixes() {
        let items = vec![
            api_card("Tempest Rune", "ven-r01", "Rune", None),
            api_card("Sudden Undertow", "ogn-045-298", "Spell", None),
        ];
        assert_eq!(
            pick_by_fragment(&items, "ven-r01").unwrap().name,
            "Tempest Rune"
        );
        assert_eq!(
            pick_by_fragment(&items, "ogn-045").unwrap().name,
            "Sudden Undertow"
        );
        assert!(pick_by_fragment(&items, "ogn-04").is_none());
    }

    fn json_card(name: &str, id: &str, card_type: &str) -> serde_json::Value {
        json!({
            "name": name,
            "riftbound_id": id,
            "classification": { "type": card_type },
            "set": { "set_id": "UNL" },
        })
    }

    #[derive(Default)]
    struct Script {
        responses: VecDeque<Result<String, TransportError>>,
        requests: Vec<String>,
    }

    struct Scripted(Arc<Mutex<Script>>);

    impl Transport for Scripted {
        fn get(&mut self, url: &str, _accept: &str) -> Result<String, TransportError> {
            let mut script = self.0.lock().unwrap();
            script.requests.push(url.to_string());
            script
                .responses
                .pop_front()
                .unwrap_or(Err(TransportError::Failed("script exhausted".into())))
        }
    }

    fn scripted(
        responses: Vec<Result<serde_json::Value, TransportError>>,
    ) -> (Riftcodex, Arc<Mutex<Script>>) {
        let script = Arc::new(Mutex::new(Script {
            responses: responses
                .into_iter()
                .map(|response| response.map(|value| value.to_string()))
                .collect(),
            requests: Vec::new(),
        }));
        let api =
            Riftcodex::with_transport("http://riftcodex.test/", Box::new(Scripted(script.clone())))
                .retrying(Retry::immediate(3))
                .throttled(Duration::ZERO);
        (api, script)
    }

    fn lillia_page() -> serde_json::Value {
        json!({ "items": [
            json_card("Lillia - Fae Fawn", "unl-082-219", "Unit"),
            json_card("Lillia - Bashful Bloom (Signature)", "unl-230*-219", "Legend"),
            json_card("Lillia - Bashful Bloom", "unl-189-219", "Legend"),
            json_card("Lillia - Bashful Bloom (Overnumbered)", "unl-230-219", "Legend"),
        ]})
    }

    #[test]
    fn a_transient_failure_is_retried_and_the_legend_still_resolves() {
        let (mut api, script) = scripted(vec![
            Err(TransportError::Failed("the fetch failed: timeout".into())),
            Err(TransportError::Status(503)),
            Ok(lillia_page()),
        ]);
        let hit = api
            .find(&Identifier::Name("Lillia - Bashful Bloom".into()))
            .unwrap()
            .unwrap();
        assert_eq!(hit.riftbound_id, "unl-189-219");
        assert_eq!(hit.kind, CardKind::Legend);
        let script = script.lock().unwrap();
        assert_eq!(script.requests.len(), 3);
        assert!(script.requests.iter().all(
            |url| url == "http://riftcodex.test/cards/name?fuzzy=Lillia%20-%20Bashful%20Bloom"
        ));
    }

    #[test]
    fn exhausted_retries_name_the_url_and_the_reason() {
        let (mut api, script) = scripted(vec![
            Err(TransportError::Status(429)),
            Err(TransportError::Status(500)),
            Err(TransportError::Failed(
                "the fetch failed: connection reset".into(),
            )),
            Ok(lillia_page()),
        ]);
        let error = api
            .find(&Identifier::Code(CardCode::parse("UNL-230").unwrap()))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "card lookup failed: http://riftcodex.test/cards/riftbound/unl-230: the fetch failed: connection reset (after 3 attempts)"
        );
        assert_eq!(script.lock().unwrap().requests.len(), 3);
    }

    #[test]
    fn a_missing_card_is_not_a_failure_and_a_client_error_is_not_retried() {
        let (mut api, script) = scripted(vec![
            Err(TransportError::Status(404)),
            Err(TransportError::Status(400)),
            Ok(lillia_page()),
        ]);
        assert_eq!(api.find(&Identifier::Id("unl-999".into())).unwrap(), None);
        let error = api
            .find(&Identifier::Id("unl-998".into()))
            .unwrap_err()
            .to_string();
        assert!(error.ends_with("the site answered 400"), "{error}");
        assert_eq!(script.lock().unwrap().requests.len(), 2);
    }

    #[test]
    fn a_code_lookup_finds_the_base_printing_beside_its_signature() {
        let both = json!([
            json_card(
                "Lillia - Bashful Bloom (Signature)",
                "unl-230*-219",
                "Legend"
            ),
            json_card(
                "Lillia - Bashful Bloom (Overnumbered)",
                "unl-230-219",
                "Legend"
            ),
        ]);
        let (mut api, _) = scripted(vec![Ok(both.clone()), Ok(both)]);
        let base = api
            .find(&Identifier::Code(CardCode::parse("UNL-230").unwrap()))
            .unwrap()
            .unwrap();
        assert_eq!(base.riftbound_id, "unl-230-219");
        let signed = api
            .find(&Identifier::Code(CardCode::parse("UNL-230s").unwrap()))
            .unwrap()
            .unwrap();
        assert_eq!(signed.riftbound_id, "unl-230*-219");
    }

    #[test]
    fn catalog_cards_carry_base_names() {
        let starter = api_card(
            "Master Yi - Wuju Bladesman (Starter)",
            "ogs-019-024",
            "Legend",
            Some("Master Yi Wuju Bladesman Starter"),
        );
        assert_eq!(catalog_card(&starter).name, "Master Yi - Wuju Bladesman");
        let renamed = api_card("Curator of the Sands", "ven-192-166", "Legend", None);
        assert_eq!(catalog_card(&renamed).name, "Nasus - Curator of the Sands");
        let alternate = api_card(
            "Nasus, Ascended (Alternate Art)",
            "ven-046a-166",
            "Unit",
            None,
        );
        assert_eq!(catalog_card(&alternate).name, "Nasus, Ascended");
        let kept = api_card("Teemo - Scout (GG EZ)", "opp-197b-298", "Unit", None);
        assert_eq!(catalog_card(&kept).name, "Teemo - Scout (GG EZ)");
    }

    #[test]
    fn a_name_lookup_matches_a_print_by_its_base_name_and_prefers_the_lowest_id() {
        let items = vec![
            api_card(
                "Master Yi - Wuju Bladesman (Metal)",
                "opp-019-024",
                "Legend",
                None,
            ),
            api_card("Master Yi - Wuju Bladesman", "opp-019-024", "Legend", None),
            api_card(
                "Master Yi - Wuju Bladesman (Starter)",
                "ogs-019-024",
                "Legend",
                None,
            ),
        ];
        let hit = pick_by_name(&items, "Master Yi, Wuju Bladesman").unwrap();
        assert_eq!(hit.riftbound_id, "ogs-019-024");
        assert_eq!(hit.name, "Master Yi - Wuju Bladesman");
        let renamed = vec![
            api_card("Curator of the Sands", "ven-192-166", "Legend", None),
            api_card(
                "Nasus - Curator of the Sands",
                "ven-145-166",
                "Legend",
                None,
            ),
        ];
        let hit = pick_by_name(&renamed, "Nasus - Curator of the Sands").unwrap();
        assert_eq!(hit.riftbound_id, "ven-145-166");
        assert_eq!(hit.name, "Nasus - Curator of the Sands");
    }

    struct Simulated {
        rows: Vec<ApiCard>,
    }

    impl Simulated {
        fn from_fixture() -> Self {
            let rows = super::super::resolve::fixtures::rows()
                .into_iter()
                .map(|row| ApiCard {
                    name: row.name,
                    riftbound_id: row.id,
                    collector_number: None,
                    classification: Classification {
                        card_type: Some(row.kind),
                        supertype: row.supertype,
                        rarity: None,
                        domain: row.domain,
                    },
                    attributes: Attributes {
                        energy: row.energy.map(i64::from),
                        might: row.might.map(i64::from),
                        power: row.power.map(i64::from),
                    },
                    text: CardText::default(),
                    set: SetInfo {
                        set_id: String::new(),
                        label: None,
                    },
                    media: Media::default(),
                    tags: row.tags,
                    metadata: Metadata {
                        clean_name: row.clean_name,
                    },
                })
                .collect();
            Self { rows }
        }

        fn fuzzy(&self, name: &str) -> Vec<ApiCard> {
            let wanted = normalize_name(name);
            self.rows
                .iter()
                .filter(|row| normalize_name(&row.name).contains(&wanted))
                .cloned()
                .collect()
        }

        fn by_partial_id(&self, fragment: &str) -> Vec<ApiCard> {
            let fragment = fragment.to_ascii_lowercase();
            self.rows
                .iter()
                .filter(|row| row.riftbound_id.to_ascii_lowercase().starts_with(&fragment))
                .cloned()
                .collect()
        }
    }

    impl CardLookup<Riftbound> for Simulated {
        fn find(&mut self, identifier: &Identifier) -> Result<Option<CatalogCard>, LookupError> {
            Ok(match identifier {
                Identifier::Name(name) => pick_by_name(&self.fuzzy(name), name),
                Identifier::Code(code) => {
                    let fragment = code.id_fragment();
                    pick_by_fragment(&self.by_partial_id(&fragment), &fragment)
                }
                Identifier::Id(id) => pick_by_fragment(&self.by_partial_id(id), id),
            })
        }
    }

    #[test]
    fn the_four_tournament_lists_resolve_to_canonical_names_through_riftcodex_picks() {
        use super::super::resolve::{fixtures, resolve};
        use super::super::text_list::parse_text;
        let canonical = fixtures::canonical();
        let mut lookup = Simulated::from_fixture();
        for (deck_name, text) in fixtures::DECK_LISTS {
            let parsed = parse_text(text).unwrap();
            let resolution = resolve(&parsed, &mut lookup).unwrap();
            assert!(
                resolution.unresolved.is_empty(),
                "{deck_name}: {:?}",
                resolution.unresolved
            );
            for (zone, names) in fixtures::zone_names(&resolution.deck) {
                for (name, _) in names {
                    assert!(
                        canonical.contains_key(&name),
                        "{deck_name} {zone}: {name:?} is not a canonical name"
                    );
                }
            }
            let legend = resolution.deck.legend.unwrap();
            assert_eq!(
                canonical.get(&legend.name).map(String::as_str),
                Some(legend.riftbound_id.as_str()),
                "{deck_name}: the legend resolves to the canonical print"
            );
        }
    }

    #[test]
    fn exact_name_ties_break_by_id_whatever_the_api_order() {
        let unl = api_card("Lillia - Protector of Dreams", "unl-058-219", "Unit", None);
        let opp = api_card("Lillia - Protector of Dreams", "opp-058-219", "Unit", None);
        let first =
            pick_by_name(&[unl.clone(), opp.clone()], "Lillia - Protector of Dreams").unwrap();
        let second = pick_by_name(&[opp, unl], "Lillia - Protector of Dreams").unwrap();
        assert_eq!(first.riftbound_id, "opp-058-219");
        assert_eq!(first, second);
    }

    #[test]
    fn fragments_prefer_the_exact_id_and_refuse_ambiguity() {
        let items = vec![
            api_card("Base", "ogn-007-298", "Unit", None),
            api_card("Base again", "ogn-007-298", "Unit", None),
            api_card("Alternate", "ogn-007a-298", "Unit", None),
        ];
        assert_eq!(
            pick_by_fragment(&items, "OGN-007-298").unwrap().name,
            "Base"
        );
        assert_eq!(pick_by_fragment(&items, "ogn-007").unwrap().name, "Base");
        let clashing = vec![
            api_card("One", "ogn-007-298", "Unit", None),
            api_card("Two", "ogn-007-299", "Unit", None),
        ];
        assert!(pick_by_fragment(&clashing, "ogn-007").is_none());
    }
}
