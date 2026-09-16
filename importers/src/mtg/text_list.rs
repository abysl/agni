use super::{Mtg, ParsedDeck, Section};
use crate::deck::TextVocabulary;

pub use crate::deck::TextError;

fn strip_set_suffix(name: &str) -> &str {
    let trimmed = name.trim_end();
    let trimmed = trimmed.strip_suffix("*F*").unwrap_or(trimmed).trim_end();
    let Some(open) = trimmed.rfind(" (") else {
        return trimmed;
    };
    let tail = &trimmed[open + 2..];
    let Some(close) = tail.find(')') else {
        return trimmed;
    };
    let set = &tail[..close];
    let after = tail[close + 1..].trim();
    let set_like = (2..=6).contains(&set.len())
        && set
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let collector_like =
        after.is_empty() || after.chars().all(|c| c.is_ascii_alphanumeric() || c == '★');
    if set_like && collector_like {
        trimmed[..open].trim_end()
    } else {
        trimmed
    }
}

impl TextVocabulary for Mtg {
    type Game = Mtg;

    fn section(header: &str) -> Option<Section> {
        match header {
            "commander" | "commanders" => Some(Section::Commander),
            "deck" | "main" | "main deck" | "maindeck" | "mainboard" => Some(Section::Main),
            "side" | "sideboard" | "companion" => Some(Section::Sideboard),
            "about" => Some(Section::Ignored),
            _ => None,
        }
    }

    fn identifier(name: &str) -> String {
        name.to_string()
    }

    fn inline_section(line: &str) -> Option<(&str, Section)> {
        line.strip_prefix("SB:")
            .or_else(|| line.strip_prefix("SB "))
            .map(|rest| (rest, Section::Sideboard))
    }

    fn skip(section: Section) -> bool {
        section == Section::Ignored
    }

    fn clean_name(name: &str) -> &str {
        strip_set_suffix(name)
    }
}

pub fn parse_text(input: &str) -> Result<ParsedDeck, TextError> {
    crate::deck::parse_text::<Mtg>(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_headers_assign_zones() {
        let text = "Commander\n1 Serelith, Tidebound Oracle\nDeck (60)\n4 Thornspire Adept\nSideboard:\n2 Cinderveil Ward\n";
        let deck = parse_text(text).unwrap();
        let sections: Vec<Option<Section>> =
            deck.entries.iter().map(|entry| entry.section).collect();
        assert_eq!(
            sections,
            vec![
                Some(Section::Commander),
                Some(Section::Main),
                Some(Section::Sideboard),
            ]
        );
    }

    #[test]
    fn arena_set_and_collector_suffixes_strip_away() {
        let deck = parse_text(
            "4 Thornspire Adept (ZNR) 194\n1 Serelith, Tidebound Oracle (C21) 42a\n2 Mistfen Causeway (DMU) 245 *F*\n",
        )
        .unwrap();
        assert_eq!(deck.entries[0].identifier, "Thornspire Adept");
        assert_eq!(deck.entries[1].identifier, "Serelith, Tidebound Oracle");
        assert_eq!(deck.entries[2].identifier, "Mistfen Causeway");
    }

    #[test]
    fn parenthetical_card_names_survive() {
        let deck = parse_text("1 Grimwood Fiend (Really Enormous)\n").unwrap();
        assert_eq!(
            deck.entries[0].identifier,
            "Grimwood Fiend (Really Enormous)"
        );
    }

    #[test]
    fn sb_prefixed_lines_join_the_sideboard() {
        let deck = parse_text("4 Thornspire Adept\nSB: 2 Cinderveil Ward\n").unwrap();
        assert_eq!(deck.entries[0].section, None);
        assert_eq!(deck.entries[1].section, Some(Section::Sideboard));
        assert_eq!(deck.entries[1].identifier, "Cinderveil Ward");
        assert_eq!(deck.entries[1].count, 2);
        assert_eq!(parse_text("SB: 3\n"), Err(TextError::BlankName(1)));
    }

    #[test]
    fn about_blocks_and_comments_are_skipped() {
        let deck =
            parse_text("About\nName Big Red\n\n# comment\n// another\nDeck\n4 Thornspire Adept\n")
                .unwrap();
        assert_eq!(deck.entries.len(), 1);
        assert_eq!(deck.entries[0].identifier, "Thornspire Adept");
    }
}
