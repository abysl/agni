use super::card_code::{CardCode, CardNumber, SetCode, Variant};
use agni_riftbound::ResolvedDeck;
use std::collections::BTreeMap;
use std::fmt;

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const FORMAT: u8 = 1;
const MAX_VERSION: u8 = 5;
const MAIN_COUNT_CEILING: u32 = 12;
const SIDE_COUNT_CEILING: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CodeEntry {
    pub code: CardCode,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecodedDeck {
    pub main: Vec<CodeEntry>,
    pub sideboard: Vec<CodeEntry>,
    pub champion: Option<CardCode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckCodeError {
    BadCharacter(char),
    Truncated,
    BadFormat(u8),
    BadVersion(u8),
    BadSet(u8),
    BadVariant(u8),
    BadPrefixFlag(u8),
    BadDeckFlag(u8),
    BadCount(String, u32),
    NeedsNewerVersion(u8, String),
    Oversized,
}

impl fmt::Display for DeckCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadCharacter(c) => write!(f, "deck code contains {c:?}, not base32"),
            Self::Truncated => write!(f, "deck code ends before the deck does"),
            Self::BadFormat(v) => write!(f, "unknown deck code format {v}"),
            Self::BadVersion(v) => {
                write!(f, "deck code version {v} is newer than v{MAX_VERSION}")
            }
            Self::BadSet(v) => write!(f, "unknown set id {v} in deck code"),
            Self::BadVariant(v) => write!(f, "unknown variant id {v} in deck code"),
            Self::BadPrefixFlag(v) => write!(f, "unknown card number prefix flag {v}"),
            Self::BadDeckFlag(v) => write!(f, "unknown deck level prefix flag {v}"),
            Self::BadCount(code, count) => {
                write!(f, "{code} has count {count}, expected at least 1")
            }
            Self::NeedsNewerVersion(v, why) => {
                write!(f, "deck cannot encode as v{v}: {why}")
            }
            Self::Oversized => write!(f, "deck code describes an implausibly large deck"),
        }
    }
}

impl std::error::Error for DeckCodeError {}

fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32_ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        buffer <<= 5 - bits;
        out.push(BASE32_ALPHABET[(buffer & 0x1f) as usize] as char);
    }
    out
}

fn base32_decode(text: &str) -> Result<Vec<u8>, DeckCodeError> {
    let mut bytes = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for c in text.chars() {
        let upper = c.to_ascii_uppercase();
        let value = BASE32_ALPHABET
            .iter()
            .position(|&a| a as char == upper)
            .ok_or(DeckCodeError::BadCharacter(c))? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn byte(&mut self) -> Result<u8, DeckCodeError> {
        let value = *self.bytes.get(self.pos).ok_or(DeckCodeError::Truncated)?;
        self.pos += 1;
        Ok(value)
    }

    fn varint(&mut self) -> Result<u32, DeckCodeError> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.byte()?;
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return u32::try_from(result).map_err(|_| DeckCodeError::Oversized);
            }
            shift += 7;
            if shift > 28 {
                return Err(DeckCodeError::Oversized);
            }
        }
    }
}

