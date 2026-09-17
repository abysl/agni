use super::catalog::{Cached, StaticCatalog};
use super::ingest::load_catalog;
use super::query::{resolve_query, DeckQuery, UreqFetch};
use serde_json::json;
use spirit_node::gateway::{ResolveReply, ResolveRequest, Resolver};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The name this resolver registers under, served at `/gateway/resolve/deck`.
pub const RESOLVER_NAME: &str = "deck";

/// Deck sites this resolver will follow a link to.
///
/// This list used to live in `spirit_node::gateway` as `DECK_SITE_ALLOWLIST`,
/// which put three Riftbound URLs inside a library whose own rules say it
/// carries no game knowledge. Fetch policy travels with the thing that knows
/// what a deck is.
pub const DECK_SITE_ALLOWLIST: [&str; 4] = [
    "piltoverarchive.com",
    "riftdecks.com",
    "riftmana.com",
    "tcg-arena.fr",
];

/// Host-exact allowlist check. Matches the real host, so neither a lookalike
/// domain (`piltoverarchive.com.evil.com`) nor a userinfo trick
/// (`https://user@evil.com/?u=riftmana.com`) passes.
pub fn deck_url_allowed(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let Some(host) = rest.split(['/', '?', '#']).next() else {
        return false;
    };
    let Some(host) = host.rsplit('@').next() else {
        return false;
    };
    let host = host
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    DECK_SITE_ALLOWLIST.contains(&host)
}

/// Reads a deck query out of the generic gateway parameters.
fn query_of(request: &ResolveRequest) -> Result<DeckQuery, ResolveReply> {
    if let Some(url) = request.get("url") {
        if !deck_url_allowed(url) {
            return Err(ResolveReply::error(
                403,
                &format!(
                    "deck links are only fetched from {}",
                    DECK_SITE_ALLOWLIST.join(", ")
                ),
            ));
        }
        return Ok(DeckQuery::Url(url.to_string()));
    }
    if let Some(code) = request.get("code") {
        return Ok(DeckQuery::Code(code.to_string()));
    }
    if let Some(text) = request.get("text") {
        return Ok(DeckQuery::Text(text.to_string()));
    }
    Err(ResolveReply::error(
        400,
        "resolve/deck needs a url, code or text parameter",
    ))
}

