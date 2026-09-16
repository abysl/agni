use crate::{
    DeckEntry, ResolvedCard, ResolvedDeck, BATTLEFIELD_COUNT, KIND_LEGEND, KIND_UNIT,
    MAIN_DECK_SIZE, RUNE_DECK_SIZE,
};
use std::collections::BTreeMap;

pub const COPY_LIMIT: u32 = 3;
pub const SIGNATURE_CAP: u32 = 3;
pub const COLORLESS: &str = "Colorless";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standard,
}

impl Mode {
    pub fn battlefields(self) -> u32 {
        match self {
            Self::Standard => BATTLEFIELD_COUNT as u32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Break,
    Unverified,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Legend,
    Champion,
    Main,
    Runes,
    Battlefields,
    Sideboard,
    Deck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    LegendMissing,
    LegendNotLegend,
    OutOfIdentity {
        name: String,
    },
    MainSize {
        have: u32,
    },
    MainOverSize {
        have: u32,
    },
    ChampionMissing,
    ChampionNotChampionUnit,
    ChampionTag {
        legend_tags: Vec<String>,
        champion_tags: Vec<String>,
    },
    CopyLimit {
        name: String,
        have: u32,
    },
    SignatureCap {
        have: u32,
    },
    SignatureTag {
        name: String,
    },
    TagsUnverified,
    RuneCount {
        have: u32,
    },
    RuneOutOfIdentity {
        name: String,
    },
    BattlefieldCount {
        have: u32,
    },
    BattlefieldDuplicate {
        name: String,
    },
    SideboardWouldBreak {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: Rule,
    pub cite: &'static str,
    pub grade: Grade,
    pub zone: Zone,
    pub cards: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Meter {
    pub legend: bool,
    pub champion: bool,
    pub main: (u32, u32),
    pub runes: (u32, u32),
    pub battlefields: (u32, u32),
    pub signatures: (u32, u32),
    pub sideboard: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Legal,
    Broken(usize),
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub verdict: Verdict,
    pub identity: Vec<String>,
    pub meter: Meter,
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn breaks(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.grade == Grade::Break)
            .count()
    }

    pub fn flagged(&self, riftbound_id: &str) -> Option<Grade> {
        self.findings
            .iter()
            .filter(|finding| finding.cards.iter().any(|id| id == riftbound_id))
            .map(|finding| finding.grade)
            .min_by_key(|grade| match grade {
                Grade::Break => 0,
                Grade::Unverified => 1,
                Grade::Advisory => 2,
            })
    }
}

pub fn identity(legend: &ResolvedCard) -> Vec<String> {
    legend.domain.clone()
}

pub fn fits_identity(identity: &[String], domains: &[String]) -> bool {
    domains
        .iter()
        .all(|domain| domain == COLORLESS || identity.contains(domain))
}

pub fn name_stem(name: &str) -> &str {
    let stem = name
        .split_once(" - ")
        .map(|(stem, _)| stem)
        .or_else(|| name.split_once(", ").map(|(stem, _)| stem))
        .unwrap_or(name);
    stem.trim()
}

pub fn champion_name(name: &str) -> &str {
    let stem = name_stem(name);
    stem.rsplit(", ").next().unwrap_or(stem).trim()
}

pub fn champion_tags(legend: &ResolvedCard) -> Vec<String> {
    let champion = champion_name(&legend.name);
    if legend.tags.is_empty() {
        return vec![champion.to_string()];
    }
    let named: Vec<String> = legend
        .tags
        .iter()
        .filter(|tag| tag.eq_ignore_ascii_case(champion))
        .cloned()
        .collect();
    if named.is_empty() {
        legend.tags.clone()
    } else {
        named
    }
}

pub fn fits_champion(card: &ResolvedCard, champion_tags: &[String]) -> bool {
    match champion_unit(card) {
        Some(false) => false,
        Some(true) => champion_tags
            .iter()
            .any(|tag| card.tags.iter().any(|own| own.eq_ignore_ascii_case(tag))),
        None => {
            let stem = name_stem(&card.name);
            stem != card.name.trim()
                && champion_tags
                    .iter()
                    .any(|tag| tag.eq_ignore_ascii_case(stem))
        }
    }
}

pub fn champion_unit(card: &ResolvedCard) -> Option<bool> {
    if card.kind.as_deref() != Some(KIND_UNIT) || card.signature {
        return Some(false);
    }
    if card.tags.is_empty() {
        return None;
    }
    let stem = name_stem(&card.name);
    Some(card.tags.iter().any(|tag| tag.eq_ignore_ascii_case(stem)))
}

pub fn rune_split(domains: usize, total: u32) -> Vec<u32> {
    if domains == 0 {
        return Vec::new();
    }
    let each = total / domains as u32;
    let remainder = total % domains as u32;
    let mut split = vec![each; domains];
    split[0] += remainder;
    split
}

fn shares_tag(left: &[String], right: &[String]) -> bool {
    left.iter()
        .any(|tag| right.iter().any(|other| other.eq_ignore_ascii_case(tag)))
}

fn join(list: &[String]) -> String {
    if list.is_empty() {
        "none".to_string()
    } else {
        list.join("/")
    }
}

fn kind_of(card: &ResolvedCard) -> &str {
    card.kind.as_deref().unwrap_or("card of unknown kind")
}

fn main_cards(deck: &ResolvedDeck) -> Vec<(&ResolvedCard, u32)> {
    deck.chosen_champion
        .iter()
        .map(|card| (card, 1))
        .chain(
            deck.main_deck
                .iter()
                .map(|entry| (&entry.card, entry.count)),
        )
        .collect()
}

fn every_card(deck: &ResolvedDeck) -> Vec<&ResolvedCard> {
    let zones: [&[DeckEntry]; 4] = [
        &deck.main_deck,
        &deck.runes,
        &deck.battlefields,
        &deck.sideboard,
    ];
    deck.legend
        .iter()
        .chain(deck.chosen_champion.iter())
        .chain(zones.into_iter().flatten().map(|entry| &entry.card))
        .collect()
}

struct ByName<'a> {
    count: u32,
    ids: Vec<String>,
    card: &'a ResolvedCard,
}

fn by_name<'a>(cards: &[(&'a ResolvedCard, u32)]) -> BTreeMap<String, ByName<'a>> {
    let mut names: BTreeMap<String, ByName<'a>> = BTreeMap::new();
    for (card, count) in cards {
        let held = names.entry(card.name.clone()).or_insert(ByName {
            count: 0,
            ids: Vec::new(),
            card,
        });
        held.count += count;
        if !held.ids.contains(&card.riftbound_id) {
            held.ids.push(card.riftbound_id.clone());
        }
    }
    names
}

