use super::json::deck_json;
use super::resolve::{resolve, Resolution};
use super::Riftbound;
use super::{link, parse_deck, DeckSource, ParsedDeck};
use crate::art::USER_AGENT;
use crate::deck::CardLookup;
use crate::transport::{Retry, Transport, UreqTransport};
use serde_json::{json, Value};

pub use super::deck_code::encode_deck as deck_code_for;

pub const FETCH_USER_AGENT: &str = USER_AGENT;
pub const FETCH_ACCEPT: &str = "*/*";

pub trait Fetch {
    fn get(&mut self, url: &str) -> Result<String, String>;
}

pub struct UreqFetch {
    transport: UreqTransport,
    retry: Retry,
}

impl Default for UreqFetch {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqFetch {
    pub fn new() -> Self {
        Self {
            transport: UreqTransport::new(FETCH_USER_AGENT),
            retry: Retry::default(),
        }
    }
}

impl Fetch for UreqFetch {
    fn get(&mut self, url: &str) -> Result<String, String> {
        let Self { transport, retry } = self;
        retry
            .run(|| transport.get(url, FETCH_ACCEPT))
            .map_err(|gave_up| gave_up.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckQuery {
    Url(String),
    Code(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryReply {
    pub status: u16,
    pub body: Value,
}

fn error_reply(status: u16, message: impl std::fmt::Display) -> QueryReply {
    QueryReply {
        status,
        body: json!({ "error": message.to_string() }),
    }
}

fn empty(resolution: &Resolution) -> bool {
    let deck = &resolution.deck;
    deck.legend.is_none()
        && deck.chosen_champion.is_none()
        && deck.main_deck.is_empty()
        && deck.runes.is_empty()
        && deck.battlefields.is_empty()
        && deck.sideboard.is_empty()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedQuery {
    pub resolution: Resolution,
    pub source_kind: &'static str,
    pub source_value: String,
    pub title: Option<String>,
}

fn resolve_parsed(
    parsed: &ParsedDeck,
    cards: &mut dyn CardLookup<Riftbound>,
    source_kind: &'static str,
    source_value: &str,
    title: Option<String>,
) -> Result<ResolvedQuery, QueryReply> {
    let resolution = resolve(parsed, cards).map_err(|error| error_reply(502, error))?;
    if empty(&resolution) {
        if let Some(failure) = resolution.first_failure() {
            return Err(error_reply(
                502,
                format!(
                    "no cards resolved; {}: {}",
                    failure.identifier, failure.reason
                ),
            ));
        }
        return Err(error_reply(
            422,
            "no cards resolved; ingest the riftbound catalog with ingest-riftbound first",
        ));
    }
    Ok(ResolvedQuery {
        resolution,
        source_kind,
        source_value: source_value.to_string(),
        title,
    })
}

pub fn reply_for(resolved: &ResolvedQuery) -> QueryReply {
    let code = deck_code_for(&resolved.resolution.deck);
    let mut body = deck_json(
        &resolved.resolution,
        resolved.source_kind,
        &resolved.source_value,
        code.as_deref().ok(),
    );
    if let Err(reason) = &code {
        body["code_unavailable"] = json!(reason);
    }
    if let Some(title) = &resolved.title {
        body["title"] = json!(title);
    }
    QueryReply { status: 200, body }
}

pub fn resolve_query_deck(
    query: &DeckQuery,
    fetch: &mut dyn Fetch,
    cards: &mut dyn CardLookup<Riftbound>,
) -> Result<ResolvedQuery, QueryReply> {
    match query {
        DeckQuery::Url(url) => {
            let Some(site) = link::classify(url) else {
                return Err(error_reply(
                    403,
                    format!(
                        "deck links are only fetched from {}",
                        link::ALLOWED_HOSTS.join(", ")
                    ),
                ));
            };
            if site == link::Site::TcgArena {
                let imported =
                    super::tcg_arena::parse_url(url).map_err(|error| error_reply(422, error))?;
                return resolve_parsed(&imported.deck, cards, "url", url, imported.title);
            }
            if let Some(code) = link::code_in_url(url) {
                let parsed =
                    parse_deck(&DeckSource::Code(code)).map_err(|error| error_reply(422, error))?;
                return resolve_parsed(&parsed, cards, "url", url, None);
            }
            let body = fetch.get(url).map_err(|error| error_reply(502, error))?;
            let extracted = link::extract(site, &body).map_err(|error| error_reply(422, error))?;
            let title = link::title(&body);
            let parsed = match extracted {
                link::Extracted::Code(code) => {
                    parse_deck(&DeckSource::Code(code)).map_err(|error| error_reply(422, error))?
                }
                link::Extracted::Deck(parsed) => parsed,
            };
            resolve_parsed(&parsed, cards, "url", url, title)
        }
        DeckQuery::Code(code) => {
            let parsed = parse_deck(&DeckSource::Code(code.clone()))
                .map_err(|error| error_reply(422, error))?;
            resolve_parsed(&parsed, cards, "code", code.trim(), None)
        }
        DeckQuery::Text(text) => {
            let (parsed, title) =
                super::parse_any_with_title(text).map_err(|error| error_reply(422, error))?;
            resolve_parsed(&parsed, cards, "text", text, title)
        }
    }
}

pub fn resolve_query(
    query: &DeckQuery,
    fetch: &mut dyn Fetch,
    cards: &mut dyn CardLookup<Riftbound>,
) -> QueryReply {
    match resolve_query_deck(query, fetch, cards) {
        Ok(resolved) => reply_for(&resolved),
        Err(reply) => reply,
    }
}

#[cfg(test)]
mod tests {
    use super::super::card_code::CardCode;
    use super::super::catalog::{test_catalog, CardKind, CatalogCard, StaticCatalog};
    use super::super::deck_code::{self, encode, CodeEntry, DecodedDeck};
    use super::super::Identifier;
    use super::*;
    use crate::deck::LookupError;
    use std::collections::BTreeMap;

    struct Flaky {
        inner: StaticCatalog,
        broken: Vec<Identifier>,
    }

    impl CardLookup<Riftbound> for Flaky {
        fn find(&mut self, identifier: &Identifier) -> Result<Option<CatalogCard>, LookupError> {
            if self.broken.contains(identifier) {
                return Err(LookupError(format!(
                    "https://api.riftcodex.test/cards/riftbound/{identifier}: the fetch failed: timeout (after 3 attempts)"
                )));
            }
            self.inner.find(identifier)
        }
    }

    struct FixtureFetch {
        pages: BTreeMap<String, String>,
        requests: Vec<String>,
    }

    impl Fetch for FixtureFetch {
        fn get(&mut self, url: &str) -> Result<String, String> {
            self.requests.push(url.to_string());
            self.pages
                .get(url)
                .cloned()
                .ok_or_else(|| "the fetch failed: no route".to_string())
        }
    }

    fn fixture(url: &str, body: &str) -> FixtureFetch {
        let mut pages = BTreeMap::new();
        pages.insert(url.to_string(), body.to_string());
        FixtureFetch {
            pages,
            requests: Vec::new(),
        }
    }

    fn sample_code() -> String {
        encode(&DecodedDeck {
            main: vec![
                CodeEntry {
                    code: CardCode::parse("OGN-201").unwrap(),
                    count: 1,
                },
                CodeEntry {
                    code: CardCode::parse("OGN-007").unwrap(),
                    count: 3,
                },
                CodeEntry {
                    code: CardCode::parse("OGN-042").unwrap(),
                    count: 12,
                },
                CodeEntry {
                    code: CardCode::parse("OGN-260").unwrap(),
                    count: 1,
                },
            ],
            sideboard: vec![CodeEntry {
                code: CardCode::parse("OGN-088").unwrap(),
                count: 2,
            }],
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        })
        .unwrap()
    }

    #[test]
    fn a_code_query_returns_the_normalized_deck() {
        let mut fetch = fixture("unused", "");
        let reply = resolve_query(
            &DeckQuery::Code(sample_code()),
            &mut fetch,
            &mut test_catalog(),
        );
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["deck"]["legend"]["name"], "Vanguard Sentinel");
        assert_eq!(
            reply.body["deck"]["chosen_champion"]["riftbound_id"],
            "ogn-007-298"
        );
        assert_eq!(reply.body["deck"]["runes"][0]["count"], 12);
        assert!(fetch.requests.is_empty());
    }

    #[test]
    fn a_piltover_url_extracts_the_code_and_matches_the_code_query() {
        let code = sample_code();
        let url = "https://piltoverarchive.com/decks/view/0000-1111";
        let body = format!("<a href=\"/deckbuilder?code={code}\">builder</a>");
        let mut fetch = fixture(url, &body);
        let from_url = resolve_query(&DeckQuery::Url(url.into()), &mut fetch, &mut test_catalog());
        let from_code = resolve_query(
            &DeckQuery::Code(code),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(from_url.status, 200);
        assert_eq!(from_url.body["deck"], from_code.body["deck"]);
        assert_eq!(from_url.body["code"], from_code.body["code"]);
        assert_eq!(fetch.requests, vec![url.to_string()]);
    }

    #[test]
    fn a_riftdecks_url_resolves_ids_and_reports_the_equivalent_code() {
        let url = "https://riftdecks.com/riftbound-metagame/deck-sample-1";
        let body = concat!(
            "<tr class=\"card-list-item\" data-card-type=\"legend\" data-quantity=\"1\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-201-298_full.png\"></tr>",
            "<tr class=\"card-list-item\" data-card-type=\"unit\" data-quantity=\"3\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-007-298_full.png\"></tr>",
            "<tr class=\"card-list-item\" data-card-type=\"runes\" data-quantity=\"12\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-042-298_full.png\"></tr>"
        );
        let mut fetch = fixture(url, body);
        let reply = resolve_query(&DeckQuery::Url(url.into()), &mut fetch, &mut test_catalog());
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["deck"]["legend"]["name"], "Vanguard Sentinel");
        assert_eq!(reply.body["deck"]["runes"][0]["count"], 12);
        let code = reply.body["code"].as_str().unwrap().to_string();
        let round = resolve_query(
            &DeckQuery::Code(code),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(round.body["deck"]["legend"], reply.body["deck"]["legend"]);
        assert_eq!(round.body["deck"]["runes"], reply.body["deck"]["runes"]);
    }

    #[test]
    fn text_queries_sniff_lists_codes_and_code_lists() {
        let text = "Legend\n1 Vanguard Sentinel\nDeck\n3 Emberwing Scout\n";
        let reply = resolve_query(
            &DeckQuery::Text(text.into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["source"]["kind"], "text");
        let as_code = resolve_query(
            &DeckQuery::Text(format!("  {}  ", sample_code())),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(as_code.status, 200);
        assert_eq!(
            as_code.body["deck"]["chosen_champion"]["name"],
            "Emberwing Scout"
        );
        let as_list = resolve_query(
            &DeckQuery::Text("OGN-007-3 OGN-042-12".into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(as_list.status, 200);
        assert_eq!(as_list.body["deck"]["runes"][0]["count"], 12);
    }

    #[test]
    fn tcg_arena_json_resolves_with_title_without_fetching() {
        let json = r#"{"game":"Riftbound","title":"Synthetic Arena","deckList":{"categoriesOrder":["Legend","Units","Runes"],"Legend":[{"count":1,"id":"ogn-201-298"}],"Units":[{"count":3,"id":"ogn-007-298"}],"Runes":[{"count":12,"id":"ogn-042-298"}]}}"#;
        let mut fetch = fixture("unused", "");
        let reply = resolve_query(
            &DeckQuery::Text(json.into()),
            &mut fetch,
            &mut test_catalog(),
        );
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["title"], "Synthetic Arena");
        assert_eq!(reply.body["deck"]["legend"]["name"], "Vanguard Sentinel");
        assert_eq!(reply.body["deck"]["runes"][0]["count"], 12);
        assert!(fetch.requests.is_empty());
        let mut without_game: Value = serde_json::from_str(json).unwrap();
        without_game.as_object_mut().unwrap().remove("game");
        let reply_without_game = resolve_query(
            &DeckQuery::Text(without_game.to_string()),
            &mut fetch,
            &mut test_catalog(),
        );
        assert_eq!(reply_without_game.status, 200);
        assert_eq!(reply_without_game.body["deck"], reply.body["deck"]);
        assert_eq!(reply_without_game.body["title"], reply.body["title"]);
        assert!(fetch.requests.is_empty());
    }

    #[test]
    fn tcg_arena_url_resolves_with_title_without_fetching() {
        let url = "https://tcg-arena.fr/import?game=Riftbound&name=Synthetic%20URL&deck=MSBWYW5ndWFyZCBTZW50aW5lbAozIEVtYmVyd2luZyBTY291dAoxMiBFbWJlciBSdW5lCg==";
        let mut fetch = fixture("unused", "");
        let reply = resolve_query(&DeckQuery::Url(url.into()), &mut fetch, &mut test_catalog());
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["title"], "Synthetic URL");
        assert_eq!(reply.body["deck"]["legend"]["name"], "Vanguard Sentinel");
        assert!(fetch.requests.is_empty());
        let encoded_url = url.replace("==", "%253D%253D");
        for query in [
            DeckQuery::Url(encoded_url.clone()),
            DeckQuery::Text(encoded_url),
        ] {
            let encoded_reply = resolve_query(&query, &mut fetch, &mut test_catalog());
            assert_eq!(encoded_reply.status, 200);
            assert_eq!(encoded_reply.body["deck"], reply.body["deck"]);
            assert_eq!(encoded_reply.body["title"], reply.body["title"]);
        }
        assert!(fetch.requests.is_empty());
    }

    #[test]
    fn off_allowlist_urls_are_refused_without_fetching() {
        let mut fetch = fixture("https://example.com/deck", "whatever");
        let reply = resolve_query(
            &DeckQuery::Url("https://example.com/deck".into()),
            &mut fetch,
            &mut test_catalog(),
        );
        assert_eq!(reply.status, 403);
        assert!(fetch.requests.is_empty());
    }

    #[test]
    fn failures_map_to_distinct_statuses() {
        let unreachable = resolve_query(
            &DeckQuery::Url("https://riftmana.com/decks/".into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(unreachable.status, 502);
        let mut fetch = fixture("https://riftmana.com/decks/", "<html>no code</html>");
        let no_deck = resolve_query(
            &DeckQuery::Url("https://riftmana.com/decks/".into()),
            &mut fetch,
            &mut test_catalog(),
        );
        assert_eq!(no_deck.status, 422);
        let bad_code = resolve_query(
            &DeckQuery::Code("not!base32".into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(bad_code.status, 422);
        let unresolvable = resolve_query(
            &DeckQuery::Text("3 Completely Unknown Card\n".into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(unresolvable.status, 422);
    }

    #[test]
    fn a_failed_lookup_keeps_the_deck_and_names_the_card_with_its_reason() {
        let url = "https://piltoverarchive.com/decks/view/lillia";
        let code = encode(&DecodedDeck {
            main: vec![
                CodeEntry {
                    code: CardCode::parse("UNL-230").unwrap(),
                    count: 1,
                },
                CodeEntry {
                    code: CardCode::parse("OGN-007").unwrap(),
                    count: 3,
                },
            ],
            sideboard: Vec::new(),
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        })
        .unwrap();
        let body = format!("<a href=\"/deckbuilder?code={code}\">builder</a>");
        let mut flaky = Flaky {
            inner: test_catalog(),
            broken: vec![Identifier::Code(CardCode::parse("UNL-230").unwrap())],
        };
        let reply = resolve_query(
            &DeckQuery::Url(url.into()),
            &mut fixture(url, &body),
            &mut flaky,
        );
        assert_eq!(reply.status, 200);
        assert!(reply.body["deck"]["legend"].is_null());
        assert_eq!(
            reply.body["deck"]["chosen_champion"]["name"],
            "Emberwing Scout"
        );
        assert_eq!(reply.body["unresolved"].as_array().unwrap().len(), 1);
        assert_eq!(reply.body["unresolved"][0]["identifier"], "UNL-230");
        assert_eq!(
            reply.body["unresolved"][0]["reason"],
            "card lookup failed: https://api.riftcodex.test/cards/riftbound/UNL-230: the fetch failed: timeout (after 3 attempts)"
        );
        let healthy = resolve_query(
            &DeckQuery::Url(url.into()),
            &mut fixture(url, &body),
            &mut test_catalog(),
        );
        assert_eq!(healthy.status, 200);
        assert_eq!(
            healthy.body["deck"]["legend"]["name"],
            "Lillia - Bashful Bloom"
        );
        assert_eq!(
            healthy.body["deck"]["legend"]["riftbound_id"],
            "unl-230-219"
        );
        assert!(healthy.body["unresolved"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_deck_that_only_failed_to_look_up_reports_the_failure_not_a_missing_catalog() {
        let mut flaky = Flaky {
            inner: test_catalog(),
            broken: vec![Identifier::Name("Emberwing Scout".into())],
        };
        let reply = resolve_query(
            &DeckQuery::Text("3 Emberwing Scout\n".into()),
            &mut fixture("unused", ""),
            &mut flaky,
        );
        assert_eq!(reply.status, 502);
        let message = reply.body["error"].as_str().unwrap();
        assert!(
            message.starts_with("no cards resolved; Emberwing Scout: card lookup failed:"),
            "{message}"
        );
        assert!(message.contains("after 3 attempts"), "{message}");
    }

    #[test]
    fn the_fetch_identifies_itself_honestly_and_accepts_anything() {
        assert_eq!(FETCH_USER_AGENT, USER_AGENT);
        assert!(FETCH_USER_AGENT.starts_with("agni-importers/"));
        assert!(!FETCH_USER_AGENT.contains("Mozilla"));
        assert_eq!(FETCH_ACCEPT, "*/*");
    }

    #[test]
    #[ignore = "live: riftdecks.com behind Cloudflare; run by hand when the UA policy is in doubt"]
    fn riftdecks_answers_the_honest_user_agent() {
        let url = "https://riftdecks.com/riftbound-metagame/deck-lillia-bashful-bloom-285276";
        let mut fetch = UreqFetch::new();
        let body = fetch.get(url).unwrap();
        let Ok(link::Extracted::Deck(deck)) = link::extract(link::Site::RiftDecks, &body) else {
            panic!("the live page no longer carries card-list-item rows");
        };
        assert!(deck.entries.len() >= 30, "{}", deck.entries.len());
        assert_eq!(
            link::title(&body).as_deref(),
            Some("Lillia, Bashful Bloom by Jonnynick")
        );
    }

    fn opp_catalog() -> StaticCatalog {
        let card = |name: &str, id: &str, kind: CardKind| CatalogCard {
            name: name.into(),
            riftbound_id: id.into(),
            kind,
            ..Default::default()
        };
        StaticCatalog::new(vec![
            card("Lillia - Bashful Bloom", "unl-189-219", CardKind::Legend),
            card("Consult the Past", "opp-083-298", CardKind::Spell),
            card(
                "Master Yi - Wuju Bladesman",
                "opp-019-024",
                CardKind::Legend,
            ),
            card("Calm Rune", "ogn-042-298", CardKind::Rune),
            card("Body Rune", "opp-126b-298", CardKind::Rune),
        ])
    }

    #[test]
    fn a_deck_with_an_opp_print_encodes_to_the_base_prints_code() {
        let text = "Legend\n1 Lillia - Bashful Bloom\nDeck\n1 Consult the Past\n1 opp-083-298\nRunes\n12 Calm Rune\n";
        let reply = resolve_query(
            &DeckQuery::Text(text.into()),
            &mut fixture("unused", ""),
            &mut opp_catalog(),
        );
        assert_eq!(reply.status, 200);
        assert_eq!(
            reply.body["deck"]["main_deck"][0]["riftbound_id"],
            "opp-083-298"
        );
        assert_eq!(reply.body["deck"]["main_deck"][0]["count"], 2);
        let code = reply.body["code"].as_str().expect("a code");
        let decoded = deck_code::decode(code).unwrap();
        let consult = decoded.main.iter().find(|entry| entry.count == 2).unwrap();
        assert_eq!(consult.code.to_string(), "OGN-083");
        assert!(reply.body["code_unavailable"].is_null());
        let expected = encode(&DecodedDeck {
            main: vec![
                CodeEntry {
                    code: CardCode::parse("UNL-189").unwrap(),
                    count: 1,
                },
                CodeEntry {
                    code: CardCode::parse("OGN-083").unwrap(),
                    count: 2,
                },
                CodeEntry {
                    code: CardCode::parse("OGN-042").unwrap(),
                    count: 12,
                },
            ],
            sideboard: Vec::new(),
            champion: None,
        })
        .unwrap();
        assert_eq!(code, expected);
    }

    #[test]
    fn an_opp_print_folds_by_set_alone_and_a_bare_opp_code_yields_no_code_with_a_reason() {
        let reply = resolve_query(
            &DeckQuery::Text("1 Body Rune\n".into()),
            &mut fixture("unused", ""),
            &mut opp_catalog(),
        );
        assert_eq!(reply.status, 200);
        assert_eq!(
            reply.body["deck"]["runes"][0]["riftbound_id"],
            "opp-126b-298"
        );
        let code = reply.body["code"].as_str().expect("a code");
        assert_eq!(
            deck_code::decode(code).unwrap().main[0].code.to_string(),
            "OGN-126b"
        );
        let bare = agni_riftbound::ResolvedDeck {
            legend: Some(agni_riftbound::ResolvedCard {
                name: "Master Yi - Wuju Bladesman".into(),
                riftbound_id: "opp-019".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let error = deck_code_for(&bare).unwrap_err();
        assert!(error.contains("OPP-019"), "{error}");
        assert!(error.contains("no Piltover Archive set number"), "{error}");
    }

    #[test]
    fn a_riftdecks_url_reply_carries_the_page_title() {
        let url = "https://riftdecks.com/riftbound-metagame/deck-sample-2";
        let body = concat!(
            "<html><head><meta name=\"og:title\" content=\"Sample by Nobody\"/></head><body>",
            "<tr class=\"card-list-item\" data-card-type=\"legend\" data-quantity=\"1\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-201-298_full.png\"></tr></body></html>"
        );
        let mut fetch = fixture(url, body);
        let reply = resolve_query(&DeckQuery::Url(url.into()), &mut fetch, &mut test_catalog());
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["title"], "Sample by Nobody");
        let from_code = resolve_query(
            &DeckQuery::Code(sample_code()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert!(from_code.body["title"].is_null());
    }

    #[test]
    fn the_same_input_resolves_the_same_deck_every_time() {
        let text = "Legend\n1 Lillia - Bashful Bloom\nDeck\n3 Emberwing Scout\n1 UNL-230\nRunes\n12 Ember Rune\n";
        let first = resolve_query(
            &DeckQuery::Text(text.into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        let second = resolve_query(
            &DeckQuery::Text(text.into()),
            &mut fixture("unused", ""),
            &mut test_catalog(),
        );
        assert_eq!(first.status, 200);
        assert_eq!(first, second);
        assert_eq!(first.body["deck"]["legend"]["riftbound_id"], "unl-189-219");
        assert_eq!(
            first.body["deck"]["main_deck"][1]["riftbound_id"],
            "unl-230-219"
        );
        assert!(first.body["unresolved"].as_array().unwrap().is_empty());
    }
}
