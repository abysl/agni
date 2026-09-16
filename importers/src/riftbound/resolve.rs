use super::card_code::CardCode;
use super::{ParsedDeck, Riftbound};
use crate::deck::{CardLookup, LookupError};
use agni_riftbound::{ResolvedCard, ResolvedDeck};

pub use crate::deck::Unresolved;

pub type Resolution = crate::deck::Resolution<Riftbound>;

pub const PRINT_SUFFIXES: [&str; 10] = [
    "Starter",
    "Alternate Art",
    "Overnumbered",
    "Signature",
    "Metal",
    "Promo",
    "Prerelease",
    "Foil",
    "Ultimate",
    "Launch Exclusive",
];

pub const ALIASES: [(&str, &str); 18] = [
    ("ven-145-166", "Nasus - Curator of the Sands"),
    ("ven-167-166", "Vi - Destructive"),
    ("ven-168-166", "Jinx - Demolitionist"),
    ("ven-172-166", "Draven - Showboat"),
    ("ven-174-166", "Irelia - Fervent"),
    ("ven-175-166", "Jayce - Man of Progress"),
    ("ven-176-166", "Viktor - Innovator"),
    ("ven-179-166", "Rengar - Trophy Hunter"),
    ("ven-180-166", "Kha'Zix - Evolving Hunter"),
    ("ven-183-166", "Diana - No Longer Human"),
    ("ven-184-166", "Leona - Determined"),
    ("ven-192-166", "Nasus - Curator of the Sands"),
    ("ven-sp1-006", "Kai'Sa - Survivor"),
    ("ven-sp2-006", "Sona - Harmonious"),
    ("ven-sp3-006", "Ahri - Inquisitive"),
    ("ven-sp4-006", "Sett - Brawler"),
    ("ven-sp5-006", "Ezreal - Prodigy"),
    ("ven-sp6-006", "Lux - Crownguard"),
];

pub fn base_name(name: &str) -> &str {
    let trimmed = name.trim_end();
    let Some(stem) = trimmed.strip_suffix(')') else {
        return trimmed;
    };
    let Some(open) = stem.rfind(" (") else {
        return trimmed;
    };
    let suffix = &stem[open + 2..];
    if PRINT_SUFFIXES.contains(&suffix) {
        stem[..open].trim_end()
    } else {
        trimmed
    }
}

pub fn alias_of(riftbound_id: &str) -> Option<&'static str> {
    let fragment = CardCode::from_riftbound_id(riftbound_id)
        .ok()?
        .id_fragment();
    ALIASES
        .iter()
        .find(|(print, _)| {
            CardCode::from_riftbound_id(print)
                .map(|code| code.id_fragment() == fragment)
                .unwrap_or(false)
        })
        .map(|(_, name)| *name)
}

pub fn canonical_name(riftbound_id: &str, name: &str) -> String {
    match alias_of(riftbound_id) {
        Some(alias) => alias.to_string(),
        None => base_name(name).to_string(),
    }
}

fn fold_card(card: &mut ResolvedCard) {
    card.name = canonical_name(&card.riftbound_id, &card.name);
}

pub fn fold_names(deck: &mut ResolvedDeck) {
    if let Some(card) = deck.legend.as_mut() {
        fold_card(card);
    }
    if let Some(card) = deck.chosen_champion.as_mut() {
        fold_card(card);
    }
    for zone in [
        &mut deck.main_deck,
        &mut deck.runes,
        &mut deck.battlefields,
        &mut deck.sideboard,
    ] {
        for entry in zone.iter_mut() {
            fold_card(&mut entry.card);
        }
    }
}

pub fn infer_champion(deck: &mut ResolvedDeck) -> bool {
    if deck.chosen_champion.is_some() {
        return false;
    }
    let Some(legend) = deck.legend.as_ref() else {
        return false;
    };
    let tags = agni_riftbound::legality::champion_tags(legend);
    let Some(index) = deck
        .main_deck
        .iter()
        .position(|entry| agni_riftbound::legality::fits_champion(&entry.card, &tags))
    else {
        return false;
    };
    let card = deck.main_deck[index].card.clone();
    if deck.main_deck[index].count > 1 {
        deck.main_deck[index].count -= 1;
    } else {
        deck.main_deck.remove(index);
    }
    deck.chosen_champion = Some(card);
    true
}