fn identity_cite(card: &ResolvedCard) -> &'static str {
    if card.domain.len() > 1 {
        "103.1.b.4"
    } else {
        "103.1.b.3"
    }
}

struct Checker<'a> {
    deck: &'a ResolvedDeck,
    mode: Mode,
    identity: Vec<String>,
    legend_tags: Vec<String>,
    tags_known: bool,
    findings: Vec<Finding>,
}

impl Checker<'_> {
    fn push(
        &mut self,
        rule: Rule,
        cite: &'static str,
        grade: Grade,
        zone: Zone,
        cards: Vec<String>,
        detail: String,
    ) {
        self.findings.push(Finding {
            rule,
            cite,
            grade,
            zone,
            cards,
            detail,
        });
    }

    fn legend(&mut self) {
        match &self.deck.legend {
            None => self.push(
                Rule::LegendMissing,
                "103.1",
                Grade::Break,
                Zone::Legend,
                Vec::new(),
                "no legend chosen".into(),
            ),
            Some(legend) if legend.kind.as_deref() != Some(KIND_LEGEND) => {
                let detail = format!("{} is a {}, not a legend", legend.name, kind_of(legend));
                self.push(
                    Rule::LegendNotLegend,
                    "103.1",
                    Grade::Break,
                    Zone::Legend,
                    vec![legend.riftbound_id.clone()],
                    detail,
                );
            }
            Some(_) => {}
        }
    }

    fn out_of_identity(&mut self, zone: Zone, cards: &[(&ResolvedCard, u32)]) {
        if self.deck.legend.is_none() {
            return;
        }
        for (name, held) in by_name(cards) {
            if fits_identity(&self.identity, &held.card.domain) {
                continue;
            }
            let detail = format!(
                "{name} is {} — outside the {} identity",
                join(&held.card.domain),
                join(&self.identity)
            );
            self.push(
                Rule::OutOfIdentity { name },
                identity_cite(held.card),
                Grade::Break,
                zone,
                held.ids,
                detail,
            );
        }
    }

    fn main_size(&mut self, have: u32) {
        let need = MAIN_DECK_SIZE as u32;
        if have < need {
            self.push(
                Rule::MainSize { have },
                "103.2",
                Grade::Break,
                Zone::Main,
                Vec::new(),
                format!("the main deck has {have} cards, {need} are needed (the chosen champion counts, the sideboard does not)"),
            );
        } else if have > need {
            self.push(
                Rule::MainOverSize { have },
                "103.2",
                Grade::Advisory,
                Zone::Main,
                Vec::new(),
                format!("the main deck has {have} cards; kai deals a shuffled {need} from the whole list"),
            );
        }
    }

    fn champion(&mut self) {
        let Some(champion) = &self.deck.chosen_champion else {
            self.push(
                Rule::ChampionMissing,
                "103.2.a",
                Grade::Break,
                Zone::Champion,
                Vec::new(),
                "no chosen champion".into(),
            );
            return;
        };
        let id = vec![champion.riftbound_id.clone()];
        if champion_unit(champion) == Some(false) {
            let why = if champion.signature {
                "a signature card, not a champion unit".to_string()
            } else if champion.kind.as_deref() != Some(KIND_UNIT) {
                format!("a {}, not a champion unit", kind_of(champion))
            } else {
                "a unit without its own champion tag, not a champion unit".to_string()
            };
            self.push(
                Rule::ChampionNotChampionUnit,
                "103.2.a.2",
                Grade::Break,
                Zone::Champion,
                id,
                format!("{} is {why}", champion.name),
            );
            return;
        }
        if self.deck.legend.is_none() {
            return;
        }
        let rule = Rule::ChampionTag {
            legend_tags: self.legend_tags.clone(),
            champion_tags: champion.tags.clone(),
        };
        if self.legend_tags.is_empty() || champion.tags.is_empty() {
            if self.tags_known {
                let untagged = if champion.tags.is_empty() {
                    champion.name.as_str()
                } else {
                    "the legend"
                };
                self.push(
                    rule,
                    "103.2.a.2",
                    Grade::Unverified,
                    Zone::Champion,
                    id,
                    format!("{untagged} carries no tags, so the champion tag cannot be checked"),
                );
            }
            return;
        }
        if !shares_tag(&self.legend_tags, &champion.tags) {
            let detail = format!(
                "{} is tagged {} but the legend asks for {}",
                champion.name,
                join(&champion.tags),
                join(&self.legend_tags)
            );
            self.push(rule, "103.2.a.2", Grade::Break, Zone::Champion, id, detail);
        }
    }

    fn copies(&mut self, cards: &[(&ResolvedCard, u32)]) {
        for (name, held) in by_name(cards) {
            if held.count > COPY_LIMIT {
                let detail = format!(
                    "{name} has {} copies, the limit is {COPY_LIMIT}",
                    held.count
                );
                self.push(
                    Rule::CopyLimit {
                        name,
                        have: held.count,
                    },
                    "103.2.b",
                    Grade::Break,
                    Zone::Main,
                    held.ids,
                    detail,
                );
            }
        }
    }

    fn signatures(&mut self, cards: &[(&ResolvedCard, u32)]) -> u32 {
        let signatures: Vec<(&ResolvedCard, u32)> = cards
            .iter()
            .copied()
            .filter(|(card, _)| card.signature)
            .collect();
        let have: u32 = signatures.iter().map(|(_, count)| count).sum();
        if have > SIGNATURE_CAP {
            let ids: Vec<String> = signatures
                .iter()
                .map(|(card, _)| card.riftbound_id.clone())
                .collect();
            self.push(
                Rule::SignatureCap { have },
                "103.2.d.1",
                Grade::Break,
                Zone::Deck,
                ids,
                format!("{have} signature cards, the cap is {SIGNATURE_CAP} regardless of name"),
            );
        }
        if self.deck.legend.is_none() {
            return have;
        }
        for (name, held) in by_name(&signatures) {
            if held.card.tags.is_empty() || self.legend_tags.is_empty() {
                if self.tags_known {
                    let untagged = if held.card.tags.is_empty() {
                        name.as_str()
                    } else {
                        "the legend"
                    };
                    let detail = format!(
                        "{untagged} carries no tags, so the signature tag of {name} cannot be checked"
                    );
                    self.push(
                        Rule::SignatureTag { name: name.clone() },
                        "103.2.d.2",
                        Grade::Unverified,
                        Zone::Main,
                        held.ids,
                        detail,
                    );
                }
                continue;
            }
            if !shares_tag(&self.legend_tags, &held.card.tags) {
                let detail = format!(
                    "{name} is a signature card tagged {} but the legend asks for {}",
                    join(&held.card.tags),
                    join(&self.legend_tags)
                );
                self.push(
                    Rule::SignatureTag { name },
                    "103.2.d.2",
                    Grade::Break,
                    Zone::Main,
                    held.ids,
                    detail,
                );
            }
        }
        have
    }

    fn tags(&mut self) {
        if self.tags_known || every_card(self.deck).is_empty() {
            return;
        }
        self.push(
            Rule::TagsUnverified,
            "103.2.a.2, 103.2.d",
            Grade::Unverified,
            Zone::Deck,
            Vec::new(),
            "no card carries tags — the catalog that resolved this deck predates tags, so the champion and signature rules cannot be checked".into(),
        );
    }

    fn runes(&mut self) -> u32 {
        let have = agni_deck::total(&self.deck.runes);
        if have != RUNE_DECK_SIZE as u32 {
            self.push(
                Rule::RuneCount { have },
                "103.3.a",
                Grade::Break,
                Zone::Runes,
                Vec::new(),
                format!("{have} runes, exactly {RUNE_DECK_SIZE} are needed"),
            );
        }
        if self.deck.legend.is_none() {
            return have;
        }
        let runes: Vec<(&ResolvedCard, u32)> = self
            .deck
            .runes
            .iter()
            .map(|entry| (&entry.card, entry.count))
            .collect();
        for (name, held) in by_name(&runes) {
            if fits_identity(&self.identity, &held.card.domain) {
                continue;
            }
            let detail = format!(
                "{name} is {} — outside the {} identity",
                join(&held.card.domain),
                join(&self.identity)
            );
            self.push(
                Rule::RuneOutOfIdentity { name },
                "103.3.a.1",
                Grade::Break,
                Zone::Runes,
                held.ids,
                detail,
            );
        }
        have
    }

    fn battlefields(&mut self) -> u32 {
        let have = agni_deck::total(&self.deck.battlefields);
        let need = self.mode.battlefields();
        if have != need {
            self.push(
                Rule::BattlefieldCount { have },
                "103.4.a",
                Grade::Break,
                Zone::Battlefields,
                Vec::new(),
                format!("{have} battlefields, exactly {need} are needed"),
            );
        }
        let fields: Vec<(&ResolvedCard, u32)> = self
            .deck
            .battlefields
            .iter()
            .map(|entry| (&entry.card, entry.count))
            .collect();
        for (name, held) in by_name(&fields) {
            if held.count > 1 {
                let detail = format!("{name} is in the deck {} times", held.count);
                self.push(
                    Rule::BattlefieldDuplicate { name },
                    "103.4.c",
                    Grade::Break,
                    Zone::Battlefields,
                    held.ids,
                    detail,
                );
            }
        }
        self.out_of_identity(Zone::Battlefields, &fields);
        have
    }

    fn sideboard(&mut self, main: &[(&ResolvedCard, u32)]) -> u32 {
        let have = agni_deck::total(&self.deck.sideboard);
        if self.deck.legend.is_none() {
            return have;
        }
        let copies = by_name(main);
        let side: Vec<(&ResolvedCard, u32)> = self
            .deck
            .sideboard
            .iter()
            .map(|entry| (&entry.card, entry.count))
            .collect();
        for (name, held) in by_name(&side) {
            let in_main = copies.get(&name).map(|held| held.count).unwrap_or(0);
            if !fits_identity(&self.identity, &held.card.domain) {
                let detail = format!(
                    "{name} is {} — swapping it in would break the {} identity",
                    join(&held.card.domain),
                    join(&self.identity)
                );
                self.push(
                    Rule::SideboardWouldBreak { name },
                    identity_cite(held.card),
                    Grade::Advisory,
                    Zone::Sideboard,
                    held.ids,
                    detail,
                );
            } else if in_main >= COPY_LIMIT {
                let detail = format!(
                    "the main deck already has {in_main} {name} — swapping one in would pass the limit of {COPY_LIMIT}"
                );
                self.push(
                    Rule::SideboardWouldBreak { name },
                    "103.2.b",
                    Grade::Advisory,
                    Zone::Sideboard,
                    held.ids,
                    detail,
                );
            }
        }
        have
    }
}