fn push_varint(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn read_number(reader: &mut Reader, flagged: bool) -> Result<CardNumber, DeckCodeError> {
    if !flagged {
        return Ok(CardNumber::Normal(reader.varint()?));
    }
    let flag = reader.byte()?;
    let value = reader.varint()?;
    match flag {
        0x00 => Ok(CardNumber::Normal(value)),
        0x01 => Ok(CardNumber::Rune(value)),
        0x02 => Ok(CardNumber::Special(value)),
        other => Err(DeckCodeError::BadPrefixFlag(other)),
    }
}

fn push_number(bytes: &mut Vec<u8>, number: CardNumber, flagged: bool) {
    if flagged {
        let (flag, value) = match number {
            CardNumber::Normal(value) => (0x00, value),
            CardNumber::Rune(value) => (0x01, value),
            CardNumber::Special(value) => (0x02, value),
        };
        bytes.push(flag);
        push_varint(bytes, value);
    } else {
        let CardNumber::Normal(value) = number else {
            unreachable!()
        };
        push_varint(bytes, value);
    }
}

fn read_group_header(reader: &mut Reader) -> Result<(u32, SetCode, Variant), DeckCodeError> {
    let cards = reader.varint()?;
    if cards > 10_000 {
        return Err(DeckCodeError::Oversized);
    }
    let set_byte = reader.byte()?;
    let set = SetCode::from_wire(set_byte).ok_or(DeckCodeError::BadSet(set_byte))?;
    let variant_byte = reader.byte()?;
    let variant =
        Variant::from_wire(variant_byte).ok_or(DeckCodeError::BadVariant(variant_byte))?;
    Ok((cards, set, variant))
}

fn read_fixed_section(
    reader: &mut Reader,
    ceiling: u32,
    flagged: bool,
) -> Result<Vec<CodeEntry>, DeckCodeError> {
    let mut entries = Vec::new();
    for count in (1..=ceiling).rev() {
        let groups = reader.varint()?;
        if groups > 10_000 {
            return Err(DeckCodeError::Oversized);
        }
        for _ in 0..groups {
            let (cards, set, variant) = read_group_header(reader)?;
            for _ in 0..cards {
                let number = read_number(reader, flagged)?;
                entries.push(CodeEntry {
                    code: CardCode {
                        set,
                        number,
                        variant,
                    },
                    count,
                });
            }
        }
    }
    Ok(entries)
}

fn read_sparse_section(
    reader: &mut Reader,
    flagged: bool,
) -> Result<Vec<CodeEntry>, DeckCodeError> {
    let mut entries = Vec::new();
    let counts = reader.varint()?;
    if counts > 10_000 {
        return Err(DeckCodeError::Oversized);
    }
    for _ in 0..counts {
        let count = reader.varint()?;
        let groups = reader.varint()?;
        if groups > 10_000 {
            return Err(DeckCodeError::Oversized);
        }
        for _ in 0..groups {
            let (cards, set, variant) = read_group_header(reader)?;
            for _ in 0..cards {
                let number = read_number(reader, flagged)?;
                entries.push(CodeEntry {
                    code: CardCode {
                        set,
                        number,
                        variant,
                    },
                    count,
                });
            }
        }
    }
    Ok(entries)
}

pub fn decode(code: &str) -> Result<DecodedDeck, DeckCodeError> {
    let bytes = base32_decode(code.trim())?;
    let mut reader = Reader::new(&bytes);
    let header = reader.byte()?;
    let format = header >> 4;
    let version = header & 0x0f;
    if format != FORMAT {
        return Err(DeckCodeError::BadFormat(format));
    }
    if version == 0 || version > MAX_VERSION {
        return Err(DeckCodeError::BadVersion(version));
    }
    let flagged = if version >= 5 {
        match reader.byte()? {
            0 => false,
            1 => true,
            other => return Err(DeckCodeError::BadDeckFlag(other)),
        }
    } else {
        version >= 4
    };
    let (main, sideboard) = if version >= 5 {
        (
            read_sparse_section(&mut reader, flagged)?,
            read_sparse_section(&mut reader, flagged)?,
        )
    } else {
        let main = read_fixed_section(&mut reader, MAIN_COUNT_CEILING, flagged)?;
        let sideboard = if version >= 2 {
            read_fixed_section(&mut reader, SIDE_COUNT_CEILING, flagged)?
        } else {
            Vec::new()
        };
        (main, sideboard)
    };
    let champion = if version >= 3 {
        match reader.byte()? {
            0x00 => None,
            _ => {
                let set_byte = reader.byte()?;
                let set = SetCode::from_wire(set_byte).ok_or(DeckCodeError::BadSet(set_byte))?;
                let variant_byte = reader.byte()?;
                let variant = Variant::from_wire(variant_byte)
                    .ok_or(DeckCodeError::BadVariant(variant_byte))?;
                let number = read_number(&mut reader, flagged)?;
                Some(CardCode {
                    set,
                    number,
                    variant,
                })
            }
        }
    } else {
        None
    };
    Ok(DecodedDeck {
        main,
        sideboard,
        champion,
    })
}

fn aggregate(entries: &[CodeEntry]) -> Result<BTreeMap<CardCode, u32>, DeckCodeError> {
    let mut totals = BTreeMap::new();
    for entry in entries {
        if entry.count == 0 {
            return Err(DeckCodeError::BadCount(entry.code.to_string(), entry.count));
        }
        *totals.entry(entry.code).or_insert(0) += entry.count;
    }
    Ok(totals)
}

type Group = ((SetCode, Variant), Vec<CardNumber>);

fn groups_for_count(totals: &BTreeMap<CardCode, u32>, count: u32) -> Vec<Group> {
    let mut groups: BTreeMap<(SetCode, Variant), Vec<CardNumber>> = BTreeMap::new();
    for (code, &total) in totals {
        if total == count {
            groups
                .entry((code.set, code.variant))
                .or_default()
                .push(code.number);
        }
    }
    groups
        .into_iter()
        .map(|(key, mut numbers)| {
            numbers.sort();
            (key, numbers)
        })
        .collect()
}

fn push_groups(bytes: &mut Vec<u8>, groups: &[Group], flagged: bool) {
    push_varint(bytes, groups.len() as u32);
    for ((set, variant), numbers) in groups {
        push_varint(bytes, numbers.len() as u32);
        bytes.push(set.to_wire());
        bytes.push(variant.to_wire());
        for &number in numbers {
            push_number(bytes, number, flagged);
        }
    }
}

fn push_fixed_section(
    bytes: &mut Vec<u8>,
    totals: &BTreeMap<CardCode, u32>,
    ceiling: u32,
    flagged: bool,
) {
    for count in (1..=ceiling).rev() {
        let groups = groups_for_count(totals, count);
        push_groups(bytes, &groups, flagged);
    }
}

fn push_sparse_section(bytes: &mut Vec<u8>, totals: &BTreeMap<CardCode, u32>, flagged: bool) {
    let mut counts: Vec<u32> = totals.values().copied().collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    counts.dedup();
    push_varint(bytes, counts.len() as u32);
    for count in counts {
        push_varint(bytes, count);
        let groups = groups_for_count(totals, count);
        push_groups(bytes, &groups, flagged);
    }
}

fn is_prefixed(number: CardNumber) -> bool {
    !matches!(number, CardNumber::Normal(_))
}

fn has_special(totals: &BTreeMap<CardCode, u32>) -> bool {
    totals
        .keys()
        .any(|code| matches!(code.number, CardNumber::Special(_)))
}

fn has_rune(totals: &BTreeMap<CardCode, u32>) -> bool {
    totals
        .keys()
        .any(|code| matches!(code.number, CardNumber::Rune(_)))
}

pub fn encode(deck: &DecodedDeck) -> Result<String, DeckCodeError> {
    let main = aggregate(&deck.main)?;
    let sideboard = aggregate(&deck.sideboard)?;
    let champion_rune = deck
        .champion
        .map(|code| matches!(code.number, CardNumber::Rune(_)))
        .unwrap_or(false);
    let champion_special = deck
        .champion
        .map(|code| matches!(code.number, CardNumber::Special(_)))
        .unwrap_or(false);
    let needs_v4 = has_rune(&main) || has_rune(&sideboard) || champion_rune;
    let any_special = has_special(&main) || has_special(&sideboard) || champion_special;
    let over_ceiling = main.values().any(|&count| count > MAIN_COUNT_CEILING)
        || sideboard.values().any(|&count| count > SIDE_COUNT_CEILING);
    let version = if over_ceiling || any_special {
        5
    } else if needs_v4 {
        4
    } else {
        3
    };
    encode_as(version, deck)
}

pub fn encode_as(version: u8, deck: &DecodedDeck) -> Result<String, DeckCodeError> {
    if version == 0 || version > MAX_VERSION {
        return Err(DeckCodeError::BadVersion(version));
    }
    let main = aggregate(&deck.main)?;
    let sideboard = aggregate(&deck.sideboard)?;
    let refuse = |why: &str| Err(DeckCodeError::NeedsNewerVersion(version, why.into()));
    if version < 2 && !sideboard.is_empty() {
        return refuse("v1 has no sideboard section");
    }
    if version < 3 && deck.champion.is_some() {
        return refuse("chosen champion needs v3");
    }
    let champion_prefixed = deck
        .champion
        .map(|c| is_prefixed(c.number))
        .unwrap_or(false);
    let any_prefixed = main.keys().any(|code| is_prefixed(code.number))
        || sideboard.keys().any(|code| is_prefixed(code.number))
        || champion_prefixed;
    let champion_special = deck
        .champion
        .map(|c| matches!(c.number, CardNumber::Special(_)))
        .unwrap_or(false);
    let any_special = has_special(&main) || has_special(&sideboard) || champion_special;
    if version < 4 && any_prefixed {
        return refuse("R numbered cards need v4 and SP numbered cards need v5");
    }
    if version < 5 && any_special {
        return refuse("SP numbered cards need v5");
    }
    if version < 5
        && (main.values().any(|&count| count > MAIN_COUNT_CEILING)
            || sideboard.values().any(|&count| count > SIDE_COUNT_CEILING))
    {
        return refuse("copy counts past the fixed ceilings need v5");
    }
    let flagged = if version >= 5 {
        any_prefixed
    } else {
        version >= 4
    };
    let mut bytes = vec![(FORMAT << 4) | version];
    if version >= 5 {
        bytes.push(u8::from(flagged));
        push_sparse_section(&mut bytes, &main, flagged);
        push_sparse_section(&mut bytes, &sideboard, flagged);
    } else {
        push_fixed_section(&mut bytes, &main, MAIN_COUNT_CEILING, flagged);
        if version >= 2 {
            push_fixed_section(&mut bytes, &sideboard, SIDE_COUNT_CEILING, flagged);
        }
    }
    if version >= 3 {
        match deck.champion {
            Some(code) => {
                bytes.push(0x01);
                bytes.push(code.set.to_wire());
                bytes.push(code.variant.to_wire());
                push_number(&mut bytes, code.number, flagged);
            }
            None => bytes.push(0x00),
        }
    }
    Ok(base32_encode(&bytes))
}

pub fn encodable(riftbound_id: &str) -> Result<CardCode, String> {
    let code = CardCode::base_print_of_id(riftbound_id).map_err(|error| error.to_string())?;
    if code.set.wire().is_none() {
        return Err(format!(
            "{code} is a {} print with no Piltover Archive set number and no base print to fold to",
            code.set.as_str()
        ));
    }
    Ok(code)
}

fn entry_of(riftbound_id: &str, count: u32) -> Result<CodeEntry, String> {
    Ok(CodeEntry {
        code: encodable(riftbound_id)?,
        count,
    })
}

pub fn encode_deck(deck: &ResolvedDeck) -> Result<String, String> {
    let mut main = Vec::new();
    if let Some(legend) = &deck.legend {
        main.push(entry_of(&legend.riftbound_id, 1)?);
    }
    if let Some(champion) = &deck.chosen_champion {
        main.push(entry_of(&champion.riftbound_id, 1)?);
    }
    for zone in [&deck.main_deck, &deck.runes, &deck.battlefields] {
        for entry in zone {
            main.push(entry_of(&entry.card.riftbound_id, entry.count)?);
        }
    }
    let mut sideboard = Vec::new();
    for entry in &deck.sideboard {
        sideboard.push(entry_of(&entry.card.riftbound_id, entry.count)?);
    }
    let champion = match &deck.chosen_champion {
        Some(card) => Some(encodable(&card.riftbound_id)?),
        None => None,
    };
    encode(&DecodedDeck {
        main,
        sideboard,
        champion,
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_riftbound::{DeckEntry, ResolvedCard};

    fn entry(code: &str, count: u32) -> CodeEntry {
        CodeEntry {
            code: CardCode::parse(code).unwrap(),
            count,
        }
    }

    fn sorted(mut entries: Vec<CodeEntry>) -> Vec<CodeEntry> {
        entries.sort();
        entries
    }

    fn assert_round_trip(version: u8, deck: &DecodedDeck) {
        let code = encode_as(version, deck).unwrap();
        let decoded = decode(&code).unwrap();
        assert_eq!(encode_as(version, &decoded).unwrap(), code, "v{version}");
        assert_eq!(
            sorted(decoded.main),
            sorted(deck.main.clone()),
            "v{version}"
        );
        assert_eq!(
            sorted(decoded.sideboard),
            sorted(deck.sideboard.clone()),
            "v{version}"
        );
        assert_eq!(decoded.champion, deck.champion, "v{version}");
    }

    fn plain_deck() -> DecodedDeck {
        DecodedDeck {
            main: vec![
                entry("OGN-007", 3),
                entry("OGN-101", 3),
                entry("OGN-045", 2),
                entry("SFD-033", 4),
                entry("OGN-042", 12),
                entry("OGN-260", 1),
                entry("UNL-116a", 1),
            ],
            sideboard: Vec::new(),
            champion: None,
        }
    }

    #[test]
    fn v1_round_trips_a_main_deck_only() {
        let deck = DecodedDeck {
            main: plain_deck().main,
            sideboard: Vec::new(),
            champion: None,
        };
        assert_round_trip(1, &deck);
    }

    #[test]
    fn v2_round_trips_a_sideboard() {
        let deck = DecodedDeck {
            main: plain_deck().main,
            sideboard: vec![entry("OGN-088", 3), entry("SFD-100", 1)],
            champion: None,
        };
        assert_round_trip(2, &deck);
    }

    #[test]
    fn v3_round_trips_a_chosen_champion() {
        let deck = DecodedDeck {
            main: plain_deck().main,
            sideboard: vec![entry("UNL-039", 2)],
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        };
        assert_round_trip(3, &deck);
    }

    #[test]
    fn v4_round_trips_rune_numbered_cards() {
        let deck = DecodedDeck {
            main: vec![
                entry("OGN-007", 3),
                entry("VEN-R01", 8),
                entry("VEN-R03", 4),
                entry("OGN-260", 1),
            ],
            sideboard: vec![entry("OGN-088", 3)],
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        };
        assert_round_trip(4, &deck);
    }

    #[test]
    fn v5_round_trips_special_numbers_and_high_counts() {
        let deck = DecodedDeck {
            main: vec![
                entry("UNL-SP3", 1),
                entry("OGN-118", 27),
                entry("VEN-R01", 12),
                entry("OGN-007", 3),
            ],
            sideboard: vec![entry("UNL-SP7", 2), entry("OGN-088", 3)],
            champion: Some(CardCode::parse("UNL-SP3").unwrap()),
        };
        assert_round_trip(5, &deck);
    }

    #[test]
    fn v5_all_normal_decks_skip_the_per_card_flag() {
        let deck = DecodedDeck {
            main: vec![entry("OGN-118", 27), entry("OGN-007", 3)],
            sideboard: Vec::new(),
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        };
        let code = encode_as(5, &deck).unwrap();
        let bytes = base32_decode(&code).unwrap();
        assert_eq!(bytes[1], 0);
        assert_round_trip(5, &deck);
    }

    #[test]
    fn every_variant_survives_every_supporting_version() {
        let deck = DecodedDeck {
            main: vec![
                entry("OGN-007", 2),
                entry("OGN-007a", 2),
                entry("OGN-007s", 2),
                entry("OGN-007b", 2),
            ],
            sideboard: Vec::new(),
            champion: None,
        };
        for version in 1..=5 {
            assert_round_trip(version, &deck);
        }
    }

    #[test]
    fn auto_encode_picks_the_smallest_sufficient_version() {
        let plain = encode(&plain_deck()).unwrap();
        assert_eq!(base32_decode(&plain).unwrap()[0] & 0x0f, 3);
        let runes = DecodedDeck {
            main: vec![entry("VEN-R01", 3)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert_eq!(
            base32_decode(&encode(&runes).unwrap()).unwrap()[0] & 0x0f,
            4
        );
        let special = DecodedDeck {
            main: vec![entry("UNL-SP3", 1)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert_eq!(
            base32_decode(&encode(&special).unwrap()).unwrap()[0] & 0x0f,
            5
        );
        let swarm = DecodedDeck {
            main: vec![entry("OGN-118", 30)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert_eq!(
            base32_decode(&encode(&swarm).unwrap()).unwrap()[0] & 0x0f,
            5
        );
    }

    #[test]
    fn duplicate_entries_aggregate_before_encoding() {
        let deck = DecodedDeck {
            main: vec![entry("OGN-007", 2), entry("OGN-007", 1)],
            sideboard: Vec::new(),
            champion: None,
        };
        let decoded = decode(&encode(&deck).unwrap()).unwrap();
        assert_eq!(decoded.main, vec![entry("OGN-007", 3)]);
    }

    #[test]
    fn encode_refuses_content_its_version_cannot_carry() {
        let side = DecodedDeck {
            main: vec![entry("OGN-007", 1)],
            sideboard: vec![entry("OGN-088", 1)],
            champion: None,
        };
        assert!(matches!(
            encode_as(1, &side),
            Err(DeckCodeError::NeedsNewerVersion(1, _))
        ));
        let champion = DecodedDeck {
            main: vec![entry("OGN-007", 1)],
            sideboard: Vec::new(),
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        };
        assert!(matches!(
            encode_as(2, &champion),
            Err(DeckCodeError::NeedsNewerVersion(2, _))
        ));
        let runes = DecodedDeck {
            main: vec![entry("VEN-R01", 1)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert!(matches!(
            encode_as(3, &runes),
            Err(DeckCodeError::NeedsNewerVersion(3, _))
        ));
        let special = DecodedDeck {
            main: vec![entry("UNL-SP1", 1)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert!(matches!(
            encode_as(4, &special),
            Err(DeckCodeError::NeedsNewerVersion(4, _))
        ));
        let swarm = DecodedDeck {
            main: vec![entry("OGN-118", 13)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert!(matches!(
            encode_as(4, &swarm),
            Err(DeckCodeError::NeedsNewerVersion(4, _))
        ));
        let zero = DecodedDeck {
            main: vec![entry("OGN-118", 0)],
            sideboard: Vec::new(),
            champion: None,
        };
        assert!(matches!(
            encode_as(3, &zero),
            Err(DeckCodeError::BadCount(_, 0))
        ));
    }

    #[test]
    fn decode_refuses_malformed_codes() {
        assert!(matches!(
            decode("CEAA!AAA"),
            Err(DeckCodeError::BadCharacter('!'))
        ));
        assert!(matches!(decode(""), Err(DeckCodeError::Truncated)));
        assert!(matches!(
            decode(&base32_encode(&[0x21])),
            Err(DeckCodeError::BadFormat(2))
        ));
        assert!(matches!(
            decode(&base32_encode(&[0x16])),
            Err(DeckCodeError::BadVersion(6))
        ));
        assert!(matches!(
            decode(&base32_encode(&[0x13, 1])),
            Err(DeckCodeError::Truncated)
        ));
        assert!(matches!(
            decode(&base32_encode(&[0x15, 9])),
            Err(DeckCodeError::BadDeckFlag(9))
        ));
        let bad_set = base32_encode(&[0x11, 1, 1, 9, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(matches!(decode(&bad_set), Err(DeckCodeError::BadSet(9))));
    }

    #[test]
    fn decode_accepts_lowercase_and_surrounding_whitespace() {
        let code = encode(&plain_deck()).unwrap();
        let sloppy = format!("  {}  ", code.to_ascii_lowercase());
        assert_eq!(decode(&sloppy).unwrap(), decode(&code).unwrap());
    }

    fn resolved(name: &str, id: &str, kind: &str) -> ResolvedCard {
        ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            kind: Some(kind.into()),
            ..Default::default()
        }
    }

    #[test]
    fn a_resolved_deck_encodes_with_reprints_folded_and_every_zone_in_main() {
        let deck = ResolvedDeck {
            legend: Some(resolved("Lillia - Bashful Bloom", "unl-189-219", "Legend")),
            chosen_champion: Some(resolved("Lillia - Fae Fawn", "unl-082-219", "Unit")),
            main_deck: vec![
                DeckEntry {
                    card: resolved("Consult the Past", "opp-083-298", "Spell"),
                    count: 2,
                },
                DeckEntry {
                    card: resolved("Poppy - Paragon", "unl-116a-219", "Unit"),
                    count: 3,
                },
            ],
            runes: vec![DeckEntry {
                card: resolved("Calm Rune", "ogn-042-298", "Rune"),
                count: 12,
            }],
            battlefields: vec![DeckEntry {
                card: resolved("Seat of Power", "sfd-217-221", "Battlefield"),
                count: 1,
            }],
            sideboard: vec![DeckEntry {
                card: resolved("Pickpocket", "ogn-100-298", "Unit"),
                count: 1,
            }],
        };
        let code = encode_deck(&deck).unwrap();
        let decoded = decode(&code).unwrap();
        assert_eq!(decoded.champion.unwrap().to_string(), "UNL-082");
        let main: Vec<(String, u32)> = sorted(decoded.main)
            .into_iter()
            .map(|entry| (entry.code.to_string(), entry.count))
            .collect();
        assert_eq!(
            main,
            vec![
                ("OGN-042".to_string(), 12),
                ("OGN-083".to_string(), 2),
                ("SFD-217".to_string(), 1),
                ("UNL-082".to_string(), 1),
                ("UNL-116a".to_string(), 3),
                ("UNL-189".to_string(), 1),
            ],
            "the chosen champion is one of the main-deck cards, as Rift Atlas and Piltover Archive read the code"
        );
        assert_eq!(decoded.sideboard.len(), 1);
        assert_eq!(decoded.sideboard[0].code.to_string(), "OGN-100");
    }

    fn folded_identity(deck: &ResolvedDeck) -> agni_deck::DeckIdentity {
        let mut snapshot = super::super::snapshot::snapshot(deck);
        for zone in snapshot.zones.iter_mut() {
            for card in zone.cards.iter_mut() {
                card.key = CardCode::base_print_of_id(&card.key)
                    .map(|code| code.id_fragment())
                    .unwrap_or_else(|_| card.key.clone());
            }
        }
        snapshot.identity()
    }

    #[test]
    fn every_pool_deck_round_trips_through_its_deck_code_up_to_base_print_folding() {
        use super::super::resolve::fixtures::{folded_catalog, pool_decks, with_sideboard};
        let mut catalog = folded_catalog();
        let mut decks = pool_decks();
        let lillia = decks
            .iter()
            .find(|(slug, _)| slug == "lillia-jonnynick")
            .map(|(_, deck)| with_sideboard(deck))
            .unwrap();
        decks.push(("lillia-jonnynick+side".into(), lillia));
        for (slug, deck) in decks {
            let code = encode_deck(&deck).unwrap_or_else(|error| panic!("{slug}: {error}"));
            let parsed =
                super::super::parse_any(&code).unwrap_or_else(|error| panic!("{slug}: {error}"));
            let resolution = super::super::resolve::resolve(&parsed, &mut catalog).unwrap();
            assert!(
                resolution.unresolved.is_empty(),
                "{slug}: {:?}",
                resolution.unresolved
            );
            assert_eq!(
                folded_identity(&resolution.deck),
                folded_identity(&deck),
                "{slug}"
            );
            assert_eq!(
                resolution
                    .deck
                    .chosen_champion
                    .as_ref()
                    .map(|card| &card.name),
                deck.chosen_champion.as_ref().map(|card| &card.name),
                "{slug}"
            );
            assert_eq!(encode_deck(&resolution.deck).unwrap(), code, "{slug}");
        }
    }

    #[test]
    fn the_lillia_list_with_its_sideboard_encodes_to_the_published_code() {
        use super::super::resolve::fixtures::{pool_decks, with_sideboard};
        let deck = pool_decks()
            .into_iter()
            .find(|(slug, _)| slug == "lillia-jonnynick")
            .map(|(_, deck)| with_sideboard(&deck))
            .unwrap();
        let published = "CMAAAAAAAAAQCAAAFIAACAIAABMQAAYGAAAC2LR2HRPWOAIDAASAEBAAIVHAGAQAAAVV2AIDAAVACBAAKMBQEAAANF5QIAYAEA25OAOZAEBAIAF5AHIQCAACAEBQAIACAUACQPIDAIAAA5D3AEBQASQBAQAFGAIEABJA";
        let decoded = decode(published).unwrap();
        assert_eq!(decoded.main.len(), 23);
        assert_eq!(decoded.sideboard.len(), 7);
        assert_eq!(decoded.champion.unwrap().to_string(), "UNL-082");
        assert!(
            !decoded
                .main
                .iter()
                .any(|entry| entry.code.to_string() == "UNL-082"),
            "riftdecks published the champion outside the main list"
        );
        let ours = decode(&encode_deck(&deck).unwrap()).unwrap();
        assert_eq!(
            ours.main.len(),
            24,
            "ours lists the champion among the main cards"
        );
        assert_eq!(ours.sideboard.len(), 7);
        assert_eq!(ours.champion, decoded.champion);
        let theirs = super::super::parsed_from_decoded(&decoded);
        let ours_parsed = super::super::parsed_from_decoded(&ours);
        let total =
            |parsed: &super::super::ParsedDeck| parsed.entries.iter().map(|e| e.count).sum::<u32>();
        assert_eq!(
            total(&theirs),
            total(&ours_parsed),
            "both readings seat the same forty"
        );
    }

    #[test]
    fn a_print_with_no_wire_set_and_no_base_fold_is_named_in_the_error() {
        let special = ResolvedDeck {
            main_deck: vec![DeckEntry {
                card: resolved("Kai'Sa - Survivor", "ven-sp1-006", "Unit"),
                count: 1,
            }],
            ..Default::default()
        };
        let code = encode_deck(&special).unwrap();
        assert_eq!(decode(&code).unwrap().main[0].code.to_string(), "VEN-SP1");
        let promo = ResolvedDeck {
            main_deck: vec![DeckEntry {
                card: resolved("Jinx - Rebel", "pr-003", "Unit"),
                count: 1,
            }],
            ..Default::default()
        };
        let error = encode_deck(&promo).unwrap_err();
        assert!(error.contains("PR-003"), "{error}");
        assert!(error.contains("no Piltover Archive set number"), "{error}");
        let bare = ResolvedDeck {
            legend: Some(resolved("Master Yi - Wuju Bladesman", "opp-019", "Legend")),
            ..Default::default()
        };
        let error = encode_deck(&bare).unwrap_err();
        assert!(error.contains("OPP-019"), "{error}");
        assert!(encode_deck(&ResolvedDeck::default()).is_ok());
    }
}