pub fn deck_resolver(dir: PathBuf) -> Resolver {
    let catalog: Mutex<Option<Cached<StaticCatalog>>> = Mutex::new(None);
    Arc::new(move |request: &ResolveRequest| {
        let query = match query_of(request) {
            Ok(query) => query,
            Err(reply) => return reply,
        };
        let mut slot = catalog.lock().unwrap();
        if slot.is_none() {
            match load_catalog(&dir) {
                Ok(Some(loaded)) => *slot = Some(Cached::new(loaded)),
                Ok(None) => {
                    return ResolveReply::json(
                        503,
                        json!({
                            "error": "this node has no riftbound catalog; run ingest-riftbound against its store"
                        })
                        .to_string(),
                    )
                }
                Err(error) => {
                    return ResolveReply::json(
                        500,
                        json!({ "error": error.to_string() }).to_string(),
                    )
                }
            }
        }
        let cards = slot.as_mut().unwrap();
        let mut fetch = UreqFetch::new();
        let reply = resolve_query(&query, &mut fetch, cards);
        ResolveReply::json(reply.status, reply.body.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::super::card_code::CardCode;
    use super::super::deck_code::{encode, CodeEntry, DecodedDeck};
    use super::super::ingest::{RiftboundCard, RiftboundManifest, REF_NAME};
    use super::*;
    use spirit_core::BlobStore;

    fn request(key: &str, value: &str) -> ResolveRequest {
        let mut params = std::collections::BTreeMap::new();
        params.insert(key.to_string(), value.to_string());
        ResolveRequest {
            name: RESOLVER_NAME.to_string(),
            params,
        }
    }

    #[test]
    fn deck_site_allowlisting_checks_the_real_host() {
        assert!(deck_url_allowed("https://piltoverarchive.com/decks/view/x"));
        assert!(deck_url_allowed("https://www.riftdecks.com/a/b"));
        assert!(deck_url_allowed("http://RIFTMANA.com/decks/"));
        assert!(!deck_url_allowed("https://example.com/piltoverarchive.com"));
        assert!(!deck_url_allowed("https://piltoverarchive.com.evil.com/"));
        assert!(!deck_url_allowed("https://user@evil.com/?u=riftmana.com"));
    }

    #[test]
    fn off_allowlist_urls_are_refused_before_any_fetch() {
        let reply = query_of(&request("url", "https://example.com/deck")).unwrap_err();
        assert_eq!(reply.status, 403);
    }

    #[test]
    fn a_query_needs_one_of_the_three_parameters() {
        let empty = ResolveRequest {
            name: RESOLVER_NAME.to_string(),
            params: Default::default(),
        };
        assert_eq!(query_of(&empty).unwrap_err().status, 400);
        assert_eq!(query_of(&request("other", "x")).unwrap_err().status, 400);
        assert_eq!(
            query_of(&request("code", "CEBAIAAA")).unwrap(),
            DeckQuery::Code("CEBAIAAA".into())
        );
        assert_eq!(
            query_of(&request("text", "3 Emberwing Scout")).unwrap(),
            DeckQuery::Text("3 Emberwing Scout".into())
        );
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agni-riftbound-gateway-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn card(name: &str, id: &str, card_type: &str) -> RiftboundCard {
        RiftboundCard {
            name: name.into(),
            image: String::new(),
            riftbound_id: id.into(),
            card_type: card_type.into(),
            supertype: String::new(),
            rarity: "Common".into(),
            domain: Vec::new(),
            energy: None,
            might: None,
            power: None,
            text: String::new(),
            set_id: "OGN".into(),
            set_label: "Origins".into(),
            collector_number: None,
            clean_name: name.into(),
            image_url: format!("https://img.example/{id}.png"),
            tags: Vec::new(),
        }
    }

    fn seed_store(dir: &PathBuf) {
        let store = BlobStore::open(dir).unwrap();
        let manifest = RiftboundManifest {
            set: REF_NAME.into(),
            cards: vec![
                card("Vanguard Sentinel", "ogn-201-298", "Legend"),
                card("Emberwing Scout", "ogn-007-298", "Unit"),
                card("Ember Rune", "ogn-042-298", "Rune"),
            ],
        };
        let mut encoded = Vec::new();
        ciborium::into_writer(&manifest, &mut encoded).unwrap();
        let hash = store.put(&encoded).unwrap();
        spirit_core::refs::write(&store, REF_NAME, hash).unwrap();
    }

    #[test]
    fn the_resolver_answers_from_the_store_catalog() {
        let dir = scratch("resolve");
        seed_store(&dir);
        let resolver = deck_resolver(dir.clone());
        let code = encode(&DecodedDeck {
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
            ],
            sideboard: Vec::new(),
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        })
        .unwrap();
        let reply = resolver(&request("code", &code));
        assert_eq!(reply.status, 200);
        let value: serde_json::Value = serde_json::from_slice(&reply.body).unwrap();
        assert_eq!(value["deck"]["legend"]["name"], "Vanguard Sentinel");
        assert_eq!(value["deck"]["chosen_champion"]["name"], "Emberwing Scout");
        assert_eq!(value["deck"]["runes"][0]["count"], 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_without_the_catalog_reports_service_unavailable() {
        let dir = scratch("empty");
        let resolver = deck_resolver(dir.clone());
        let reply = resolver(&request("text", "3 Emberwing Scout"));
        assert_eq!(reply.status, 503);
        assert!(String::from_utf8_lossy(&reply.body).contains("ingest-riftbound"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
