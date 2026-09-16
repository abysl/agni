use super::{Game, ParsedDeck, ParsedEntry};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    Empty,
    BlankName(usize),
    ZeroCount(usize),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the deck list is empty"),
            Self::BlankName(line) => write!(f, "line {line} has a count but no card name"),
            Self::ZeroCount(line) => write!(f, "line {line} asks for zero copies"),
        }
    }
}

impl std::error::Error for TextError {}

pub trait TextVocabulary {
    type Game: Game;

    fn section(header: &str) -> Option<<Self::Game as Game>::Section>;

    fn identifier(name: &str) -> <Self::Game as Game>::Identifier;

    fn inline_section(_line: &str) -> Option<(&str, <Self::Game as Game>::Section)> {
        None
    }

    fn skip(_section: <Self::Game as Game>::Section) -> bool {
        false
    }

    fn clean_name(name: &str) -> &str {
        name
    }
}

pub fn unwrap_header(line: &str) -> &str {
    let line = line.trim();
    let tilded = line
        .strip_prefix("~~")
        .and_then(|rest| rest.strip_suffix("~~"))
        .map(str::trim);
    let hashed = line
        .strip_prefix('#')
        .filter(|rest| rest.starts_with('#'))
        .map(|rest| rest.trim_start_matches('#').trim());
    tilded.or(hashed).unwrap_or(line)
}

fn header_of(line: &str) -> String {
    let mut header = unwrap_header(line).trim_end_matches(':').trim();
    if let Some(open) = header.rfind('(') {
        let tail = &header[open..];
        if tail.ends_with(')') && tail[1..tail.len() - 1].chars().all(|c| c.is_ascii_digit()) {
            header = header[..open].trim();
        }
    }
    header.to_ascii_lowercase()
}

fn is_title(line: &str) -> bool {
    line.starts_with('#') && !line[1..].starts_with('#')
}

fn digits(token: &str) -> Option<u32> {
    if token.is_empty() || !token.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    token.parse().ok()
}

pub fn split_count(line: &str) -> (Option<u32>, &str) {
    if let Some((first, rest)) = line.split_once(char::is_whitespace) {
        let token = first.strip_suffix(['x', 'X']).unwrap_or(first);
        if let Some(count) = digits(token) {
            let rest = rest.trim_start();
            let rest = rest
                .strip_prefix(['x', 'X'])
                .filter(|after| after.starts_with(char::is_whitespace))
                .map(str::trim_start)
                .unwrap_or(rest);
            return (Some(count), rest);
        }
    }
    if let Some((name, last)) = line.rsplit_once(char::is_whitespace) {
        if let Some(count) = last.strip_prefix(['x', 'X']).and_then(digits) {
            return (Some(count), name.trim_end());
        }
        if let Some(count) = digits(last) {
            let name = name.trim_end();
            if let Some(name) = name.strip_suffix(['x', 'X']) {
                if name.ends_with(char::is_whitespace) {
                    return (Some(count), name.trim_end());
                }
            }
        }
    }
    (None, line)
}

fn count_only(line: &str) -> bool {
    let bare = line.strip_suffix(['x', 'X']).unwrap_or(line);
    !bare.is_empty() && bare.chars().all(|c| c.is_ascii_digit())
}

pub fn parse_text<V: TextVocabulary>(input: &str) -> Result<ParsedDeck<V::Game>, TextError> {
    let mut deck = ParsedDeck::default();
    let mut section = None;
    for (index, raw) in input.lines().enumerate() {
        let line_number = index + 1;
        let mut line = raw.trim();
        if line.is_empty() || is_title(line) || line.starts_with("//") {
            continue;
        }
        if let Some(next) = V::section(&header_of(line)) {
            section = Some(next);
            continue;
        }
        if unwrap_header(line) != line {
            continue;
        }
        if section.is_some_and(V::skip) {
            continue;
        }
        let mut line_section = section;
        if let Some((rest, inline)) = V::inline_section(line) {
            line = rest.trim_start();
            line_section = Some(inline);
        }
        if count_only(line) {
            return Err(TextError::BlankName(line_number));
        }
        let (count, name) = split_count(line);
        if count == Some(0) {
            return Err(TextError::ZeroCount(line_number));
        }
        let name = V::clean_name(name);
        if name.is_empty() {
            return Err(TextError::BlankName(line_number));
        }
        deck.entries.push(ParsedEntry {
            identifier: V::identifier(name),
            count: count.unwrap_or(1),
            section: line_section,
        });
    }
    if deck.entries.is_empty() {
        return Err(TextError::Empty);
    }
    Ok(deck)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{Section, TestGame};
    use super::*;

    fn parse(input: &str) -> Result<ParsedDeck<TestGame>, TextError> {
        parse_text::<TestGame>(input)
    }

    #[test]
    fn a_bare_list_parses_counts_and_names() {
        let deck = parse("3 Emberwing Scout\n2x Gloomvale Trickster\nLone Wanderer\n").unwrap();
        assert_eq!(deck.entries.len(), 3);
        assert_eq!(deck.entries[0].count, 3);
        assert_eq!(deck.entries[0].identifier, "Emberwing Scout");
        assert_eq!(deck.entries[1].count, 2);
        assert_eq!(deck.entries[1].identifier, "Gloomvale Trickster");
        assert_eq!(deck.entries[2].count, 1);
        assert_eq!(deck.entries[2].identifier, "Lone Wanderer");
        assert!(deck.entries.iter().all(|entry| entry.section.is_none()));
    }

    #[test]
    fn names_keep_their_commas_and_inner_numbers() {
        let deck = parse("4 Yorick, Keeper of the 1000 Graves\n").unwrap();
        assert_eq!(deck.entries[0].count, 4);
        assert_eq!(
            deck.entries[0].identifier,
            "Yorick, Keeper of the 1000 Graves"
        );
    }

    #[test]
    fn section_headers_assign_zones_and_shed_their_counts() {
        let deck = parse("Main Deck (40)\n3 Scout\nSideboard:\n2 Warden\nDECK\n1 Rune\n").unwrap();
        let sections: Vec<Option<Section>> =
            deck.entries.iter().map(|entry| entry.section).collect();
        assert_eq!(
            sections,
            vec![
                Some(Section::Main),
                Some(Section::Side),
                Some(Section::Main)
            ]
        );
    }

    #[test]
    fn a_card_named_like_a_header_still_parses_as_a_card() {
        let deck = parse("2 Sideboard of Old\n").unwrap();
        assert_eq!(deck.entries[0].identifier, "Sideboard of Old");
        assert_eq!(deck.entries[0].section, None);
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let deck = parse("\n# a comment\n// another\n3 Emberwing Scout\n\n").unwrap();
        assert_eq!(deck.entries.len(), 1);
    }

    #[test]
    fn errors_carry_line_numbers() {
        assert_eq!(parse(""), Err(TextError::Empty));
        assert_eq!(parse("3 Ok\n0 Broken\n"), Err(TextError::ZeroCount(2)));
        assert_eq!(parse("3\n"), Err(TextError::BlankName(1)));
        assert_eq!(parse("3x\n"), Err(TextError::BlankName(1)));
    }
}
