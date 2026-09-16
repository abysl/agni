use super::deck_code;
use super::{Identifier, ParsedDeck, ParsedEntry, Section};
use std::fmt;

pub const ALLOWED_HOSTS: [&str; 4] = [
    "piltoverarchive.com",
    "riftdecks.com",
    "riftmana.com",
    "play.riftatlas.com",
];
pub const RIFT_ATLAS_HUB: &str = "https://play.riftatlas.com/?deckCode=";

pub fn riftatlas_url(code: &str) -> String {
    format!("{RIFT_ATLAS_HUB}{}", code.trim())
}

pub fn code_in_url(url: &str) -> Option<String> {
    let site = classify(url)?;
    let query = url.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    let wanted = match site {
        Site::PiltoverArchive => "code",
        Site::RiftAtlas => "deckCode",
        Site::RiftDecks | Site::RiftMana => return None,
    };
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == wanted)
        .map(|(_, value)| value.replace("%3D", "=").replace("%3d", "="))
        .filter(|value| !value.is_empty() && super::deck_code::decode(value).is_ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    PiltoverArchive,
    RiftDecks,
    RiftMana,
    RiftAtlas,
}

impl Site {
    pub fn label(self) -> &'static str {
        match self {
            Self::PiltoverArchive => "piltoverarchive.com",
            Self::RiftDecks => "riftdecks.com",
            Self::RiftMana => "riftmana.com",
            Self::RiftAtlas => "play.riftatlas.com",
        }
    }
}

pub const PILTOVER_DECKBUILDER: &str = "https://piltoverarchive.com/deckbuilder?code=";

pub fn piltover_url(code: &str) -> String {
    format!("{PILTOVER_DECKBUILDER}{}", code.trim())
}

pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = host.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

pub fn classify(url: &str) -> Option<Site> {
    let host = host_of(url)?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    match host {
        "piltoverarchive.com" => Some(Site::PiltoverArchive),
        "riftdecks.com" => Some(Site::RiftDecks),
        "riftmana.com" => Some(Site::RiftMana),
        "play.riftatlas.com" | "riftatlas.com" => Some(Site::RiftAtlas),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extracted {
    Code(String),
    Deck(ParsedDeck),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractError {
    pub site: Site,
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no deck found in the {} page; paste the deck code or card list instead",
            self.site.label()
        )
    }
}

impl std::error::Error for ExtractError {}

pub fn scan_for_code(body: &str) -> Option<String> {
    let bytes = body.as_bytes();
    let mut start = None;
    let mut candidates = Vec::new();
    for (index, &byte) in bytes.iter().chain(std::iter::once(&b' ')).enumerate() {
        let in_alphabet = byte.is_ascii_uppercase() || (b'2'..=b'7').contains(&byte);
        match (start, in_alphabet) {
            (None, true) => start = Some(index),
            (Some(from), false) => {
                if index - from >= 16 {
                    candidates.push(&body[from..index]);
                }
                start = None;
            }
            _ => {}
        }
    }
    candidates
        .into_iter()
        .find(|candidate| deck_code::decode(candidate).is_ok())
        .map(str::to_string)
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let needle = format!(" {name}={quote}");
        if let Some(at) = tag.find(&needle) {
            let from = at + needle.len();
            let len = tag[from..].find(quote)?;
            return Some(&tag[from..from + len]);
        }
    }
    None
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';').filter(|&end| end <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|number| match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn meta_content<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = body;
    while let Some(at) = rest.find("<meta") {
        let tag = &rest[at..];
        let end = tag.find('>')?;
        let tag = &tag[..end];
        let named = attribute(tag, "property") == Some(key) || attribute(tag, "name") == Some(key);
        if named {
            if let Some(content) = attribute(tag, "content") {
                return Some(content);
            }
        }
        rest = &rest[at + end..];
    }
    None
}

