use super::{Identifier, ParsedDeck, ParsedEntry, Section};
use serde_json::Value;
use std::fmt;

pub const HOST: &str = "tcg-arena.fr";
pub const MAX_INPUT_BYTES: usize = 1_048_576;
pub const MAX_ENTRIES: usize = 512;
pub const MAX_CARD_COUNT: u32 = 4096;
pub const MAX_TOTAL_CARDS: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub deck: ParsedDeck,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooLarge,
    InvalidJson(String),
    InvalidExport(String),
    InvalidUrl(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the TCG Arena deck export is empty"),
            Self::TooLarge => f.write_str("the TCG Arena deck export is too large"),
            Self::InvalidJson(reason) => write!(f, "the TCG Arena deck JSON is invalid: {reason}"),
            Self::InvalidExport(reason) => {
                write!(f, "the TCG Arena deck export is invalid: {reason}")
            }
            Self::InvalidUrl(reason) => write!(f, "the TCG Arena deck URL is invalid: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

fn size_check(input: &str) -> Result<(), Error> {
    if input.is_empty() {
        return Err(Error::Empty);
    }
    if input.len() > MAX_INPUT_BYTES {
        return Err(Error::TooLarge);
    }
    Ok(())
}

fn section(category: &str) -> Section {
    match category
        .to_ascii_lowercase()
        .replace([' ', '-'], "")
        .as_str()
    {
        "legend" => Section::Legend,
        "chosen_champion" | "chosenchampion" | "champion" => Section::Champion,
        "battlefields" | "battlefield" => Section::Battlefields,
        "runes" | "rune" => Section::Runes,
        "sideboard" | "side" => Section::Sideboard,
        _ => Section::Main,
    }
}

fn count(value: &Value) -> Result<u32, Error> {
    let count = value
        .as_u64()
        .ok_or_else(|| Error::InvalidExport("card count is not an integer".into()))?;
    u32::try_from(count)
        .ok()
        .filter(|count| (1..=MAX_CARD_COUNT).contains(count))
        .ok_or_else(|| Error::InvalidExport("card count must be between 1 and 4096".into()))
}

fn bounded(deck: ParsedDeck) -> Result<ParsedDeck, Error> {
    if deck.entries.len() > MAX_ENTRIES {
        return Err(Error::TooLarge);
    }
    let total = deck.entries.iter().try_fold(0u64, |total, entry| {
        total
            .checked_add(u64::from(entry.count))
            .filter(|total| *total <= MAX_TOTAL_CARDS)
            .ok_or(Error::TooLarge)
    })?;
    if total == 0 {
        return Err(Error::Empty);
    }
    Ok(deck)
}

fn entry(value: &Value, section: Section) -> Result<ParsedEntry, Error> {
    let object = value
        .as_object()
        .ok_or_else(|| Error::InvalidExport("deck entry is not an object".into()))?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Error::InvalidExport("deck entry has no card id".into()))?;
    let count = count(
        object
            .get("count")
            .ok_or_else(|| Error::InvalidExport("deck entry has no count".into()))?,
    )?;
    Ok(ParsedEntry {
        identifier: Identifier::Id(id.to_string()),
        count,
        section: Some(section),
    })
}

pub fn parse_json(input: &str) -> Result<Import, Error> {
    size_check(input)?;
    let root: Value =
        serde_json::from_str(input).map_err(|error| Error::InvalidJson(error.to_string()))?;
    if root
        .get("game")
        .and_then(Value::as_str)
        .is_none_or(|game| !game.eq_ignore_ascii_case("riftbound"))
    {
        return Err(Error::InvalidExport("game must be Riftbound".into()));
    }
    let deck = root
        .get("deckList")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::InvalidExport("missing deckList".into()))?;
    let order = deck
        .get("categoriesOrder")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::InvalidExport("missing categoriesOrder".into()))?;
    let mut entries = Vec::new();
    let mut categories = Vec::new();
    for category in order {
        let category = category
            .as_str()
            .filter(|category| !category.trim().is_empty())
            .ok_or_else(|| Error::InvalidExport("categoriesOrder contains a non-name".into()))?;
        if categories.contains(&category) {
            return Err(Error::InvalidExport(format!(
                "categoriesOrder repeats {category}"
            )));
        }
        if !deck.get(category).is_some_and(Value::is_array) {
            return Err(Error::InvalidExport(format!(
                "category {category} is missing or not an array"
            )));
        }
        categories.push(category);
    }
    if deck.get("Sideboard").is_some_and(Value::is_array) && !categories.contains(&"Sideboard") {
        categories.push("Sideboard");
    }
    for category in categories {
        let cards = deck
            .get(category)
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidExport(format!("category {category} is missing")))?;
        for card in cards {
            entries.push(entry(card, section(category))?);
        }
    }
    let deck = bounded(ParsedDeck { entries })?;
    Ok(Import {
        deck,
        title: root
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub fn parse_text(input: &str) -> Result<Import, Error> {
    size_check(input)?;
    let deck = super::text_list::parse_text(input)
        .map_err(|error| Error::InvalidExport(error.to_string()))?;
    Ok(Import {
        deck: bounded(deck)?,
        title: None,
    })
}

pub fn parse(input: &str) -> Result<Import, Error> {
    if input.trim_start().starts_with('{') {
        parse_json(input)
    } else {
        parse_text(input)
    }
}

fn percent_decode(value: &str) -> Result<String, Error> {
    let mut bytes = Vec::with_capacity(value.len());
    let raw = value.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' {
            if index + 2 >= raw.len() {
                return Err(Error::InvalidUrl("bad percent escape".into()));
            }
            let hex = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                _ => None,
            };
            let high = hex(raw[index + 1])
                .ok_or_else(|| Error::InvalidUrl("bad percent escape".into()))?;
            let low = hex(raw[index + 2])
                .ok_or_else(|| Error::InvalidUrl("bad percent escape".into()))?;
            bytes.push(high * 16 + low);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidUrl("parameter is not UTF-8".into()))
}