pub fn resolve(
    parsed: &ParsedDeck,
    cards: &mut dyn CardLookup<Riftbound>,
) -> Result<Resolution, LookupError> {
    let mut resolution = crate::deck::resolve(parsed, cards)?;
    fold_names(&mut resolution.deck);
    infer_champion(&mut resolution.deck);
    Ok(resolution)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::super::catalog::{
        CardKind, CatalogCard, StaticCatalog, SUPERTYPE_CHAMPION, SUPERTYPE_SIGNATURE,
    };
    use agni_riftbound::{DeckEntry, ResolvedCard, ResolvedDeck};
    use std::collections::BTreeMap;

    pub const RIFTCODEX_ROWS: &str = include_str!("testdata/riftcodex-2026-09.tsv");
    pub const CANONICAL_NAMES: &str = include_str!("testdata/canonical-names.tsv");
    pub const DECK_LISTS: [(&str, &str); 4] = [
        ("lillia", include_str!("testdata/deck-lillia.txt")),
        ("master-yi", include_str!("testdata/deck-master-yi.txt")),
        ("nasus", include_str!("testdata/deck-nasus.txt")),
        ("kha-zix", include_str!("testdata/deck-kha-zix.txt")),
    ];

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Row {
        pub id: String,
        pub name: String,
        pub kind: String,
        pub supertype: Option<String>,
        pub domain: Vec<String>,
        pub energy: Option<u8>,
        pub might: Option<u8>,
        pub power: Option<u8>,
        pub clean_name: Option<String>,
        pub tags: Vec<String>,
        pub set_id: String,
    }

    fn small(field: &str) -> Option<u8> {
        field.parse().ok()
    }

    fn optional(field: &str) -> Option<String> {
        if field.is_empty() {
            None
        } else {
            Some(field.to_string())
        }
    }

    pub fn rows() -> Vec<Row> {
        RIFTCODEX_ROWS
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                assert_eq!(fields.len(), 11, "{line}");
                Row {
                    id: fields[0].to_string(),
                    name: fields[1].to_string(),
                    kind: fields[2].to_string(),
                    supertype: optional(fields[3]),
                    domain: fields[4]
                        .split('/')
                        .filter(|d| !d.is_empty())
                        .map(String::from)
                        .collect(),
                    energy: small(fields[5]),
                    might: small(fields[6]),
                    power: small(fields[7]),
                    clean_name: optional(fields[8]),
                    tags: fields[9]
                        .split('/')
                        .filter(|t| !t.is_empty())
                        .map(String::from)
                        .collect(),
                    set_id: fields[10].to_string(),
                }
            })
            .collect()
    }

    pub fn catalog_card(row: &Row, name: &str) -> CatalogCard {
        CatalogCard {
            name: name.to_string(),
            riftbound_id: row.id.clone(),
            kind: CardKind::parse(&row.kind),
            champion: row.supertype.as_deref() == Some(SUPERTYPE_CHAMPION),
            image_url: None,
            energy: row.energy,
            power: row.power,
            might: row.might,
            domain: row.domain.clone(),
            tags: row.tags.clone(),
            signature: row.supertype.as_deref() == Some(SUPERTYPE_SIGNATURE),
            set_id: Some(row.set_id.clone()),
            text: None,
        }
    }

    fn base_prints_first(rows: &mut [Row]) {
        rows.sort_by_cached_key(|row| {
            let code = super::super::card_code::CardCode::from_riftbound_id(&row.id).ok();
            let variant = code.is_some_and(|code| {
                code.variant != super::super::card_code::Variant::Base
                    || code.set.reprints_another_set()
            });
            let suffixed = super::base_name(&row.name) != row.name;
            (variant || suffixed, row.id.clone())
        });
    }

    pub fn raw_catalog() -> StaticCatalog {
        let mut rows = rows();
        base_prints_first(&mut rows);
        StaticCatalog::new(
            rows.iter()
                .map(|row| catalog_card(row, &row.name))
                .collect(),
        )
    }

    pub fn folded_catalog() -> StaticCatalog {
        let mut rows = rows();
        base_prints_first(&mut rows);
        StaticCatalog::new(
            rows.iter()
                .map(|row| catalog_card(row, &super::canonical_name(&row.id, &row.name)))
                .collect(),
        )
    }

    fn names_of(entries: &[DeckEntry]) -> Vec<(String, u32)> {
        entries
            .iter()
            .map(|entry| (entry.card.name.clone(), entry.count))
            .collect()
    }

    pub fn zone_names(deck: &ResolvedDeck) -> Vec<(&'static str, Vec<(String, u32)>)> {
        let single =
            |card: &Option<ResolvedCard>| card.iter().map(|card| (card.name.clone(), 1)).collect();
        vec![
            ("legend", single(&deck.legend)),
            ("champion", single(&deck.chosen_champion)),
            ("main", names_of(&deck.main_deck)),
            ("runes", names_of(&deck.runes)),
            ("battlefields", names_of(&deck.battlefields)),
            ("sideboard", names_of(&deck.sideboard)),
        ]
    }

    pub fn pool_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../games/riftbound/rules/pool")
    }

    pub fn pool_blocks() -> Vec<(String, String)> {
        let dir = pool_dir();
        let mut blocks: Vec<(String, String)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .filter_map(|path| {
                let markdown = std::fs::read_to_string(&path).unwrap();
                let block = super::super::text_list::deck_block(&markdown)?.to_string();
                Some((
                    path.file_stem().unwrap().to_string_lossy().into_owned(),
                    block,
                ))
            })
            .collect();
        blocks.sort();
        assert_eq!(blocks.len(), 6, "{}", dir.display());
        blocks
    }

    pub fn pool_decks() -> Vec<(String, ResolvedDeck)> {
        let mut catalog = folded_catalog();
        pool_blocks()
            .into_iter()
            .map(|(slug, block)| {
                let parsed = super::super::text_list::parse_text(&block)
                    .unwrap_or_else(|error| panic!("{slug}: {error}"));
                let resolution = super::resolve(&parsed, &mut catalog).unwrap();
                assert!(
                    resolution.unresolved.is_empty(),
                    "{slug}: {:?}",
                    resolution.unresolved
                );
                (slug, resolution.deck)
            })
            .collect()
    }

    pub fn with_sideboard(deck: &ResolvedDeck) -> ResolvedDeck {
        let mut catalog = folded_catalog();
        let text = "Sideboard\n2 Decree of Focus\n2 Decree of Insight\n1 Smoke and Mirrors\n2 Disarming Rake\n1 Pickpocket\n1 Thousand-Tailed Watcher\n1 Unchecked Power\n";
        let parsed = super::super::text_list::parse_text(text).unwrap();
        let resolution = super::resolve(&parsed, &mut catalog).unwrap();
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        ResolvedDeck {
            sideboard: resolution.deck.sideboard,
            ..deck.clone()
        }
    }

    pub fn canonical() -> BTreeMap<String, String> {
        CANONICAL_NAMES
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let (id, name) = line.split_once('\t').unwrap();
                (name.to_string(), id.to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::test_catalog;
    use super::super::text_list::parse_text;
    use super::super::{parse_deck, DeckSource, Identifier, ParsedEntry, Section};
    use super::fixtures::{self, zone_names};
    use super::*;

    fn assert_canonical(deck_name: &str, resolution: &Resolution) {
        let canonical = fixtures::canonical();
        assert!(
            resolution.unresolved.is_empty(),
            "{deck_name}: {:?}",
            resolution.unresolved
        );
        let mut faces = 0;
        for (zone, names) in zone_names(&resolution.deck) {
            for (name, _) in names {
                faces += 1;
                assert!(
                    canonical.contains_key(&name),
                    "{deck_name} {zone}: {name:?} is not a canonical name"
                );
            }
        }
        assert!(faces >= 30, "{deck_name}: only {faces} faces");
    }

    #[test]
    fn print_suffixes_fold_away_and_real_parentheses_stay() {
        assert_eq!(
            base_name("Master Yi - Wuju Bladesman (Starter)"),
            "Master Yi - Wuju Bladesman"
        );
        assert_eq!(
            base_name("Nasus, Ascended (Alternate Art)"),
            "Nasus, Ascended"
        );
        assert_eq!(
            base_name("Lillia - Bashful Bloom (Overnumbered)"),
            "Lillia - Bashful Bloom"
        );
        assert_eq!(
            base_name("Vi - Piltover Enforcer (Signature)"),
            "Vi - Piltover Enforcer"
        );
        assert_eq!(base_name("Sett - The Boss (Metal)"), "Sett - The Boss");
        assert_eq!(base_name("Jinx - Rebel (Promo)"), "Jinx - Rebel");
        assert_eq!(base_name("Jinx - Rebel (Prerelease)"), "Jinx - Rebel");
        assert_eq!(base_name("Jinx - Rebel (Foil)"), "Jinx - Rebel");
        assert_eq!(base_name("Baron Nashor (Ultimate)"), "Baron Nashor");
        assert_eq!(
            base_name("Ahri - Alluring (Launch Exclusive)"),
            "Ahri - Alluring"
        );
        assert_eq!(base_name("Teemo - Scout (GG EZ)"), "Teemo - Scout (GG EZ)");
        assert_eq!(base_name("Gold // Buff"), "Gold // Buff");
        assert_eq!(base_name("Charm"), "Charm");
        assert_eq!(base_name("(Starter)"), "(Starter)");
    }

    #[test]
    fn aliases_rename_the_prints_riftcodex_renamed_and_target_canonical_names() {
        let canonical = fixtures::canonical();
        let rows = fixtures::rows();
        for (id, name) in ALIASES {
            let carried_elsewhere = rows
                .iter()
                .any(|row| row.id != id && base_name(&row.name) == name);
            assert!(carried_elsewhere, "{id} -> {name}");
            assert_eq!(canonical_name(id, "whatever the endpoint says"), name);
            assert_eq!(canonical_name(&id.to_ascii_uppercase(), "x"), name);
        }
        for id in ["ven-145-166", "ven-192-166", "ven-179-166"] {
            assert!(canonical.contains_key(alias_of(id).unwrap()), "{id}");
        }
        let mut spellings = std::collections::BTreeMap::new();
        for row in &rows {
            spellings
                .entry(crate::naming::normalize_name(&canonical_name(
                    &row.id, &row.name,
                )))
                .or_insert_with(std::collections::BTreeSet::new)
                .insert(canonical_name(&row.id, &row.name));
        }
        let ambiguous: Vec<_> = spellings.values().filter(|forms| forms.len() > 1).collect();
        assert!(ambiguous.is_empty(), "{ambiguous:?}");
        assert_eq!(
            canonical_name("ven-192", "Curator of the Sands"),
            "Nasus - Curator of the Sands"
        );
        assert_eq!(
            canonical_name("ven-1920-166", "Something Else"),
            "Something Else"
        );
        assert_eq!(
            canonical_name("ogs-019-024", "Master Yi - Wuju Bladesman (Starter)"),
            "Master Yi - Wuju Bladesman"
        );
    }

    #[test]
    fn every_reprint_in_the_catalog_folds_to_the_name_of_a_same_numbered_print() {
        let rows = fixtures::rows();
        let mut folded = 0;
        for row in &rows {
            let name = canonical_name(&row.id, &row.name);
            if name == row.name {
                continue;
            }
            folded += 1;
            let twin = rows
                .iter()
                .any(|other| other.id != row.id && canonical_name(&other.id, &other.name) == name);
            assert!(
                twin,
                "{} folds to {name:?}, which no other print carries",
                row.id
            );
        }
        assert!(folded > 200, "{folded}");
    }

    #[test]
    fn the_four_tournament_lists_resolve_to_canonical_names_through_a_folded_catalog() {
        let mut catalog = fixtures::folded_catalog();
        for (deck_name, text) in fixtures::DECK_LISTS {
            let parsed = parse_text(text).unwrap();
            let resolution = resolve(&parsed, &mut catalog).unwrap();
            assert_canonical(deck_name, &resolution);
        }
    }

    #[test]
    fn the_four_tournament_lists_resolve_to_canonical_names_through_a_raw_catalog() {
        let mut catalog = fixtures::raw_catalog();
        for (deck_name, text) in fixtures::DECK_LISTS {
            let parsed = parse_text(text).unwrap();
            let resolution = resolve(&parsed, &mut catalog).unwrap();
            assert_canonical(deck_name, &resolution);
        }
    }

    #[test]
    fn the_six_pool_decks_resolve_completely_against_the_fixture_catalog_with_tags() {
        for (slug, deck) in fixtures::pool_decks() {
            let legend = deck
                .legend
                .as_ref()
                .unwrap_or_else(|| panic!("{slug}: legend"));
            assert!(!legend.tags.is_empty(), "{slug}: {} has tags", legend.name);
            assert!(deck.chosen_champion.is_some(), "{slug}");
            assert_eq!(agni_deck::total(&deck.main_deck), 39, "{slug}");
            assert_eq!(agni_deck::total(&deck.runes), 12, "{slug}");
            assert_eq!(agni_deck::total(&deck.battlefields), 3, "{slug}");
            assert!(deck.sideboard.is_empty(), "{slug}");
            let report =
                agni_riftbound::legality::check(&deck, agni_riftbound::legality::Mode::Standard);
            assert_eq!(
                report.verdict,
                agni_riftbound::legality::Verdict::Legal,
                "{slug}: {:?}",
                report.findings
            );
            let mut untagged = deck.clone();
            for card in untagged
                .legend
                .iter_mut()
                .chain(untagged.chosen_champion.iter_mut())
            {
                card.tags.clear();
            }
            for zone in [
                &mut untagged.main_deck,
                &mut untagged.runes,
                &mut untagged.battlefields,
            ] {
                for entry in zone.iter_mut() {
                    entry.card.tags.clear();
                    entry.card.signature = false;
                }
            }
            let report = agni_riftbound::legality::check(
                &untagged,
                agni_riftbound::legality::Mode::Standard,
            );
            assert_eq!(report.breaks(), 0, "{slug}: {:?}", report.findings);
            assert_eq!(
                report.verdict,
                agni_riftbound::legality::Verdict::Unverified,
                "{slug}"
            );
        }
    }

    #[test]
    fn comma_and_dash_forms_reach_the_same_card() {
        let mut catalog = fixtures::folded_catalog();
        let parsed = parse_text(
            "Legend\n1 Master Yi, Wuju Bladesman\nMain Deck\n3 Rengar, Trophy Hunter\n1 Rengar - Trophy Hunter\n2 Fiora, Peerless\n",
        )
        .unwrap();
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        let deck = resolution.deck;
        assert_eq!(deck.legend.unwrap().name, "Master Yi - Wuju Bladesman");
        assert_eq!(deck.main_deck.len(), 2);
        assert_eq!(deck.main_deck[0].card.name, "Rengar - Trophy Hunter");
        assert_eq!(deck.main_deck[0].count, 4);
        assert_eq!(deck.main_deck[1].card.name, "Fiora - Peerless");
    }

    #[test]
    fn a_variant_print_resolves_by_id_to_its_base_name() {
        let mut catalog = fixtures::raw_catalog();
        let parsed = ParsedDeck {
            entries: vec![
                ParsedEntry {
                    identifier: Identifier::Id("ven-192-166".into()),
                    count: 1,
                    section: Some(Section::Legend),
                },
                ParsedEntry {
                    identifier: Identifier::Id("ven-046a-166".into()),
                    count: 1,
                    section: Some(Section::Champion),
                },
                ParsedEntry {
                    identifier: Identifier::Id("ogs-019-024".into()),
                    count: 1,
                    section: None,
                },
                ParsedEntry {
                    identifier: Identifier::Code(CardCode::parse("OPP-083").unwrap()),
                    count: 1,
                    section: None,
                },
            ],
        };
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        let deck = resolution.deck;
        assert_eq!(deck.legend.unwrap().name, "Nasus - Curator of the Sands");
        assert_eq!(deck.chosen_champion.unwrap().name, "Nasus, Ascended");
        assert_eq!(deck.main_deck[0].card.name, "Master Yi - Wuju Bladesman");
        assert_eq!(deck.main_deck[0].card.riftbound_id, "ogs-019-024");
        assert_eq!(deck.main_deck[1].card.name, "Consult the Past");
        assert_eq!(deck.main_deck[1].card.riftbound_id, "opp-083-298");
    }

    #[test]
    fn a_counted_champion_or_legend_line_keeps_its_extra_copies_in_main() {
        let mut catalog = test_catalog();
        let parsed = parse_text(
            "Legend\n2 Vanguard Sentinel\nChampion\n3 Emberwing Scout\nMain Deck\n1 Gloomvale Trickster",
        )
        .unwrap();
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        assert!(resolution.unresolved.is_empty());
        let deck = resolution.deck;
        assert_eq!(deck.legend.unwrap().name, "Vanguard Sentinel");
        assert_eq!(deck.chosen_champion.unwrap().name, "Emberwing Scout");
        let counts: Vec<(String, u32)> = deck
            .main_deck
            .iter()
            .map(|entry| (entry.card.name.clone(), entry.count))
            .collect();
        assert_eq!(
            counts,
            vec![
                ("Vanguard Sentinel".to_string(), 1),
                ("Emberwing Scout".to_string(), 2),
                ("Gloomvale Trickster".to_string(), 1),
            ]
        );
    }

    #[test]
    fn zones_reconstruct_from_card_kinds_without_sections() {
        let mut catalog = test_catalog();
        let parsed = ParsedDeck {
            entries: vec![
                ParsedEntry {
                    identifier: Identifier::Name("Vanguard Sentinel".into()),
                    count: 1,
                    section: None,
                },
                ParsedEntry {
                    identifier: Identifier::Name("Ember Rune".into()),
                    count: 12,
                    section: None,
                },
                ParsedEntry {
                    identifier: Identifier::Name("Sunken Causeway".into()),
                    count: 1,
                    section: None,
                },
                ParsedEntry {
                    identifier: Identifier::Name("Emberwing Scout".into()),
                    count: 3,
                    section: None,
                },
            ],
        };
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        assert!(resolution.unresolved.is_empty());
        let deck = resolution.deck;
        assert_eq!(deck.legend.unwrap().name, "Vanguard Sentinel");
        assert_eq!(deck.runes.len(), 1);
        assert_eq!(deck.runes[0].count, 12);
        assert_eq!(deck.battlefields.len(), 1);
        assert_eq!(deck.main_deck.len(), 1);
        assert_eq!(deck.main_deck[0].card.riftbound_id, "ogn-007-298");
    }

    #[test]
    fn explicit_sections_win_over_kinds() {
        let mut catalog = test_catalog();
        let parsed = ParsedDeck {
            entries: vec![ParsedEntry {
                identifier: Identifier::Name("Sudden Undertow".into()),
                count: 2,
                section: Some(Section::Sideboard),
            }],
        };
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        assert_eq!(resolution.deck.sideboard.len(), 1);
        assert!(resolution.deck.main_deck.is_empty());
    }

    #[test]
    fn a_deck_code_resolves_with_champion_and_zones() {
        let mut catalog = test_catalog();
        let code = super::super::deck_code::encode(&super::super::deck_code::DecodedDeck {
            main: vec![
                super::super::deck_code::CodeEntry {
                    code: CardCode::parse("OGN-201").unwrap(),
                    count: 1,
                },
                super::super::deck_code::CodeEntry {
                    code: CardCode::parse("OGN-007").unwrap(),
                    count: 3,
                },
                super::super::deck_code::CodeEntry {
                    code: CardCode::parse("OGN-042").unwrap(),
                    count: 12,
                },
                super::super::deck_code::CodeEntry {
                    code: CardCode::parse("OGN-260").unwrap(),
                    count: 1,
                },
            ],
            sideboard: vec![super::super::deck_code::CodeEntry {
                code: CardCode::parse("OGN-088").unwrap(),
                count: 2,
            }],
            champion: Some(CardCode::parse("OGN-007").unwrap()),
        })
        .unwrap();
        let parsed = parse_deck(&DeckSource::Code(code)).unwrap();
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        let deck = resolution.deck;
        assert_eq!(deck.legend.unwrap().name, "Vanguard Sentinel");
        assert_eq!(deck.chosen_champion.unwrap().name, "Emberwing Scout");
        assert_eq!(deck.runes[0].count, 12);
        assert_eq!(deck.battlefields[0].card.name, "Sunken Causeway");
        assert_eq!(deck.sideboard[0].count, 2);
        assert_eq!(deck.main_deck.len(), 1);
        assert_eq!(
            deck.main_deck[0].count, 2,
            "the code's main list holds the champion copy; the pointer takes it out"
        );
    }

    #[test]
    fn unresolved_codes_are_described_as_codes() {
        let mut catalog = test_catalog();
        let parsed = ParsedDeck {
            entries: vec![ParsedEntry {
                identifier: Identifier::Code(CardCode::parse("OGN-999").unwrap()),
                count: 2,
                section: None,
            }],
        };
        let resolution = resolve(&parsed, &mut catalog).unwrap();
        assert_eq!(resolution.unresolved.len(), 1);
        assert_eq!(resolution.unresolved[0].identifier, "OGN-999");
    }
}