pub fn title(body: &str) -> Option<String> {
    let raw = meta_content(body, "og:title").or_else(|| {
        let open = body.find("<title")?;
        let after = &body[open..];
        let start = after.find('>')? + 1;
        let end = after[start..].find("</title")?;
        Some(&after[start..start + end])
    })?;
    let decoded = decode_entities(raw);
    let collapsed: Vec<&str> = decoded.split_whitespace().collect();
    let text = collapsed.join(" ");
    let text = text
        .rsplit_once(" | ")
        .filter(|(_, site)| site.to_ascii_lowercase().contains("riftdecks"))
        .map(|(head, _)| head.to_string())
        .unwrap_or(text);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn text_between_tags(chunk: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in chunk.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn riftdecks_name(chunk: &str) -> Option<String> {
    let anchor = chunk.find("href=\"/cards/")?;
    let after = &chunk[anchor..];
    let inner = &after[after.find('>')? + 1..];
    let close = inner.find("</a>")?;
    let text = decode_entities(&text_between_tags(&inner[..close]));
    let name: Vec<&str> = text.split_whitespace().collect();
    if name.is_empty() {
        None
    } else {
        Some(name.join(" "))
    }
}

fn image_stem(path: &str) -> Option<&str> {
    let file = path.rsplit('/').next()?;
    let stem = file.split(['_', '.']).next()?;
    if stem.is_empty() {
        None
    } else {
        Some(stem)
    }
}

fn riftdecks_section(card_type: &str) -> Option<Section> {
    match card_type {
        "legend" => Some(Section::Legend),
        "champion" => Some(Section::Champion),
        "runes" | "rune" => Some(Section::Runes),
        "battlefield" | "battlefields" => Some(Section::Battlefields),
        "sideboard" => Some(Section::Sideboard),
        _ => None,
    }
}

fn riftdecks_row(chunk: &str) -> Option<ParsedEntry> {
    let tag = &chunk[..chunk.find('>')?];
    let quantity: u32 = attribute(tag, "data-quantity")?.trim().parse().ok()?;
    if quantity == 0 {
        return None;
    }
    let row_end = chunk.find("</tr>").unwrap_or(chunk.len());
    let identifier = match attribute(tag, "data-image-src").and_then(image_stem) {
        Some(id) => Identifier::Id(id.to_ascii_lowercase()),
        None => Identifier::Name(riftdecks_name(&chunk[..row_end])?),
    };
    Some(ParsedEntry {
        identifier,
        count: quantity,
        section: attribute(tag, "data-card-type")
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .and_then(|kind| riftdecks_section(&kind)),
    })
}

fn extract_riftdecks(body: &str) -> Option<ParsedDeck> {
    let entries: Vec<ParsedEntry> = body
        .split("card-list-item")
        .skip(1)
        .filter_map(riftdecks_row)
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(ParsedDeck { entries })
    }
}

fn extract_piltover(body: &str) -> Option<String> {
    let marker = "/deckbuilder?code=";
    if let Some(at) = body.find(marker) {
        let tail = &body[at + marker.len()..];
        let len = tail
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric())
            .count();
        if len >= 16 && deck_code::decode(&tail[..len]).is_ok() {
            return Some(tail[..len].to_string());
        }
    }
    scan_for_code(body)
}