fn base64_decode(input: &str) -> Result<String, Error> {
    if input.is_empty() || input.len() % 4 != 0 {
        return Err(Error::InvalidUrl(
            "deck has malformed base64 padding".into(),
        ));
    }
    let padding = input.bytes().rev().take_while(|byte| *byte == b'=').count();
    if padding > 2 || input[..input.len() - padding].contains('=') {
        return Err(Error::InvalidUrl(
            "deck has malformed base64 padding".into(),
        ));
    }
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input[..input.len() - padding].bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                return Err(Error::InvalidUrl(
                    "deck has malformed base64 padding".into(),
                ))
            }
            _ => return Err(Error::InvalidUrl("deck is not base64".into())),
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
            if output.len() > MAX_INPUT_BYTES {
                return Err(Error::TooLarge);
            }
        }
    }
    if buffer != 0 {
        return Err(Error::InvalidUrl(
            "deck has malformed base64 padding".into(),
        ));
    }
    String::from_utf8(output).map_err(|_| Error::InvalidUrl("deck is not UTF-8".into()))
}

pub fn parse_url(url: &str) -> Result<Import, Error> {
    if url.len() > MAX_INPUT_BYTES * 2 {
        return Err(Error::TooLarge);
    }
    let authority = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split_once('/'))
        .map(|(authority, _)| authority)
        .unwrap_or_default();
    if authority != HOST {
        return Err(Error::InvalidUrl(format!(
            "only https://{HOST} is supported"
        )));
    }
    let path = url
        .split_once("//")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map(|(_, path)| path)
        .unwrap_or_default();
    if !path.starts_with("import?") {
        return Err(Error::InvalidUrl(
            "the URL is not a deck import link".into(),
        ));
    }
    let query = path.strip_prefix("import?").unwrap_or_default();
    let mut game = None;
    let mut title = None;
    let mut deck = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(value)?;
        match key {
            "game" => game = Some(value),
            "name" => title = Some(value),
            "deck" => deck = Some(value),
            _ => {}
        }
    }
    if !game.is_some_and(|game| game.eq_ignore_ascii_case("riftbound")) {
        return Err(Error::InvalidUrl("the URL is for a different game".into()));
    }
    let encoded = deck.ok_or_else(|| Error::InvalidUrl("missing deck parameter".into()))?;
    let mut imported = parse_text(&base64_decode(&encoded)?)?;
    imported.title = title.filter(|title| !title.trim().is_empty());
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "1 Synthetic Legend\n1 Synthetic Champion\n1 Synthetic Battlefield\n12 Synthetic Rune\n3 Synthetic Unit\nSideboard:\n2 Synthetic Side\n";

    #[test]
    fn parses_tcg_arena_text_with_sideboard() {
        let imported = parse_text(TEXT).unwrap();
        assert_eq!(imported.deck.entries.len(), 6);
        assert_eq!(imported.deck.entries[0].section, None);
        assert_eq!(imported.deck.entries[5].section, Some(Section::Sideboard));
    }

    #[test]
    fn parses_starter_deck_json_by_category_and_id() {
        let json = r#"{"title":"Synthetic","game":"Riftbound","deckList":{"categoriesOrder":["Legend","Chosen_Champion","Battlefields","Runes","Units"],"Legend":[{"count":1,"id":"SYN-001"}],"Chosen_Champion":[{"count":1,"id":"SYN-002"}],"Battlefields":[{"count":2,"id":"SYN-003"}],"Runes":[{"count":12,"id":"SYN-004"}],"Units":[{"count":3,"id":"SYN-005"}],"Sideboard":[{"count":2,"id":"SYN-006"}]}}"#;
        let imported = parse_json(json).unwrap();
        assert_eq!(imported.title.as_deref(), Some("Synthetic"));
        assert_eq!(
            imported.deck.entries[0].identifier,
            Identifier::Id("SYN-001".into())
        );
        assert_eq!(imported.deck.entries[1].section, Some(Section::Champion));
        assert_eq!(
            imported.deck.entries[2].section,
            Some(Section::Battlefields)
        );
        assert_eq!(imported.deck.entries[3].section, Some(Section::Runes));
        assert_eq!(imported.deck.entries[5].section, Some(Section::Sideboard));
    }

    #[test]
    fn rejects_json_for_another_game() {
        let json = r#"{"game":"Magic: The Gathering","deckList":{"categoriesOrder":[]}}"#;
        assert!(parse_json(json)
            .unwrap_err()
            .to_string()
            .contains("Riftbound"));
    }

    #[test]
    fn rejects_repeated_or_missing_categories() {
        let repeated =
            r#"{"game":"Riftbound","deckList":{"categoriesOrder":["Units","Units"],"Units":[]}}"#;
        let missing = r#"{"game":"Riftbound","deckList":{"categoriesOrder":["Units"]}}"#;
        assert!(parse_json(repeated).is_err());
        assert!(parse_json(missing).is_err());
    }

    #[test]
    fn rejects_counts_that_could_expand_without_bound() {
        let json = r#"{"game":"Riftbound","deckList":{"categoriesOrder":["Units"],"Units":[{"count":4294967295,"id":"SYN-001"}]}}"#;
        assert!(parse_json(json).is_err());
    }

    #[test]
    fn parses_the_public_tcg_arena_import_url_without_fetching() {
        let url = "https://tcg-arena.fr/import?game=Riftbound&name=Synthetic&deck=MSBTeW50aGV0aWMgTGVnZW5kCg==";
        let imported = parse_url(url).unwrap();
        assert_eq!(imported.title.as_deref(), Some("Synthetic"));
        assert_eq!(imported.deck.entries.len(), 1);
    }

    #[test]
    fn rejects_lookalike_hosts_and_other_paths() {
        assert!(
            parse_url("https://tcg-arena.fr.evil.test/import?game=Riftbound&deck=QQ==").is_err()
        );
        assert!(parse_url("https://tcg-arena.fr/load/QQ==").is_err());
    }

    #[test]
    fn rejects_malformed_base64_padding() {
        for deck in ["QQ=", "QR==", "QQ==x", "Q==="] {
            let url = format!("https://tcg-arena.fr/import?game=Riftbound&deck={deck}");
            assert!(parse_url(&url).is_err(), "{deck}");
        }
    }
}
