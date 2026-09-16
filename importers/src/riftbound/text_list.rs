use super::card_code::CardCode;
use super::catalog::CardKind;
use super::{Identifier, ParsedDeck, Riftbound, Section};
use crate::deck::TextVocabulary;
use agni_riftbound::{DeckEntry, ResolvedCard, ResolvedDeck};
use std::fmt::Write;

pub use crate::deck::TextError;

pub const HEADER_LEGEND: &str = "Legend";
pub const HEADER_CHAMPION: &str = "Champion";
pub const HEADER_MAIN: &str = "MainDeck";
pub const HEADER_RUNES: &str = "Runes";
pub const HEADER_BATTLEFIELDS: &str = "Battlefields";
pub const HEADER_SIDEBOARD: &str = "Sideboard";

impl TextVocabulary for Riftbound {
    type Game = Riftbound;

    fn section(header: &str) -> Option<Section> {
        match header {
            "legend" | "legends" => Some(Section::Legend),
            "champion" | "chosen champion" | "champions" => Some(Section::Champion),
            "deck" | "main" | "main deck" | "maindeck" | "mainboard" => Some(Section::Main),
            "rune" | "runes" | "rune deck" => Some(Section::Runes),
            "battlefield" | "battlefields" => Some(Section::Battlefields),
            "side" | "sideboard" => Some(Section::Sideboard),
            _ => None,
        }
    }

    fn identifier(name: &str) -> Identifier {
        let (name, bracketed) = split_bracketed_code(name);
        if let Some(code) = bracketed.and_then(|code| CardCode::parse(code).ok()) {
            return Identifier::Code(code);
        }
        match CardCode::from_riftbound_id(name) {
            Ok(code) => Identifier::Code(code),
            Err(_) => Identifier::Name(name.to_string()),
        }
    }
}

pub fn split_bracketed_code(name: &str) -> (&str, Option<&str>) {
    let trimmed = name.trim_end();
    let Some(open) = trimmed.rfind('[') else {
        return (name, None);
    };
    let Some(inner) = trimmed[open + 1..].strip_suffix(']') else {
        return (name, None);
    };
    let inner = inner.trim();
    if inner.is_empty()
        || !inner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '*')
    {
        return (name, None);
    }
    (trimmed[..open].trim_end(), Some(inner))
}

fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

pub fn parse_text(input: &str) -> Result<ParsedDeck, TextError> {
    if input.lines().any(is_fence) {
        let unfenced: String = input
            .lines()
            .map(|line| if is_fence(line) { "" } else { line })
            .collect::<Vec<_>>()
            .join("\n");
        return crate::deck::parse_text::<Riftbound>(&unfenced);
    }
    crate::deck::parse_text::<Riftbound>(input)
}

fn kind_rank(card: &ResolvedCard) -> u8 {
    match card.kind.as_deref().map(CardKind::parse) {
        Some(CardKind::Unit) => 0,
        Some(CardKind::Spell) => 1,
        Some(CardKind::Gear) => 2,
        Some(CardKind::Legend) => 3,
        Some(CardKind::Rune) => 4,
        Some(CardKind::Battlefield) => 5,
        Some(CardKind::Other) | None => 6,
    }
}

pub fn ordered(entries: &[DeckEntry]) -> Vec<&DeckEntry> {
    let mut sorted: Vec<&DeckEntry> = entries.iter().collect();
    sorted.sort_by(|left, right| {
        kind_rank(&left.card)
            .cmp(&kind_rank(&right.card))
            .then_with(|| {
                left.card
                    .energy
                    .unwrap_or(u8::MAX)
                    .cmp(&right.card.energy.unwrap_or(u8::MAX))
            })
            .then_with(|| {
                left.card
                    .name
                    .to_lowercase()
                    .cmp(&right.card.name.to_lowercase())
            })
            .then_with(|| left.card.riftbound_id.cmp(&right.card.riftbound_id))
    });
    sorted
}

pub fn merged_by_name(entries: &[DeckEntry]) -> Vec<(String, u32)> {
    let mut lines: Vec<(String, u32)> = Vec::new();
    for entry in ordered(entries) {
        match lines.iter_mut().find(|(name, _)| *name == entry.card.name) {
            Some((_, count)) => *count += entry.count,
            None => lines.push((entry.card.name.clone(), entry.count)),
        }
    }
    lines
}