pub fn extract(site: Site, body: &str) -> Result<Extracted, ExtractError> {
    let error = ExtractError { site };
    match site {
        Site::PiltoverArchive => extract_piltover(body).map(Extracted::Code).ok_or(error),
        Site::RiftDecks => extract_riftdecks(body)
            .map(Extracted::Deck)
            .or_else(|| scan_for_code(body).map(Extracted::Code))
            .ok_or(error),
        Site::RiftMana | Site::RiftAtlas => scan_for_code(body).map(Extracted::Code).ok_or(error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::card_code::CardCode;
    use super::super::deck_code::{encode, CodeEntry, DecodedDeck};
    use super::*;

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
            ],
            sideboard: Vec::new(),
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        })
        .unwrap()
    }

    #[test]
    fn urls_classify_by_host_alone() {
        assert_eq!(
            classify("https://piltoverarchive.com/decks/view/abc"),
            Some(Site::PiltoverArchive)
        );
        assert_eq!(
            classify("https://www.riftdecks.com/riftbound-metagame/deck-x-1"),
            Some(Site::RiftDecks)
        );
        assert_eq!(classify("http://riftmana.com/decks/"), Some(Site::RiftMana));
        assert_eq!(classify("https://example.com/piltoverarchive.com"), None);
        assert_eq!(classify("https://evil.com/?u=riftdecks.com"), None);
        assert_eq!(classify("https://piltoverarchive.com.evil.com/x"), None);
        assert_eq!(classify("https://user@evil.com:443/riftmana.com"), None);
    }

    #[test]
    fn the_piltover_link_round_trips_through_the_page_extractor() {
        let code = sample_code();
        let url = piltover_url(&code);
        assert_eq!(
            url,
            format!("https://piltoverarchive.com/deckbuilder?code={code}")
        );
        assert_eq!(classify(&url), Some(Site::PiltoverArchive));
        let page = format!("<html><a href=\"{url}\">open</a></html>");
        assert_eq!(
            extract(Site::PiltoverArchive, &page).unwrap(),
            Extracted::Code(code.clone())
        );
        assert_eq!(
            extract(Site::PiltoverArchive, &url).unwrap(),
            Extracted::Code(code.clone())
        );
        assert_eq!(piltover_url(&format!("  {code}\n")), url);
    }

    #[test]
    fn a_piltover_page_yields_its_deckbuilder_code() {
        let code = sample_code();
        let body = format!(
            "<html><a class=\"btn\" href=\"/deckbuilder?code={code}\">Open in builder</a></html>"
        );
        assert_eq!(
            extract(Site::PiltoverArchive, &body),
            Ok(Extracted::Code(code))
        );
    }

    #[test]
    fn a_riftmana_page_yields_any_embedded_code() {
        let code = sample_code();
        let body = format!("<div>SHOUTING TEXT 22345 <span>{code}</span></div>");
        assert_eq!(extract(Site::RiftMana, &body), Ok(Extracted::Code(code)));
    }

    #[test]
    fn pages_without_a_deck_explain_the_fallback() {
        let error = extract(Site::RiftMana, "<html>nothing here</html>").unwrap_err();
        assert!(error.to_string().contains("paste the deck code"));
    }

    const LILLIA_PAGE: &str = include_str!("testdata/riftdecks-lillia-285276.html");

    #[test]
    fn the_saved_riftdecks_page_parses_to_the_lillia_list() {
        use super::super::resolve::{fixtures, fixtures::zone_names, resolve};
        use super::super::text_list::parse_text;
        let Ok(Extracted::Deck(page)) = extract(Site::RiftDecks, LILLIA_PAGE) else {
            panic!("expected a deck");
        };
        assert_eq!(page.entries.len(), 31);
        assert_eq!(
            page.entries[0].identifier,
            Identifier::Id("unl-230-219".into())
        );
        assert_eq!(page.entries[0].section, Some(Section::Legend));
        assert_eq!(page.entries[1].section, Some(Section::Champion));
        assert_eq!(
            page.entries[1].identifier,
            Identifier::Id("unl-082a-219".into())
        );
        assert_eq!(page.entries[2].section, None);
        let total = |section: Option<Section>| -> u32 {
            page.entries
                .iter()
                .filter(|entry| entry.section == section)
                .map(|entry| entry.count)
                .sum()
        };
        assert_eq!(total(None), 39);
        assert_eq!(total(Some(Section::Runes)), 12);
        assert_eq!(total(Some(Section::Battlefields)), 3);
        assert_eq!(total(Some(Section::Sideboard)), 10);
        let mut catalog = fixtures::raw_catalog();
        let from_page = resolve(&page, &mut catalog).unwrap();
        assert!(
            from_page.unresolved.is_empty(),
            "{:?}",
            from_page.unresolved
        );
        let list = fixtures::DECK_LISTS
            .iter()
            .find(|(name, _)| *name == "lillia")
            .map(|(_, text)| parse_text(text).unwrap())
            .unwrap();
        let from_list = resolve(&list, &mut catalog).unwrap();
        assert!(
            from_list.unresolved.is_empty(),
            "{:?}",
            from_list.unresolved
        );
        let sorted = |deck| {
            zone_names(deck)
                .into_iter()
                .map(|(zone, mut names)| {
                    names.sort();
                    (zone, names)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(sorted(&from_page.deck), sorted(&from_list.deck));
        assert_eq!(
            from_page.deck.legend.unwrap().name,
            "Lillia - Bashful Bloom"
        );
        assert_eq!(
            from_page.deck.chosen_champion.unwrap().name,
            "Lillia - Fae Fawn"
        );
    }

    #[test]
    fn the_saved_riftdecks_page_carries_its_title() {
        assert_eq!(
            title(LILLIA_PAGE).as_deref(),
            Some("Lillia, Bashful Bloom by Jonnynick")
        );
        assert_eq!(
            title("<html><head><title>  8/14/2026 nasus by ThunderTrees | riftDecks.com</title></head></html>")
                .as_deref(),
            Some("8/14/2026 nasus by ThunderTrees")
        );
        assert_eq!(
            title(
                "<meta property=\"og:title\" content=\"Tom &amp; Jerry&#39;s &quot;deck&quot;\">"
            )
            .as_deref(),
            Some("Tom & Jerry's \"deck\"")
        );
        assert_eq!(title("<html><body>nothing</body></html>"), None);
        assert_eq!(title("<meta name=\"og:title\" content=\"\">"), None);
    }

    #[test]
    fn a_riftdecks_row_without_an_image_falls_back_to_the_linked_name() {
        let body = concat!(
            "<tr class=\"card-list-item\" data-card-type='unit' data-quantity='2'>",
            "<td><a href=\"/cards/details-lillia-fae-fawn\">\n  Lillia, Fae Fawn\n</a></td></tr>",
            "<tr class=\"card-list-item\" data-quantity=\"0\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-007-298_full.png\"></tr>",
            "<tr class=\"card-list-item\" data-quantity=\"1\" data-card-type=\"Runes\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/OGN-042-298_full.png\"></tr>"
        );
        let Ok(Extracted::Deck(deck)) = extract(Site::RiftDecks, body) else {
            panic!("expected a deck");
        };
        assert_eq!(deck.entries.len(), 2);
        assert_eq!(
            deck.entries[0].identifier,
            Identifier::Name("Lillia, Fae Fawn".into())
        );
        assert_eq!(deck.entries[0].count, 2);
        assert_eq!(
            deck.entries[1].identifier,
            Identifier::Id("ogn-042-298".into())
        );
        assert_eq!(deck.entries[1].section, Some(Section::Runes));
    }

    #[test]
    fn a_riftdecks_page_yields_ids_counts_and_zones() {
        let body = concat!(
            "<table><tr class=\"card-list-item\" data-card-type=\"legend\" ",
            "data-orientation=\"portrait\" data-quantity=\"1\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-201-298_full.png\">",
            "<td>Vanguard Sentinel</td></tr>",
            "<tr class=\"card-list-item\" data-card-type=\"unit\" ",
            "data-quantity=\"3\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-007-298_full.png\"></tr>",
            "<tr class=\"card-list-item\" data-card-type=\"runes\" ",
            "data-quantity=\"12\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-042-298_full.png\"></tr>",
            "<tr class=\"card-list-item\" data-card-type=\"sideboard\" ",
            "data-quantity=\"2\" ",
            "data-image-src=\"/img/cards/riftbound/OGN/ogn-088-298_full.png\"></tr>",
            "</table>"
        );
        let Ok(Extracted::Deck(deck)) = extract(Site::RiftDecks, body) else {
            panic!("expected a deck");
        };
        assert_eq!(deck.entries.len(), 4);
        assert_eq!(
            deck.entries[0].identifier,
            Identifier::Id("ogn-201-298".into())
        );
        assert_eq!(deck.entries[0].section, Some(Section::Legend));
        assert_eq!(deck.entries[1].section, None);
        assert_eq!(deck.entries[2].section, Some(Section::Runes));
        assert_eq!(deck.entries[2].count, 12);
        assert_eq!(deck.entries[3].section, Some(Section::Sideboard));
    }
}