pub fn check(deck: &ResolvedDeck, mode: Mode) -> Report {
    let identity = deck.legend.as_ref().map(identity).unwrap_or_default();
    let legend_tags = deck.legend.as_ref().map(champion_tags).unwrap_or_default();
    let tags_known = every_card(deck).iter().any(|card| !card.tags.is_empty());
    let mut checker = Checker {
        deck,
        mode,
        identity,
        legend_tags,
        tags_known,
        findings: Vec::new(),
    };
    let main = main_cards(deck);
    let main_total: u32 = main.iter().map(|(_, count)| count).sum();
    checker.legend();
    checker.out_of_identity(Zone::Main, &main);
    checker.main_size(main_total);
    checker.champion();
    checker.copies(&main);
    let signatures = checker.signatures(&main);
    checker.tags();
    let runes = checker.runes();
    let battlefields = checker.battlefields();
    let sideboard = checker.sideboard(&main);
    let breaks = checker
        .findings
        .iter()
        .filter(|finding| finding.grade == Grade::Break)
        .count();
    let verdict = if breaks > 0 {
        Verdict::Broken(breaks)
    } else if checker
        .findings
        .iter()
        .any(|finding| finding.grade == Grade::Unverified)
    {
        Verdict::Unverified
    } else {
        Verdict::Legal
    };
    Report {
        verdict,
        identity: checker.identity,
        meter: Meter {
            legend: deck.legend.is_some(),
            champion: deck.chosen_champion.is_some(),
            main: (main_total, MAIN_DECK_SIZE as u32),
            runes: (runes, RUNE_DECK_SIZE as u32),
            battlefields: (battlefields, mode.battlefields()),
            signatures: (signatures, SIGNATURE_CAP),
            sideboard,
        },
        findings: checker.findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KIND_BATTLEFIELD, KIND_RUNE};

    fn card(name: &str, id: &str, kind: &str, domain: &[&str], tags: &[&str]) -> ResolvedCard {
        ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            kind: Some(kind.into()),
            domain: domain.iter().map(|d| d.to_string()).collect(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        }
    }

    fn entry(card: ResolvedCard, count: u32) -> DeckEntry {
        DeckEntry { card, count }
    }

    fn lillia_legend() -> ResolvedCard {
        card(
            "Lillia - Bashful Bloom",
            "unl-189-219",
            KIND_LEGEND,
            &["Calm", "Mind"],
            &["Lillia"],
        )
    }

    fn lillia_champion() -> ResolvedCard {
        card(
            "Lillia - Fae Fawn",
            "unl-082-219",
            KIND_UNIT,
            &["Mind"],
            &["Fae", "Lillia", "Ionia"],
        )
    }

    fn filler(index: u32) -> ResolvedCard {
        card(
            &format!("Filler {index}"),
            &format!("ogn-{index:03}-298"),
            KIND_UNIT,
            &["Calm"],
            &["Ionia"],
        )
    }

    fn legal_deck() -> ResolvedDeck {
        let mut main = Vec::new();
        for index in 0..13 {
            main.push(entry(filler(index), 3));
        }
        ResolvedDeck {
            legend: Some(lillia_legend()),
            chosen_champion: Some(lillia_champion()),
            main_deck: main,
            runes: vec![
                entry(
                    card("Calm Rune", "ogn-042-298", KIND_RUNE, &["Calm"], &[]),
                    7,
                ),
                entry(
                    card("Mind Rune", "ogn-089-298", KIND_RUNE, &["Mind"], &[]),
                    5,
                ),
            ],
            battlefields: vec![
                entry(
                    card(
                        "Dusk Rose Lab",
                        "unl-209-219",
                        KIND_BATTLEFIELD,
                        &["Colorless"],
                        &[],
                    ),
                    1,
                ),
                entry(
                    card(
                        "Ravenbloom Conservatory",
                        "sfd-215-221",
                        KIND_BATTLEFIELD,
                        &["Colorless"],
                        &[],
                    ),
                    1,
                ),
                entry(
                    card(
                        "Seat of Power",
                        "sfd-217-221",
                        KIND_BATTLEFIELD,
                        &["Colorless"],
                        &[],
                    ),
                    1,
                ),
            ],
            sideboard: Vec::new(),
        }
    }

    fn rules(report: &Report) -> Vec<&Rule> {
        report
            .findings
            .iter()
            .map(|finding| &finding.rule)
            .collect()
    }

    fn the_only(report: &Report) -> &Finding {
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        &report.findings[0]
    }

    #[test]
    fn a_full_deck_is_legal_with_a_full_meter() {
        let report = check(&legal_deck(), Mode::Standard);
        assert_eq!(report.verdict, Verdict::Legal, "{:?}", report.findings);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.identity,
            vec!["Calm".to_string(), "Mind".to_string()]
        );
        assert_eq!(
            report.meter,
            Meter {
                legend: true,
                champion: true,
                main: (40, 40),
                runes: (12, 12),
                battlefields: (3, 3),
                signatures: (0, 3),
                sideboard: 0,
            }
        );
    }

    #[test]
    fn an_empty_deck_reports_every_shortfall() {
        let report = check(&ResolvedDeck::default(), Mode::Standard);
        assert_eq!(
            rules(&report),
            vec![
                &Rule::LegendMissing,
                &Rule::MainSize { have: 0 },
                &Rule::ChampionMissing,
                &Rule::RuneCount { have: 0 },
                &Rule::BattlefieldCount { have: 0 },
            ]
        );
        assert_eq!(report.verdict, Verdict::Broken(5));
        assert!(report.identity.is_empty());
        assert_eq!(
            report.meter,
            Meter {
                legend: false,
                champion: false,
                main: (0, 40),
                runes: (0, 12),
                battlefields: (0, 3),
                signatures: (0, 3),
                sideboard: 0,
            }
        );
        for finding in &report.findings {
            assert_eq!(finding.grade, Grade::Break);
        }
        assert_eq!(report.findings[0].cite, "103.1");
        assert_eq!(report.findings[1].cite, "103.2");
        assert_eq!(report.findings[2].cite, "103.2.a");
        assert_eq!(report.findings[3].cite, "103.3.a");
        assert_eq!(report.findings[4].cite, "103.4.a");
    }

    #[test]
    fn a_non_legend_in_the_legend_slot_breaks() {
        let mut deck = legal_deck();
        deck.legend = Some(card(
            "Filler 99",
            "ogn-099-298",
            KIND_UNIT,
            &["Calm", "Mind"],
            &["Lillia"],
        ));
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(finding.rule, Rule::LegendNotLegend);
        assert_eq!(finding.zone, Zone::Legend);
        assert_eq!(finding.cards, vec!["ogn-099-298".to_string()]);
        assert!(
            finding.detail.contains("not a legend"),
            "{}",
            finding.detail
        );
    }

    #[test]
    fn an_out_of_identity_card_breaks_once_per_name_and_cites_the_domain_count() {
        let mut deck = legal_deck();
        deck.main_deck[0] = entry(
            card("Fury Fellow", "ogn-500-298", KIND_UNIT, &["Fury"], &[]),
            2,
        );
        deck.main_deck.push(entry(
            card("Fury Fellow", "ogn-501-298", KIND_UNIT, &["Fury"], &[]),
            1,
        ));
        deck.main_deck[1] = entry(
            card("Half In", "ogn-502-298", KIND_UNIT, &["Calm", "Chaos"], &[]),
            3,
        );
        let report = check(&deck, Mode::Standard);
        assert_eq!(report.verdict, Verdict::Broken(2));
        let fury = &report.findings[0];
        assert_eq!(
            fury.rule,
            Rule::OutOfIdentity {
                name: "Fury Fellow".into()
            }
        );
        assert_eq!(fury.cite, "103.1.b.3");
        assert_eq!(
            fury.cards,
            vec!["ogn-500-298".to_string(), "ogn-501-298".to_string()]
        );
        let half = &report.findings[1];
        assert_eq!(
            half.rule,
            Rule::OutOfIdentity {
                name: "Half In".into()
            }
        );
        assert_eq!(half.cite, "103.1.b.4");
        assert_eq!(half.zone, Zone::Main);
    }

    #[test]
    fn colorless_and_empty_domains_always_fit() {
        let identity = vec!["Calm".to_string()];
        assert!(fits_identity(&identity, &[]));
        assert!(fits_identity(&identity, &["Colorless".to_string()]));
        assert!(fits_identity(&identity, &["Calm".to_string()]));
        assert!(!fits_identity(&identity, &["Mind".to_string()]));
        assert!(!fits_identity(
            &identity,
            &["Calm".to_string(), "Mind".to_string()]
        ));
        assert!(fits_identity(
            &["Calm".to_string(), "Mind".to_string()],
            &["Calm".to_string(), "Mind".to_string()]
        ));
    }

    #[test]
    fn identity_rules_wait_for_a_legend() {
        let mut deck = legal_deck();
        deck.legend = None;
        deck.main_deck[0] = entry(
            card("Fury Fellow", "ogn-500-298", KIND_UNIT, &["Fury"], &[]),
            3,
        );
        deck.runes[0] = entry(
            card("Fury Rune", "ogn-040-298", KIND_RUNE, &["Fury"], &[]),
            7,
        );
        let report = check(&deck, Mode::Standard);
        assert_eq!(rules(&report), vec![&Rule::LegendMissing]);
    }

    #[test]
    fn the_main_deck_counts_the_champion_and_never_the_sideboard() {
        let mut deck = legal_deck();
        deck.main_deck.pop();
        deck.sideboard.push(entry(filler(50), 3));
        let report = check(&deck, Mode::Standard);
        let short = report
            .findings
            .iter()
            .find(|finding| matches!(finding.rule, Rule::MainSize { .. }))
            .expect("a size finding");
        assert_eq!(short.rule, Rule::MainSize { have: 37 });
        assert_eq!(short.grade, Grade::Break);
        assert_eq!(report.meter.main, (37, 40));
        assert_eq!(report.meter.sideboard, 3);
        deck.chosen_champion = None;
        let report = check(&deck, Mode::Standard);
        assert!(rules(&report).contains(&&Rule::MainSize { have: 36 }));
        assert!(rules(&report).contains(&&Rule::ChampionMissing));
    }

    #[test]
    fn an_oversized_main_deck_is_only_advised() {
        let mut deck = legal_deck();
        deck.main_deck.push(entry(filler(51), 2));
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(finding.rule, Rule::MainOverSize { have: 42 });
        assert_eq!(finding.grade, Grade::Advisory);
        assert_eq!(report.verdict, Verdict::Legal);
    }

    #[test]
    fn a_signature_unit_or_a_spell_cannot_be_the_champion() {
        let mut deck = legal_deck();
        let mut tibbers = card("Tibbers", "ogn-300-298", KIND_UNIT, &["Calm"], &["Annie"]);
        tibbers.signature = true;
        deck.chosen_champion = Some(tibbers);
        let report = check(&deck, Mode::Standard);
        let finding = &report.findings[0];
        assert_eq!(finding.rule, Rule::ChampionNotChampionUnit);
        assert_eq!(finding.cite, "103.2.a.2");
        assert!(finding.detail.contains("signature"), "{}", finding.detail);
        assert_eq!(
            report.findings[1].rule,
            Rule::SignatureTag {
                name: "Tibbers".into()
            }
        );
        assert_eq!(report.verdict, Verdict::Broken(2));
        deck.chosen_champion = Some(card(
            "Charm",
            "ogn-043-298",
            "Spell",
            &["Calm"],
            &["Lillia"],
        ));
        let report = check(&deck, Mode::Standard);
        assert_eq!(the_only(&report).rule, Rule::ChampionNotChampionUnit);
        deck.chosen_champion = Some(card(
            "Lecturing Yordle",
            "ogn-310-298",
            KIND_UNIT,
            &["Calm"],
            &["Yordle", "Lillia"],
        ));
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(finding.rule, Rule::ChampionNotChampionUnit);
        assert!(
            finding.detail.contains("without its own champion tag"),
            "{}",
            finding.detail
        );
    }

    #[test]
    fn a_champion_whose_tags_miss_the_legend_breaks() {
        let mut deck = legal_deck();
        deck.chosen_champion = Some(card(
            "Vi - Piltover Enforcer",
            "unl-229-219",
            KIND_UNIT,
            &["Calm"],
            &["Vi", "Piltover"],
        ));
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(
            finding.rule,
            Rule::ChampionTag {
                legend_tags: vec!["Lillia".into()],
                champion_tags: vec!["Vi".into(), "Piltover".into()],
            }
        );
        assert_eq!(finding.grade, Grade::Break);
        assert_eq!(finding.zone, Zone::Champion);
        assert_eq!(report.verdict, Verdict::Broken(1));
    }

    #[test]
    fn a_species_before_the_comma_is_not_the_champion_tag() {
        let mut deck = legal_deck();
        deck.legend = Some(card(
            "Yordle, Kennen - Heart of the Tempest",
            "sfd-190-221",
            KIND_LEGEND,
            &["Calm", "Mind"],
            &["Yordle", "Kennen"],
        ));
        deck.chosen_champion = Some(card(
            "Teemo - Scout",
            "ogn-150-298",
            KIND_UNIT,
            &["Calm"],
            &["Yordle", "Teemo", "Bandle City"],
        ));
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(
            finding.rule,
            Rule::ChampionTag {
                legend_tags: vec!["Kennen".into()],
                champion_tags: vec!["Yordle".into(), "Teemo".into(), "Bandle City".into()],
            }
        );
        assert_eq!(finding.grade, Grade::Break);
        deck.chosen_champion = Some(card(
            "Kennen - Storm's Edge",
            "sfd-120-221",
            KIND_UNIT,
            &["Calm"],
            &["Yordle", "Kennen", "Ionia"],
        ));
        let report = check(&deck, Mode::Standard);
        assert_eq!(report.verdict, Verdict::Legal, "{:?}", report.findings);
    }

    #[test]
    fn empty_tags_are_unverified_never_broken() {
        let mut deck = legal_deck();
        for card in every_card_mut(&mut deck) {
            card.tags.clear();
        }
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(finding.rule, Rule::TagsUnverified);
        assert_eq!(finding.grade, Grade::Unverified);
        assert_eq!(finding.zone, Zone::Deck);
        assert_eq!(report.verdict, Verdict::Unverified);
        let mut partial = legal_deck();
        partial.chosen_champion.as_mut().unwrap().tags.clear();
        let report = check(&partial, Mode::Standard);
        let finding = the_only(&report);
        assert!(matches!(finding.rule, Rule::ChampionTag { .. }));
        assert_eq!(finding.grade, Grade::Unverified);
        assert_eq!(report.verdict, Verdict::Unverified);
    }

    fn every_card_mut(deck: &mut ResolvedDeck) -> Vec<&mut ResolvedCard> {
        let mut cards: Vec<&mut ResolvedCard> = Vec::new();
        cards.extend(deck.legend.iter_mut());
        cards.extend(deck.chosen_champion.iter_mut());
        for zone in [
            &mut deck.main_deck,
            &mut deck.runes,
            &mut deck.battlefields,
            &mut deck.sideboard,
        ] {
            cards.extend(zone.iter_mut().map(|entry| &mut entry.card));
        }
        cards
    }

    #[test]
    fn more_than_three_of_a_name_breaks_across_prints_and_the_champion() {
        let mut deck = legal_deck();
        deck.main_deck[0] = entry(lillia_champion(), 2);
        let mut alternate = lillia_champion();
        alternate.riftbound_id = "unl-082a-219".into();
        deck.main_deck.push(entry(alternate, 1));
        let report = check(&deck, Mode::Standard);
        let copies = report
            .findings
            .iter()
            .find(|finding| matches!(finding.rule, Rule::CopyLimit { .. }))
            .expect("a copy finding");
        assert_eq!(
            copies.rule,
            Rule::CopyLimit {
                name: "Lillia - Fae Fawn".into(),
                have: 4
            }
        );
        assert_eq!(copies.cite, "103.2.b");
        assert_eq!(
            copies.cards,
            vec!["unl-082-219".to_string(), "unl-082a-219".to_string()]
        );
        assert_eq!(report.meter.main, (40, 40));
    }

    #[test]
    fn signatures_are_capped_and_must_carry_the_legends_tag() {
        let mut deck = legal_deck();
        let sig = |name: &str, id: &str, tags: &[&str]| {
            let mut card = card(name, id, "Spell", &["Calm"], tags);
            card.signature = true;
            card
        };
        deck.main_deck[0] = entry(sig("Sprite Burst", "unl-069-219", &["Lillia"]), 3);
        deck.main_deck[1] = entry(sig("Showstopper", "ogn-270-298", &["Sett"]), 3);
        deck.main_deck[2] = entry(sig("Untagged Sig", "ogn-271-298", &[]), 3);
        let report = check(&deck, Mode::Standard);
        let cap = report
            .findings
            .iter()
            .find(|finding| matches!(finding.rule, Rule::SignatureCap { .. }))
            .expect("a cap finding");
        assert_eq!(cap.rule, Rule::SignatureCap { have: 9 });
        assert_eq!(cap.cite, "103.2.d.1");
        assert_eq!(cap.zone, Zone::Deck);
        assert_eq!(cap.cards.len(), 3);
        let showstopper = report
            .findings
            .iter()
            .find(|finding| {
                finding.rule
                    == Rule::SignatureTag {
                        name: "Showstopper".into(),
                    }
            })
            .expect("a tag finding");
        assert_eq!(showstopper.grade, Grade::Break);
        assert_eq!(showstopper.cite, "103.2.d.2");
        let untagged = report
            .findings
            .iter()
            .find(|finding| {
                finding.rule
                    == Rule::SignatureTag {
                        name: "Untagged Sig".into(),
                    }
            })
            .expect("an unverified finding");
        assert_eq!(untagged.grade, Grade::Unverified);
        assert_eq!(report.meter.signatures, (9, 3));
        assert_eq!(report.verdict, Verdict::Broken(2));
    }

    #[test]
    fn runes_must_number_twelve_inside_the_identity() {
        let mut deck = legal_deck();
        deck.runes[1] = entry(
            card("Fury Rune", "ogn-040-298", KIND_RUNE, &["Fury"], &[]),
            4,
        );
        let report = check(&deck, Mode::Standard);
        assert_eq!(
            rules(&report),
            vec![
                &Rule::RuneCount { have: 11 },
                &Rule::RuneOutOfIdentity {
                    name: "Fury Rune".into()
                }
            ]
        );
        assert_eq!(report.findings[1].cite, "103.3.a.1");
        assert_eq!(report.findings[1].zone, Zone::Runes);
        assert_eq!(report.meter.runes, (11, 12));
    }

    #[test]
    fn battlefields_number_three_without_repeats() {
        let mut deck = legal_deck();
        deck.battlefields[1] = entry(deck.battlefields[0].card.clone(), 1);
        let report = check(&deck, Mode::Standard);
        let finding = the_only(&report);
        assert_eq!(
            finding.rule,
            Rule::BattlefieldDuplicate {
                name: "Dusk Rose Lab".into()
            }
        );
        assert_eq!(finding.cite, "103.4.c");
        deck.battlefields.pop();
        let report = check(&deck, Mode::Standard);
        assert!(rules(&report).contains(&&Rule::BattlefieldCount { have: 2 }));
        assert_eq!(report.meter.battlefields, (2, 3));
        assert_eq!(Mode::Standard.battlefields(), 3);
    }

    #[test]
    fn a_sideboard_card_that_would_break_is_advised_and_never_counted() {
        let mut deck = legal_deck();
        deck.sideboard.push(entry(
            card("Fury Fellow", "ogn-500-298", KIND_UNIT, &["Fury"], &[]),
            2,
        ));
        deck.sideboard.push(entry(filler(0), 1));
        deck.sideboard.push(entry(filler(60), 1));
        let report = check(&deck, Mode::Standard);
        assert_eq!(report.verdict, Verdict::Legal);
        assert_eq!(report.findings.len(), 2, "{:?}", report.findings);
        assert_eq!(
            report.findings[0].rule,
            Rule::SideboardWouldBreak {
                name: "Filler 0".into()
            }
        );
        assert_eq!(report.findings[0].cite, "103.2.b");
        assert_eq!(
            report.findings[1].rule,
            Rule::SideboardWouldBreak {
                name: "Fury Fellow".into()
            }
        );
        assert_eq!(report.findings[1].cite, "103.1.b.3");
        for finding in &report.findings {
            assert_eq!(finding.grade, Grade::Advisory);
            assert_eq!(finding.zone, Zone::Sideboard);
        }
        assert_eq!(report.meter.sideboard, 4);
        assert_eq!(report.meter.main, (40, 40));
    }

    #[test]
    fn champion_tags_read_the_name_stem_and_fall_back_to_every_tag() {
        assert_eq!(champion_tags(&lillia_legend()), vec!["Lillia".to_string()]);
        let vi = card(
            "Vi - Piltover Enforcer",
            "unl-229-219",
            KIND_LEGEND,
            &[],
            &["Vi"],
        );
        assert_eq!(champion_tags(&vi), vec!["Vi".to_string()]);
        let kennen = card(
            "Yordle, Kennen - Heart of the Tempest",
            "sfd-190-221",
            KIND_LEGEND,
            &[],
            &["Yordle", "Kennen"],
        );
        assert_eq!(
            champion_tags(&kennen),
            vec!["Kennen".to_string()],
            "a species before the comma is not the champion tag"
        );
        assert_eq!(
            champion_name("Yordle, Kennen - Heart of the Tempest"),
            "Kennen"
        );
        assert_eq!(champion_name("Vi - Piltover Enforcer"), "Vi");
        let renamed = card(
            "Curator of the Sands",
            "ven-145-166",
            KIND_LEGEND,
            &[],
            &["Nasus"],
        );
        assert_eq!(champion_tags(&renamed), vec!["Nasus".to_string()]);
        let cased = card(
            "Rek'sai - Void Burrower",
            "sfd-001-221",
            KIND_LEGEND,
            &[],
            &["Rek'Sai"],
        );
        assert_eq!(champion_tags(&cased), vec!["Rek'Sai".to_string()]);
        let untagged = card("Anyone - At All", "x-1", KIND_LEGEND, &[], &[]);
        assert_eq!(
            champion_tags(&untagged),
            vec!["Anyone".to_string()],
            "a catalog without tags still names the champion by the legend's stem"
        );
        let fawn = card("Anyone - Fae Fawn", "x-2", KIND_UNIT, &[], &[]);
        assert!(fits_champion(&fawn, &champion_tags(&untagged)));
        let plain = card("Anyone", "x-3", KIND_UNIT, &[], &[]);
        assert!(
            !fits_champion(&plain, &champion_tags(&untagged)),
            "a unit named only by the stem is not a champion unit"
        );
        assert!(fits_champion(
            &lillia_champion(),
            &champion_tags(&lillia_legend())
        ));
    }

    #[test]
    fn a_champion_unit_is_a_unit_tagged_with_its_own_name_stem() {
        assert_eq!(champion_unit(&lillia_champion()), Some(true));
        let comma = card(
            "Nasus, Ascended",
            "ven-046-166",
            KIND_UNIT,
            &["Order"],
            &["Shurima", "Nasus"],
        );
        assert_eq!(champion_unit(&comma), Some(true));
        let plain = card(
            "Allay, Eager Admirer",
            "sfd-100-221",
            KIND_UNIT,
            &["Calm"],
            &["Yordle", "Bandle City"],
        );
        assert_eq!(champion_unit(&plain), Some(false));
        let untagged = card("Lonely Poro", "sfd-036-221", KIND_UNIT, &["Calm"], &[]);
        assert_eq!(champion_unit(&untagged), None);
        let mut daisy = card("Daisy!", "unl-196-219", KIND_UNIT, &["Calm"], &["Ivern"]);
        daisy.signature = true;
        assert_eq!(champion_unit(&daisy), Some(false));
    }

    #[test]
    fn rune_splits_share_twelve_with_the_remainder_first() {
        assert_eq!(rune_split(1, 12), vec![12]);
        assert_eq!(rune_split(2, 12), vec![6, 6]);
        assert_eq!(rune_split(3, 12), vec![4, 4, 4]);
        assert_eq!(rune_split(2, 7), vec![4, 3]);
        assert_eq!(rune_split(5, 12), vec![4, 2, 2, 2, 2]);
        assert!(rune_split(0, 12).is_empty());
    }

    #[test]
    fn a_report_names_the_worst_grade_on_a_card() {
        let mut deck = legal_deck();
        deck.main_deck[0] = entry(
            card("Fury Fellow", "ogn-500-298", KIND_UNIT, &["Fury"], &[]),
            3,
        );
        let report = check(&deck, Mode::Standard);
        assert_eq!(report.flagged("ogn-500-298"), Some(Grade::Break));
        assert_eq!(report.flagged("ogn-001-298"), None);
        assert_eq!(report.breaks(), 1);
    }
}