fn section(out: &mut String, header: &str, lines: &[(String, u32)]) {
    if lines.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let _ = writeln!(out, "{header}:");
    for (name, count) in lines {
        let _ = writeln!(out, "{count} {name}");
    }
}

fn single(card: Option<&ResolvedCard>) -> Vec<(String, u32)> {
    card.map(|card| (card.name.clone(), 1))
        .into_iter()
        .collect()
}

pub fn render(deck: &ResolvedDeck) -> String {
    let mut out = String::new();
    section(&mut out, HEADER_LEGEND, &single(deck.legend.as_ref()));
    section(
        &mut out,
        HEADER_CHAMPION,
        &single(deck.chosen_champion.as_ref()),
    );
    section(&mut out, HEADER_MAIN, &merged_by_name(&deck.main_deck));
    section(
        &mut out,
        HEADER_BATTLEFIELDS,
        &merged_by_name(&deck.battlefields),
    );
    section(&mut out, HEADER_RUNES, &merged_by_name(&deck.runes));
    section(&mut out, HEADER_SIDEBOARD, &merged_by_name(&deck.sideboard));
    out
}

pub fn deck_block(markdown: &str) -> Option<&str> {
    let heading = markdown.find("## Deck")?;
    let section = &markdown[heading + "## Deck".len()..];
    let section = match section.find("\n## ") {
        Some(next) => &section[..next],
        None => section,
    };
    let open = section.find("```")?;
    let after_marker = &section[open + 3..];
    let body_start = after_marker.find('\n')?;
    let body = &after_marker[body_start + 1..];
    let close = body.find("```")?;
    Some(body[..close].trim_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::super::ParsedEntry;
    use super::*;

    fn name(entry: &ParsedEntry) -> &str {
        match &entry.identifier {
            Identifier::Name(name) => name,
            other => panic!("expected a name, got {other:?}"),
        }
    }

    #[test]
    fn lines_parse_to_named_identifiers() {
        let deck = parse_text("3 Emberwing Scout\nLone Wanderer\n").unwrap();
        assert_eq!(deck.entries.len(), 2);
        assert_eq!(deck.entries[0].count, 3);
        assert_eq!(name(&deck.entries[0]), "Emberwing Scout");
        assert_eq!(name(&deck.entries[1]), "Lone Wanderer");
    }

    #[test]
    fn section_headers_assign_zones() {
        let text = "Legend\n1 Vanguard Sentinel\nChampion:\n1 Emberwing Scout\nMain Deck (40)\n3 Gloomvale Trickster\nRunes (12)\n12 Ember Rune\nBattlefields\n1 Sunken Causeway\nSideboard\n2 Duskwatch Warden\n";
        let deck = parse_text(text).unwrap();
        let sections: Vec<Option<Section>> =
            deck.entries.iter().map(|entry| entry.section).collect();
        assert_eq!(
            sections,
            vec![
                Some(Section::Legend),
                Some(Section::Champion),
                Some(Section::Main),
                Some(Section::Runes),
                Some(Section::Battlefields),
                Some(Section::Sideboard),
            ]
        );
    }

    #[test]
    fn a_line_that_is_a_card_code_becomes_a_code_not_a_name() {
        let deck = parse_text("1 UNL-230\n1 unl-189-219\n1 Lillia - Bashful Bloom\n").unwrap();
        assert_eq!(
            deck.entries[0].identifier,
            Identifier::Code(CardCode::parse("UNL-230").unwrap())
        );
        assert_eq!(
            deck.entries[1].identifier,
            Identifier::Code(CardCode::parse("UNL-189").unwrap())
        );
        assert_eq!(name(&deck.entries[2]), "Lillia - Bashful Bloom");
    }

    #[test]
    fn a_card_named_like_a_header_still_parses_as_a_card() {
        let deck = parse_text("2 Battlefields of Old\n").unwrap();
        assert_eq!(name(&deck.entries[0]), "Battlefields of Old");
        assert_eq!(deck.entries[0].section, None);
    }

    #[test]
    fn opp_codes_in_a_text_list_become_codes() {
        let deck = parse_text("1 OPP-083\n1 opp-083-298\n1 OPP-SP1\n2 VEN-040-166\n").unwrap();
        let opp = CardCode::parse("OPP-083").unwrap();
        assert_eq!(deck.entries[0].identifier, Identifier::Code(opp));
        assert_eq!(deck.entries[1].identifier, Identifier::Code(opp));
        assert_eq!(
            deck.entries[2].identifier,
            Identifier::Code(CardCode::parse("OPP-SP1").unwrap())
        );
        assert_eq!(
            deck.entries[3].identifier,
            Identifier::Code(CardCode::parse("VEN-040").unwrap())
        );
    }

    #[test]
    fn a_fenced_block_pasted_whole_parses_like_its_body() {
        let body = "Legend\n1 Lillia - Bashful Bloom\nMain Deck (39)\n3 Defy\n";
        let fenced = format!("```\n{body}```\n");
        assert_eq!(parse_text(&fenced).unwrap(), parse_text(body).unwrap());
        let indented = format!("  ```text\n{body}  ```");
        assert_eq!(parse_text(&indented).unwrap(), parse_text(body).unwrap());
        assert_eq!(parse_text("```\n```\n"), Err(TextError::Empty));
    }

    fn pool_files() -> Vec<(String, String)> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../games/riftbound/rules/pool");
        let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .map(|path| {
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read_to_string(&path).unwrap(),
                )
            })
            .collect();
        files.sort();
        assert!(files.len() >= 6, "{}", dir.display());
        files
    }

    fn total(deck: &ParsedDeck, section: Section) -> u32 {
        deck.entries
            .iter()
            .filter(|entry| entry.section == Some(section))
            .map(|entry| entry.count)
            .sum()
    }

    #[test]
    fn every_pool_deck_file_carries_one_deck_block_the_parser_accepts_and_a_set_file_none() {
        let mut decks = 0;
        for (file, markdown) in pool_files() {
            let Some(block) = deck_block(&markdown) else {
                assert!(
                    file == "origins.md"
                        || file == "spiritforged.md"
                        || file == "unleashed.md"
                        || file == "vendetta.md",
                    "{file}: no deck block"
                );
                assert!(
                    markdown.contains("\n## Cards\n"),
                    "{file}: a set file lists its cards"
                );
                continue;
            };
            decks += 1;
            let deck = parse_text(block).unwrap_or_else(|error| panic!("{file}: {error}"));
            assert_eq!(total(&deck, Section::Legend), 1, "{file}");
            assert_eq!(total(&deck, Section::Champion), 1, "{file}");
            assert_eq!(total(&deck, Section::Main), 39, "{file}");
            assert_eq!(total(&deck, Section::Runes), 12, "{file}");
            assert_eq!(total(&deck, Section::Battlefields), 3, "{file}");
            assert_eq!(total(&deck, Section::Sideboard), 0, "{file}");
            assert!(
                deck.entries.iter().all(|entry| entry.section.is_some()),
                "{file}: an entry outside every section"
            );
            assert!(
                deck.entries
                    .iter()
                    .all(|entry| matches!(entry.identifier, Identifier::Name(_))),
                "{file}: the deck block names cards, never codes"
            );
            let whole_section = markdown
                .split("## Deck")
                .nth(1)
                .and_then(|rest| rest.split("\n## ").next())
                .unwrap();
            assert_eq!(parse_text(whole_section).unwrap(), deck, "{file}");
        }
        assert_eq!(decks, 6);
    }

    fn lillia() -> agni_riftbound::ResolvedDeck {
        let (_, deck) = super::super::resolve::fixtures::pool_decks()
            .into_iter()
            .find(|(slug, _)| slug == "lillia-jonnynick")
            .expect("the lillia pool deck");
        super::super::resolve::fixtures::with_sideboard(&deck)
    }

    #[test]
    fn the_lillia_list_renders_in_section_order_sorted_by_kind_energy_and_name() {
        let text = render(&lillia());
        assert_eq!(
            text,
            "Legend:\n1 Lillia - Bashful Bloom\n\nChampion:\n1 Lillia - Fae Fawn\n\nMainDeck:\n3 Lonely Poro\n3 Ravenbloom Student\n1 Disarming Rake\n1 Janna - Savior\n2 Charm\n3 Defy\n3 En Garde\n3 Stupefy\n3 Discipline\n2 Smoke and Mirrors\n2 Smoke Screen\n3 Sprite Burst\n1 Singularity\n1 Unchecked Power\n2 Brutalizer\n3 Mask of Foresight\n3 Sprite Fountain\n\nBattlefields:\n1 Dusk Rose Lab\n1 Ravenbloom Conservatory\n1 Seat of Power\n\nRunes:\n7 Calm Rune\n5 Mind Rune\n\nSideboard:\n2 Disarming Rake\n1 Pickpocket\n1 Thousand-Tailed Watcher\n2 Decree of Focus\n2 Decree of Insight\n1 Smoke and Mirrors\n1 Unchecked Power\n",
            "the community format Rift Atlas and Piltover Archive import: colon headers, a blank line between sections"
        );
    }

    #[test]
    fn an_empty_deck_renders_nothing_and_a_bare_legend_renders_one_section() {
        assert_eq!(render(&agni_riftbound::ResolvedDeck::default()), "");
        let deck = agni_riftbound::ResolvedDeck {
            legend: Some(agni_riftbound::ResolvedCard {
                name: "Vanguard Sentinel".into(),
                riftbound_id: "ogn-201-298".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(render(&deck), "Legend:\n1 Vanguard Sentinel\n");
    }

    #[test]
    fn two_prints_of_one_name_render_as_one_line() {
        let card = |id: &str| agni_riftbound::ResolvedCard {
            name: "Poppy - Paragon".into(),
            riftbound_id: id.into(),
            kind: Some("Unit".into()),
            energy: Some(5),
            ..Default::default()
        };
        let deck = agni_riftbound::ResolvedDeck {
            main_deck: vec![
                agni_riftbound::DeckEntry {
                    card: card("unl-116-219"),
                    count: 2,
                },
                agni_riftbound::DeckEntry {
                    card: card("unl-116a-219"),
                    count: 1,
                },
            ],
            ..Default::default()
        };
        assert_eq!(render(&deck), "MainDeck:\n3 Poppy - Paragon\n");
    }

    #[test]
    fn every_pool_deck_round_trips_through_its_text_list_up_to_print() {
        use super::super::resolve::fixtures::{folded_catalog, pool_decks, with_sideboard};
        use super::super::snapshot::snapshot;
        let mut catalog = folded_catalog();
        let mut decks = pool_decks();
        let lillia = decks
            .iter()
            .find(|(slug, _)| slug == "lillia-jonnynick")
            .map(|(_, deck)| with_sideboard(deck))
            .unwrap();
        decks.push(("lillia-jonnynick+side".into(), lillia));
        for (slug, deck) in decks {
            let text = render(&deck);
            let parsed = parse_text(&text).unwrap_or_else(|error| panic!("{slug}: {error}"));
            let resolution = super::super::resolve::resolve(&parsed, &mut catalog).unwrap();
            assert!(
                resolution.unresolved.is_empty(),
                "{slug}: {:?}",
                resolution.unresolved
            );
            assert_eq!(
                snapshot(&resolution.deck).identity(),
                snapshot(&deck).identity(),
                "{slug}"
            );
            assert_eq!(render(&resolution.deck), text, "{slug}");
            let mut mine = deck.names();
            let mut theirs = resolution.deck.names();
            for names in [&mut mine, &mut theirs] {
                names.main_deck.sort();
                names.runes.sort();
                names.battlefields.sort();
                names.sideboard.sort();
            }
            assert_eq!(theirs, mine, "{slug}");
        }
    }

    #[test]
    fn the_deck_block_reader_stops_at_the_next_heading() {
        let markdown = "# Deck\n\n## Deck\n\n```\nLegend\n1 Vanguard Sentinel\n```\n\n## Cards\n\n- **Vanguard Sentinel** (ogn-201-298; Legend; Fury; ): text\n";
        assert_eq!(deck_block(markdown), Some("Legend\n1 Vanguard Sentinel"));
        assert_eq!(deck_block("# Deck\n\n## Cards\n\n```\n1 x\n```\n"), None);
        assert_eq!(deck_block("## Deck\n\n```\n1 x\n"), None);
    }
}
