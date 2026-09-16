use crate::cbor::Writer;
pub use crate::table::ZoneSummary;
use crate::table::{parse_state, private_faces, unveil, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub plugin_state: Vec<u8>,
    pub players: u8,
    pub seat: u8,
    pub zones: Vec<ZoneSummary>,
    pub table: Snapshot,
}

pub fn parse(bytes: &[u8]) -> Option<Request> {
    let mut reader = crate::cbor::Reader::new(bytes);
    let mut plugin_state = None;
    let mut table = None;
    let mut seat = None;
    let mut faces = Vec::new();
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "plugin_state" => plugin_state = Some(reader.bytes()?.to_vec()),
            "state" => table = Some(parse_state(&mut reader)?),
            "seat" => seat = Some(u8::try_from(reader.unsigned()?).ok()?),
            "faces" => faces = private_faces(&mut reader)?,
            _ => reader.skip()?,
        }
    }
    let mut table = table?;
    unveil(&mut table, &faces);
    Some(Request {
        plugin_state: plugin_state?,
        players: table.players,
        seat: seat?,
        zones: table.zones.clone(),
        table,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Plain,
    Commit {
        roll: u32,
    },
    Reveal {
        roll: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Affordance {
    pub label: String,
    pub hotkey: Option<String>,
    pub enabled: bool,
    pub kind: Kind,
    pub data: Vec<u8>,
    pub card: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PromptSummary {
    pub seat: u8,
    pub why: String,
    pub min: u8,
    pub max: u8,
    pub picked: u8,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LegalKind {
    Play { accelerate: bool },
    March,
    Activate { ability: u8 },
    React,
    Answer,
    Hide,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Legal {
    pub card: u32,
    pub kinds: Vec<LegalKind>,
    pub zones: Vec<u16>,
    pub hidden: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Card(u32),
    Item(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetRef {
    Card(u32),
    Seat(u8),
    Zone(u16),
    Item(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArrowKind {
    Spell,
    Ability,
    Attack,
    Counter,
    Combat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Arrow {
    pub from: Origin,
    pub to: TargetRef,
    pub kind: ArrowKind,
}

impl LegalKind {
    fn write(self, writer: &mut Writer) {
        match self {
            LegalKind::Play { accelerate } => {
                writer.map(1);
                writer.text("Play");
                writer.map(1);
                writer.text("accelerate");
                writer.bool(accelerate);
            }
            LegalKind::March => writer.text("March"),
            LegalKind::Activate { ability } => {
                writer.map(1);
                writer.text("Activate");
                writer.map(1);
                writer.text("ability");
                writer.unsigned(u64::from(ability));
            }
            LegalKind::React => writer.text("React"),
            LegalKind::Answer => writer.text("Answer"),
            LegalKind::Hide => writer.text("Hide"),
        }
    }
}

impl Legal {
    fn write(&self, writer: &mut Writer) {
        writer.map(2 + !self.zones.is_empty() as usize + !self.hidden.is_empty() as usize);
        writer.text("card");
        writer.unsigned(u64::from(self.card));
        writer.text("kinds");
        writer.array(self.kinds.len());
        for kind in &self.kinds {
            kind.write(writer);
        }
        if !self.zones.is_empty() {
            writer.text("zones");
            writer.array(self.zones.len());
            for zone in &self.zones {
                writer.unsigned(u64::from(*zone));
            }
        }
        if !self.hidden.is_empty() {
            writer.text("hidden");
            writer.array(self.hidden.len());
            for zone in &self.hidden {
                writer.unsigned(u64::from(*zone));
            }
        }
    }
}

impl Origin {
    fn write(self, writer: &mut Writer) {
        writer.map(1);
        match self {
            Origin::Card(card) => {
                writer.text("Card");
                writer.unsigned(u64::from(card));
            }
            Origin::Item(item) => {
                writer.text("Item");
                writer.unsigned(u64::from(item));
            }
        }
    }
}

impl TargetRef {
    fn write(self, writer: &mut Writer) {
        writer.map(1);
        match self {
            TargetRef::Card(card) => {
                writer.text("Card");
                writer.unsigned(u64::from(card));
            }
            TargetRef::Seat(seat) => {
                writer.text("Seat");
                writer.unsigned(u64::from(seat));
            }
            TargetRef::Zone(zone) => {
                writer.text("Zone");
                writer.unsigned(u64::from(zone));
            }
            TargetRef::Item(item) => {
                writer.text("Item");
                writer.unsigned(u64::from(item));
            }
        }
    }
}

impl ArrowKind {
    fn name(self) -> &'static str {
        match self {
            ArrowKind::Spell => "Spell",
            ArrowKind::Ability => "Ability",
            ArrowKind::Attack => "Attack",
            ArrowKind::Counter => "Counter",
            ArrowKind::Combat => "Combat",
        }
    }
}

impl Arrow {
    fn write(self, writer: &mut Writer) {
        writer.map(3);
        writer.text("from");
        self.from.write(writer);
        writer.text("to");
        self.to.write(writer);
        writer.text("kind");
        writer.text(self.kind.name());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ChainRow {
    pub item: u16,
    pub card: Option<u32>,
    pub seat: u8,
}

impl ChainRow {
    fn write(self, writer: &mut Writer) {
        writer.map(2 + self.card.is_some() as usize);
        writer.text("item");
        writer.unsigned(u64::from(self.item));
        if let Some(card) = self.card {
            writer.text("card");
            writer.unsigned(u64::from(card));
        }
        writer.text("seat");
        writer.unsigned(u64::from(self.seat));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TurnInfo {
    pub number: u32,
    pub seat: u8,
    pub phase: String,
    pub phases: Vec<String>,
    pub mode: String,
}

impl TurnInfo {
    fn write(&self, writer: &mut Writer) {
        writer.map(4 + !self.phases.is_empty() as usize);
        writer.text("number");
        writer.unsigned(u64::from(self.number));
        writer.text("seat");
        writer.unsigned(u64::from(self.seat));
        writer.text("phase");
        writer.text(&self.phase);
        if !self.phases.is_empty() {
            writer.text("phases");
            writer.array(self.phases.len());
            for phase in &self.phases {
                writer.text(phase);
            }
        }
        writer.text("mode");
        writer.text(&self.mode);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeatInfo {
    pub seat: u8,
    pub points: i32,
    pub victory: i32,
    pub xp: i32,
    pub hand: u32,
    pub deck: u32,
    pub runes_ready: u32,
    pub runes_total: u32,
}

impl SeatInfo {
    fn write(self, writer: &mut Writer) {
        writer.map(8);
        writer.text("seat");
        writer.unsigned(u64::from(self.seat));
        writer.text("points");
        writer.signed(i64::from(self.points));
        writer.text("victory");
        writer.signed(i64::from(self.victory));
        writer.text("xp");
        writer.signed(i64::from(self.xp));
        writer.text("hand");
        writer.unsigned(u64::from(self.hand));
        writer.text("deck");
        writer.unsigned(u64::from(self.deck));
        writer.text("runes_ready");
        writer.unsigned(u64::from(self.runes_ready));
        writer.text("runes_total");
        writer.unsigned(u64::from(self.runes_total));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Waiting {
    pub seat: Option<u8>,
    pub what: String,
}

impl Waiting {
    fn write(&self, writer: &mut Writer) {
        writer.map(1 + self.seat.is_some() as usize);
        if let Some(seat) = self.seat {
            writer.text("seat");
            writer.unsigned(u64::from(seat));
        }
        writer.text("what");
        writer.text(&self.what);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PluginView {
    pub status: Vec<String>,
    pub affordances: Vec<Affordance>,
    pub winner: Option<u8>,
    pub prompt: Option<PromptSummary>,
    pub legal: Vec<Legal>,
    pub arrows: Vec<Arrow>,
    pub chain: Vec<ChainRow>,
    pub turn: Option<TurnInfo>,
    pub seats: Vec<SeatInfo>,
    pub waiting: Option<Waiting>,
    pub narration: Vec<String>,
    pub primary: Option<u16>,
    pub hidden: Vec<u16>,
}

impl PluginView {
    pub fn line(mut self, text: impl Into<String>) -> Self {
        self.status.push(text.into());
        self
    }

    pub fn turn(mut self, turn: TurnInfo) -> Self {
        self.turn = Some(turn);
        self
    }

    pub fn seat(mut self, seat: SeatInfo) -> Self {
        self.seats.retain(|held| held.seat != seat.seat);
        self.seats.push(seat);
        self.seats.sort_by_key(|held| held.seat);
        self
    }

    pub fn waiting(mut self, seat: Option<u8>, what: impl Into<String>) -> Self {
        self.waiting = Some(Waiting {
            seat,
            what: what.into(),
        });
        self
    }

    pub fn narrate(mut self, line: impl Into<String>) -> Self {
        self.narration.push(line.into());
        self
    }

    pub fn primary(mut self, index: usize) -> Self {
        self.primary = u16::try_from(index).ok();
        self
    }

    pub fn primary_last(self) -> Self {
        let last = self.affordances.len().checked_sub(1);
        match last {
            Some(index) => self.primary(index),
            None => self,
        }
    }

    pub fn offer_hidden(mut self, label: impl Into<String>, data: Vec<u8>) -> Self {
        let index = self.affordances.len();
        self = self.offer(label, None, data);
        if let Ok(index) = u16::try_from(index) {
            self.hidden.push(index);
        }
        self
    }

    pub fn won_by(mut self, seat: u8) -> Self {
        self.winner = Some(seat);
        self
    }

    pub fn prompt(mut self, summary: PromptSummary) -> Self {
        self.prompt = Some(summary);
        self
    }

    pub fn legal(mut self, rows: Vec<Legal>) -> Self {
        self.legal = rows;
        self
    }

    pub fn arrows(mut self, arrows: Vec<Arrow>) -> Self {
        self.arrows = arrows;
        self
    }

    pub fn chain(mut self, rows: Vec<ChainRow>) -> Self {
        self.chain = rows;
        self
    }

    pub fn offer(mut self, label: impl Into<String>, hotkey: Option<&str>, data: Vec<u8>) -> Self {
        self.affordances.push(Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: Kind::Plain,
            data,
            card: None,
        });
        self
    }

    pub fn offer_card(self, label: impl Into<String>, card: u32, data: Vec<u8>) -> Self {
        self.offer_card_enabled(label, card, data, true)
    }

    pub fn offer_card_enabled(
        mut self,
        label: impl Into<String>,
        card: u32,
        data: Vec<u8>,
        enabled: bool,
    ) -> Self {
        self.affordances.push(Affordance {
            label: label.into(),
            hotkey: None,
            enabled,
            kind: Kind::Plain,
            data,
            card: Some(card),
        });
        self
    }

    pub fn offer_kind(mut self, label: impl Into<String>, kind: Kind, data: Vec<u8>) -> Self {
        self.affordances.push(Affordance {
            label: label.into(),
            hotkey: None,
            enabled: true,
            kind,
            data,
            card: None,
        });
        self
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(
            2 + self.winner.is_some() as usize
                + self.prompt.is_some() as usize
                + !self.legal.is_empty() as usize
                + !self.arrows.is_empty() as usize
                + !self.chain.is_empty() as usize
                + self.turn.is_some() as usize
                + !self.seats.is_empty() as usize
                + self.waiting.is_some() as usize
                + !self.narration.is_empty() as usize
                + self.primary.is_some() as usize
                + !self.hidden.is_empty() as usize,
        );
        if let Some(winner) = self.winner {
            writer.text("winner");
            writer.unsigned(u64::from(winner));
        }
        if let Some(prompt) = &self.prompt {
            writer.text("prompt");
            writer.map(6);
            writer.text("seat");
            writer.unsigned(u64::from(prompt.seat));
            writer.text("why");
            writer.text(&prompt.why);
            writer.text("min");
            writer.unsigned(u64::from(prompt.min));
            writer.text("max");
            writer.unsigned(u64::from(prompt.max));
            writer.text("picked");
            writer.unsigned(u64::from(prompt.picked));
            writer.text("optional");
            writer.bool(prompt.optional);
        }
        writer.text("status");
        writer.array(self.status.len());
        for line in &self.status {
            writer.text(line);
        }
        writer.text("affordances");
        writer.array(self.affordances.len());
        for affordance in &self.affordances {
            writer.map(5 + affordance.card.is_some() as usize);
            writer.text("label");
            writer.text(&affordance.label);
            writer.text("hotkey");
            match &affordance.hotkey {
                Some(key) => writer.text(key),
                None => writer.null(),
            }
            writer.text("enabled");
            writer.bool(affordance.enabled);
            writer.text("kind");
            match affordance.kind {
                Kind::Plain => writer.text("Plain"),
                Kind::Commit { roll } | Kind::Reveal { roll } => {
                    writer.map(1);
                    writer.text(match affordance.kind {
                        Kind::Commit { .. } => "Commit",
                        _ => "Reveal",
                    });
                    writer.map(1);
                    writer.text("roll");
                    writer.unsigned(u64::from(roll));
                }
            }
            writer.text("data");
            writer.bytes(&affordance.data);
            if let Some(card) = affordance.card {
                writer.text("card");
                writer.unsigned(u64::from(card));
            }
        }
        if !self.legal.is_empty() {
            writer.text("legal");
            writer.array(self.legal.len());
            for row in &self.legal {
                row.write(&mut writer);
            }
        }
        if !self.arrows.is_empty() {
            writer.text("arrows");
            writer.array(self.arrows.len());
            for arrow in &self.arrows {
                arrow.write(&mut writer);
            }
        }
        if !self.chain.is_empty() {
            writer.text("chain");
            writer.array(self.chain.len());
            for row in &self.chain {
                row.write(&mut writer);
            }
        }
        if let Some(turn) = &self.turn {
            writer.text("turn");
            turn.write(&mut writer);
        }
        if !self.seats.is_empty() {
            writer.text("seats");
            writer.array(self.seats.len());
            for seat in &self.seats {
                seat.write(&mut writer);
            }
        }
        if let Some(waiting) = &self.waiting {
            writer.text("waiting");
            waiting.write(&mut writer);
        }
        if !self.narration.is_empty() {
            writer.text("narration");
            writer.array(self.narration.len());
            for line in &self.narration {
                writer.text(line);
            }
        }
        if let Some(primary) = self.primary {
            writer.text("primary");
            writer.unsigned(u64::from(primary));
        }
        if !self.hidden.is_empty() {
            writer.text("hidden");
            writer.array(self.hidden.len());
            for index in &self.hidden {
                writer.unsigned(u64::from(*index));
            }
        }
        writer.finish()
    }
}

pub fn nothing(_request: &[u8]) -> Vec<u8> {
    PluginView::default().encode()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::cbor::Writer;
    use crate::decide::fixtures::seats;

    pub fn zone(writer: &mut Writer, id: u64, kind: &str, owner: &str, label: &str) {
        zone_seen(writer, id, kind, owner, "All", label)
    }

    pub fn zone_seen(
        writer: &mut Writer,
        id: u64,
        kind: &str,
        owner: &str,
        visibility: &str,
        label: &str,
    ) {
        writer.map(6);
        writer.text("id");
        writer.unsigned(id);
        writer.text("name");
        writer.text("n");
        writer.text("kind");
        writer.text(kind);
        writer.text("owner");
        writer.text(owner);
        writer.text("visibility");
        writer.text(visibility);
        writer.text("label");
        writer.text(label);
    }

    pub fn request(plugin_state: &[u8], players: usize, seat: u8, battlefields: &[u64]) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("plugin_state");
        writer.bytes(plugin_state);
        writer.text("state");
        writer.map(2);
        seats(&mut writer, players);
        writer.text("zones");
        writer.array(battlefields.len() + 1);
        zone_seen(&mut writer, 0, "Hand", "PerSeat", "Owner", "Hand");
        for id in battlefields {
            zone(
                &mut writer,
                *id,
                "Battlefield",
                "Shared",
                &format!("Battlefield {id}"),
            );
        }
        writer.text("seat");
        writer.unsigned(seat as u64);
        writer.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::request;
    use super::*;
    use crate::cbor::{Item, Reader};

    #[test]
    fn a_view_request_lists_the_shared_battlefields() {
        let parsed = parse(&request(&[1], 2, 1, &[9, 10])).unwrap();
        assert_eq!(parsed.seat, 1);
        assert_eq!(parsed.players, 2);
        assert_eq!(parsed.plugin_state, vec![1]);
        let contested: Vec<u16> = parsed
            .zones
            .iter()
            .filter(|zone| zone.shared && zone.battlefield)
            .map(|zone| zone.id)
            .collect();
        assert_eq!(contested, [9, 10]);
        assert_eq!(parsed.zones[0].label, "Hand");
        assert!(!parsed.zones[0].shared);
        assert_eq!(
            parsed.zones[0].visibility,
            crate::table::ZoneVisibility::Owner
        );
    }

    #[test]
    fn a_view_encodes_status_lines_and_affordances_in_declaration_order() {
        let view = PluginView::default()
            .line("turn 1")
            .offer("end turn", Some("space"), vec![2]);
        let bytes = view.encode();
        assert_eq!(bytes[0], 0xa2);
        assert!(bytes.windows(8).any(|w| w == b"end turn"));
        assert!(!bytes.windows(4).any(|w| w == b"card"));
        assert!(!bytes.windows(6).any(|w| w == b"prompt"));
        assert_eq!(nothing(&[]), PluginView::default().encode());
    }

    fn keys_of(reader: &mut Reader) -> Vec<String> {
        let mut keys = Vec::new();
        for _ in 0..reader.map_len().unwrap() {
            keys.push(reader.key().unwrap().to_string());
            reader.skip().unwrap();
        }
        keys
    }

    #[test]
    fn an_empty_legal_list_and_no_arrows_write_neither_key() {
        let bytes = PluginView::default()
            .line("turn 1")
            .legal(Vec::new())
            .arrows(Vec::new())
            .encode();
        assert_eq!(bytes[0], 0xa2);
        assert!(!bytes.windows(5).any(|w| w == b"legal"));
        assert!(!bytes.windows(6).any(|w| w == b"arrows"));
    }

    #[test]
    fn legal_rows_and_arrows_are_written_under_their_keys_in_declaration_order() {
        let view = PluginView::default()
            .line("turn 3")
            .legal(vec![
                Legal {
                    card: 70,
                    kinds: vec![LegalKind::Play { accelerate: true }, LegalKind::React],
                    zones: vec![8, 12],
                    hidden: Vec::new(),
                },
                Legal {
                    card: 75,
                    kinds: vec![
                        LegalKind::March,
                        LegalKind::Activate { ability: 2 },
                        LegalKind::Answer,
                    ],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                },
                Legal {
                    card: 76,
                    kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
                    zones: vec![12],
                    hidden: vec![9, 10],
                },
            ])
            .arrows(vec![
                Arrow {
                    from: Origin::Card(71),
                    to: TargetRef::Card(81),
                    kind: ArrowKind::Spell,
                },
                Arrow {
                    from: Origin::Item(4),
                    to: TargetRef::Seat(1),
                    kind: ArrowKind::Ability,
                },
                Arrow {
                    from: Origin::Card(50),
                    to: TargetRef::Zone(9),
                    kind: ArrowKind::Attack,
                },
                Arrow {
                    from: Origin::Card(52),
                    to: TargetRef::Item(4),
                    kind: ArrowKind::Counter,
                },
                Arrow {
                    from: Origin::Card(50),
                    to: TargetRef::Card(60),
                    kind: ArrowKind::Combat,
                },
            ]);
        let bytes = view.encode();
        assert_eq!(bytes[0], 0xa4);
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.map_len(), Some(4));
        assert_eq!(reader.key(), Some("status"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("affordances"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("legal"));
        assert_eq!(reader.array_len(), Some(3));
        assert_eq!(keys_of(&mut reader), ["card", "kinds", "zones"]);
        assert_eq!(
            keys_of(&mut reader),
            ["card", "kinds"],
            "a row with no destinations omits the key"
        );
        assert_eq!(
            keys_of(&mut reader),
            ["card", "kinds", "zones", "hidden"],
            "the hide destinations ride under their own key after the plays"
        );
        assert_eq!(reader.key(), Some("arrows"));
        assert_eq!(reader.array_len(), Some(5));
        for _ in 0..5 {
            assert_eq!(keys_of(&mut reader), ["from", "to", "kind"]);
        }
        assert_eq!(reader.item(), None);
    }

    #[test]
    fn a_legal_kind_writes_a_bare_name_or_a_one_key_map_for_its_payload() {
        let named = |kind: LegalKind| {
            let mut writer = Writer::new();
            kind.write(&mut writer);
            writer.finish()
        };
        assert_eq!(named(LegalKind::March), {
            let mut writer = Writer::new();
            writer.text("March");
            writer.finish()
        });
        let play = named(LegalKind::Play { accelerate: true });
        let mut reader = Reader::new(&play);
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("Play"));
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("accelerate"));
        assert_eq!(reader.item(), Some(Item::Simple(21)));
        let activate = named(LegalKind::Activate { ability: 2 });
        let mut reader = Reader::new(&activate);
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("Activate"));
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("ability"));
        assert_eq!(reader.unsigned(), Some(2));
    }

    #[test]
    fn the_structured_fields_ride_after_the_chain_under_their_own_keys() {
        let view = PluginView::default()
            .line("turn 3 · {seat 1} · action phase · rules enforced")
            .offer("pass", Some("w"), vec![9])
            .offer_hidden("concede", vec![14])
            .primary(0)
            .turn(TurnInfo {
                number: 3,
                seat: 1,
                phase: "action phase".into(),
                phases: vec!["setup".into(), "action phase".into()],
                mode: "rules enforced".into(),
            })
            .seat(SeatInfo {
                seat: 1,
                points: 0,
                victory: 8,
                xp: 0,
                hand: 3,
                deck: 30,
                runes_ready: 0,
                runes_total: 2,
            })
            .seat(SeatInfo {
                seat: 0,
                points: 2,
                victory: 8,
                xp: 1,
                hand: 5,
                deck: 31,
                runes_ready: 3,
                runes_total: 6,
            })
            .waiting(Some(1), "their action phase")
            .narrate("{seat 0} plays {card 4}");
        assert_eq!(view.hidden, [1]);
        assert_eq!(view.affordances[1].label, "concede");
        assert_eq!(
            view.seats.iter().map(|seat| seat.seat).collect::<Vec<u8>>(),
            [0, 1],
            "seats sort by number whatever the order they were added in"
        );
        let bytes = view.encode();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.map_len(), Some(8));
        assert_eq!(reader.key(), Some("status"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("affordances"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("turn"));
        assert_eq!(
            keys_of(&mut reader),
            ["number", "seat", "phase", "phases", "mode"]
        );
        assert_eq!(reader.key(), Some("seats"));
        assert_eq!(reader.array_len(), Some(2));
        for _ in 0..2 {
            assert_eq!(
                keys_of(&mut reader),
                [
                    "seat",
                    "points",
                    "victory",
                    "xp",
                    "hand",
                    "deck",
                    "runes_ready",
                    "runes_total"
                ]
            );
        }
        assert_eq!(reader.key(), Some("waiting"));
        assert_eq!(keys_of(&mut reader), ["seat", "what"]);
        assert_eq!(reader.key(), Some("narration"));
        assert_eq!(reader.array_len(), Some(1));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("primary"));
        assert_eq!(reader.unsigned(), Some(0));
        assert_eq!(reader.key(), Some("hidden"));
        assert_eq!(reader.array_len(), Some(1));
        assert_eq!(reader.unsigned(), Some(1));
        assert_eq!(reader.item(), None);
        let every_seat = PluginView::default().waiting(None, "every seat to roll");
        let bytes = every_seat.encode();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.map_len(), Some(3));
        reader.key().unwrap();
        reader.skip().unwrap();
        reader.key().unwrap();
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("waiting"));
        assert_eq!(keys_of(&mut reader), ["what"]);
        assert_eq!(
            PluginView::default().primary_last().primary,
            None,
            "no affordance, no primary"
        );
        assert_eq!(
            PluginView::default()
                .offer("end turn", None, vec![2])
                .primary_last()
                .primary,
            Some(0)
        );
    }

    #[test]
    fn a_card_affordance_and_a_prompt_summary_are_written_under_their_keys() {
        let view = PluginView::default()
            .line("set aside up to 2 cards")
            .offer_card("set aside {card 3}", 3, vec![10, 1, 0, 0, 0])
            .offer("keep", None, vec![10, 1, 0, 1, 0])
            .prompt(PromptSummary {
                seat: 1,
                why: "mulligan".into(),
                min: 0,
                max: 2,
                picked: 1,
                optional: true,
            });
        let bytes = view.encode();
        assert_eq!(bytes[0], 0xa3);
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.map_len(), Some(3));
        assert_eq!(reader.key(), Some("prompt"));
        assert_eq!(
            keys_of(&mut reader),
            ["seat", "why", "min", "max", "picked", "optional"]
        );
        assert_eq!(reader.key(), Some("status"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("affordances"));
        assert_eq!(reader.array_len(), Some(2));
        let with_card = keys_of(&mut reader);
        assert_eq!(
            with_card,
            ["label", "hotkey", "enabled", "kind", "data", "card"]
        );
        assert_eq!(
            keys_of(&mut reader),
            ["label", "hotkey", "enabled", "kind", "data"]
        );
        assert_eq!(reader.item(), None);
        assert_eq!(view.affordances[0].card, Some(3));
        assert!(matches!(
            Reader::new(&bytes[bytes.len() - 1..]).item(),
            Some(Item::Unsigned(_) | Item::Bytes(_))
        ));
    }
}
