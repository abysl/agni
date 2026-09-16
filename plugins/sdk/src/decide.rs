use crate::cbor::{Item, Reader, Writer};
use crate::table::{
    parse_state, signed, target, text, write_zone, zone_id, Face, Snapshot, Target,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Game(Vec<u8>),
    Move {
        card: u32,
        to: Option<u16>,
        seat: u8,
        index: u32,
        hidden: bool,
    },
    Spawn {
        face: Face,
        zone: Option<u16>,
        seat: u8,
    },
    Annotate {
        card: u32,
        key: String,
        value: Option<Vec<u8>>,
    },
    Counter {
        target: Target,
        counter: u16,
        delta: i32,
    },
    Reveal {
        card: u32,
        face: Face,
    },
    Deal {
        zone: Option<u16>,
        count: u32,
    },
    Reset,
    Clear {
        seat: u8,
    },
    Join,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub plugin_state: Vec<u8>,
    pub players: u8,
    pub seat: u8,
    pub action: Action,
    pub table: Snapshot,
}

pub fn parse(bytes: &[u8]) -> Option<Request> {
    let mut reader = Reader::new(bytes);
    let mut plugin_state = None;
    let mut table = None;
    let mut seat = None;
    let mut action = None;
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "plugin_state" => plugin_state = Some(reader.bytes()?.to_vec()),
            "state" => table = Some(parse_state(&mut reader)?),
            "entry" => {
                let (entry_seat, entry_action) = entry(&mut reader)?;
                seat = Some(entry_seat);
                action = Some(entry_action);
            }
            _ => reader.skip()?,
        }
    }
    let table = table?;
    Some(Request {
        plugin_state: plugin_state?,
        players: table.players,
        seat: seat?,
        action: action?,
        table,
    })
}

pub(crate) fn skip_array(reader: &mut Reader) -> Option<usize> {
    let len = reader.array_len()?;
    for _ in 0..len {
        reader.skip()?;
    }
    Some(len)
}

fn entry(reader: &mut Reader) -> Option<(u8, Action)> {
    let mut seat = None;
    let mut action = None;
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "seat" => seat = Some(u8::try_from(reader.unsigned()?).ok()?),
            "action" => action = Some(action_of(reader)?),
            _ => reader.skip()?,
        }
    }
    Some((seat?, action?))
}

fn card_id(reader: &mut Reader) -> Option<u32> {
    u32::try_from(reader.unsigned()?).ok()
}

fn action_of(reader: &mut Reader) -> Option<Action> {
    match reader.item()? {
        Item::Text("Reset") => Some(Action::Reset),
        Item::Text(_) => Some(Action::Other),
        Item::Map(1) => match reader.key()? {
            "Game" => {
                let mut data = None;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "data" => data = Some(reader.bytes()?.to_vec()),
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Game(data?))
            }
            "Spawn" => {
                let mut face = None;
                let mut zone = None;
                let mut seat = 0;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "face" => face = Some(Face::read(reader)?),
                        "to" => zone = zone_id(reader)?,
                        "seat" => seat = u8::try_from(reader.unsigned()?).ok()?,
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Spawn {
                    face: face?,
                    zone,
                    seat,
                })
            }
            "Move" => {
                let mut card = None;
                let mut to = None;
                let mut seat = 0;
                let mut index = 0;
                let mut hidden = false;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "card" => card = Some(card_id(reader)?),
                        "to" => to = zone_id(reader)?,
                        "seat" => seat = u8::try_from(reader.unsigned()?).ok()?,
                        "index" => index = u32::try_from(reader.unsigned()?).ok()?,
                        "hidden" => hidden = reader.boolean()?,
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Move {
                    card: card?,
                    to,
                    seat,
                    index,
                    hidden,
                })
            }
            "Annotate" => {
                let mut card = None;
                let mut key = None;
                let mut value = None;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "card" => card = Some(card_id(reader)?),
                        "key" => key = Some(text(reader)?),
                        "value" => {
                            value = match reader.item()? {
                                Item::Bytes(bytes) => Some(bytes.to_vec()),
                                Item::Simple(22) => None,
                                _ => return None,
                            }
                        }
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Annotate {
                    card: card?,
                    key: key?,
                    value,
                })
            }
            "Counter" => {
                let mut held = None;
                let mut counter = None;
                let mut delta = 0;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "target" => held = Some(target(reader)?),
                        "counter" => counter = Some(u16::try_from(reader.unsigned()?).ok()?),
                        "delta" => delta = signed(reader)?,
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Counter {
                    target: held?,
                    counter: counter?,
                    delta,
                })
            }
            "Reveal" => {
                let mut card = None;
                let mut face = None;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "card" => card = Some(card_id(reader)?),
                        "face" => face = Some(Face::read(reader)?),
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Reveal {
                    card: card?,
                    face: face?,
                })
            }
            "Deal" => {
                let mut zone = None;
                let mut count = 0;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "cards" => count = u32::try_from(skip_array(reader)?).ok()?,
                        "to" => zone = zone_id(reader)?,
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Deal { zone, count })
            }
            "Clear" => {
                let mut seat = None;
                for _ in 0..reader.map_len()? {
                    match reader.key()? {
                        "seat" => seat = Some(u8::try_from(reader.unsigned()?).ok()?),
                        _ => reader.skip()?,
                    }
                }
                Some(Action::Clear { seat: seat? })
            }
            "Join" => {
                reader.skip()?;
                Some(Action::Join)
            }
            _ => {
                reader.skip()?;
                Some(Action::Other)
            }
        },
        _ => None,
    }
}

pub const BOTTOM: u32 = 0;
pub const TOP: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Move {
        card: u32,
        zone: u16,
        seat: u8,
        index: u32,
    },
    Annotate {
        card: u32,
        key: String,
        value: Option<Vec<u8>>,
    },
    Counter {
        target: crate::table::Target,
        counter: u16,
        delta: i32,
    },
    Spawn {
        face: Face,
        zone: u16,
        seat: u8,
        owner: Option<u8>,
    },
    Despawn {
        card: u32,
    },
    Reveal {
        card: u32,
    },
    Peek {
        card: u32,
        seat: u8,
    },
    Conceal {
        card: u32,
    },
}

impl Effect {
    pub fn exhaust(card: u32) -> Self {
        Effect::Annotate {
            card,
            key: "exhausted".into(),
            value: Some(vec![1]),
        }
    }

    pub fn ready(card: u32) -> Self {
        Effect::Annotate {
            card,
            key: "exhausted".into(),
            value: None,
        }
    }

    pub fn score(seat: u8, counter: u16, delta: i32) -> Self {
        Effect::Counter {
            target: crate::table::Target::Seat(seat),
            counter,
            delta,
        }
    }

    fn write(&self, writer: &mut Writer) {
        writer.map(1);
        match self {
            Effect::Move {
                card,
                zone,
                seat,
                index,
            } => {
                writer.text("Move");
                writer.map(4);
                writer.text("card");
                writer.unsigned(u64::from(*card));
                writer.text("to");
                write_zone(writer, *zone);
                writer.text("seat");
                writer.unsigned(u64::from(*seat));
                writer.text("index");
                writer.unsigned(u64::from(*index));
            }
            Effect::Annotate { card, key, value } => {
                writer.text("Annotate");
                writer.map(3);
                writer.text("card");
                writer.unsigned(u64::from(*card));
                writer.text("key");
                writer.text(key);
                writer.text("value");
                match value {
                    Some(bytes) => writer.bytes(bytes),
                    None => writer.null(),
                }
            }
            Effect::Counter {
                target,
                counter,
                delta,
            } => {
                writer.text("Counter");
                writer.map(3);
                writer.text("target");
                match target {
                    crate::table::Target::Table => writer.text("Table"),
                    crate::table::Target::Seat(seat) => {
                        writer.map(1);
                        writer.text("Seat");
                        writer.unsigned(u64::from(*seat));
                    }
                    crate::table::Target::Card(card) => {
                        writer.map(1);
                        writer.text("Card");
                        writer.unsigned(u64::from(*card));
                    }
                }
                writer.text("counter");
                writer.unsigned(u64::from(*counter));
                writer.text("delta");
                writer.signed(i64::from(*delta));
            }
            Effect::Spawn {
                face,
                zone,
                seat,
                owner,
            } => {
                writer.text("Spawn");
                writer.map(3 + owner.is_some() as usize);
                writer.text("face");
                face.write(writer);
                writer.text("to");
                write_zone(writer, *zone);
                writer.text("seat");
                writer.unsigned(u64::from(*seat));
                if let Some(owner) = owner {
                    writer.text("owner");
                    writer.unsigned(u64::from(*owner));
                }
            }
            Effect::Despawn { card } => {
                writer.text("Despawn");
                writer.map(1);
                writer.text("card");
                writer.unsigned(u64::from(*card));
            }
            Effect::Reveal { card } => {
                writer.text("Reveal");
                writer.map(1);
                writer.text("card");
                writer.unsigned(u64::from(*card));
            }
            Effect::Peek { card, seat } => {
                writer.text("Peek");
                writer.map(2);
                writer.text("card");
                writer.unsigned(u64::from(*card));
                writer.text("seat");
                writer.unsigned(u64::from(*seat));
            }
            Effect::Conceal { card } => {
                writer.text("Conceal");
                writer.map(1);
                writer.text("card");
                writer.unsigned(u64::from(*card));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub accept: bool,
    pub plugin_state: Option<Vec<u8>>,
    pub effects: Vec<Effect>,
    pub reason: Option<String>,
}

impl Verdict {
    pub fn accept() -> Self {
        Self {
            accept: true,
            plugin_state: None,
            effects: Vec::new(),
            reason: None,
        }
    }

    pub fn reject() -> Self {
        Self {
            accept: false,
            plugin_state: None,
            effects: Vec::new(),
            reason: None,
        }
    }

    pub fn refuse(reason: impl Into<String>) -> Self {
        Self {
            accept: false,
            plugin_state: None,
            effects: Vec::new(),
            reason: Some(reason.into()),
        }
    }

    pub fn advance(plugin_state: Vec<u8>) -> Self {
        Self {
            accept: true,
            plugin_state: Some(plugin_state),
            effects: Vec::new(),
            reason: None,
        }
    }

    pub fn with_effects(mut self, effects: Vec<Effect>) -> Self {
        self.effects = effects;
        self
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(2 + (!self.effects.is_empty()) as usize + self.reason.is_some() as usize);
        writer.text("accept");
        writer.bool(self.accept);
        writer.text("plugin_state");
        match &self.plugin_state {
            Some(bytes) => writer.bytes(bytes),
            None => writer.null(),
        }
        if !self.effects.is_empty() {
            writer.text("effects");
            writer.array(self.effects.len());
            for effect in &self.effects {
                effect.write(&mut writer);
            }
        }
        if let Some(reason) = &self.reason {
            writer.text("reason");
            writer.text(reason);
        }
        writer.finish()
    }
}

pub fn accept_all(_request: &[u8]) -> Vec<u8> {
    Verdict::accept().encode()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::cbor::Writer;
    use crate::table::fixtures::Placed;

    pub fn seats(writer: &mut Writer, players: usize) {
        writer.text("seats");
        writer.array(players);
        for seat in 0..players {
            writer.map(2);
            writer.text("seat");
            writer.unsigned(seat as u64);
            writer.text("name");
            writer.text("p");
        }
    }

    pub fn request(
        plugin_state: &[u8],
        players: usize,
        seat: u8,
        action: &dyn Fn(&mut Writer),
    ) -> Vec<u8> {
        request_at(plugin_state, players, seat, &[], &[], action)
    }

    pub fn request_at(
        plugin_state: &[u8],
        players: usize,
        seat: u8,
        cards: &[Placed],
        exhausted: &[u64],
        action: &dyn Fn(&mut Writer),
    ) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("plugin_state");
        writer.bytes(plugin_state);
        writer.text("state");
        writer.map(4);
        crate::table::fixtures::cards(&mut writer, cards);
        seats(&mut writer, players);
        writer.text("zones");
        writer.array(0);
        crate::table::fixtures::exhausted(&mut writer, exhausted);
        writer.text("entry");
        writer.map(3);
        writer.text("seq");
        writer.unsigned(4);
        writer.text("seat");
        writer.unsigned(seat as u64);
        writer.text("action");
        action(&mut writer);
        writer.finish()
    }

    pub fn game(data: &[u8]) -> impl Fn(&mut Writer) + '_ {
        move |writer| {
            writer.map(1);
            writer.text("Game");
            writer.map(1);
            writer.text("data");
            writer.bytes(data);
        }
    }

    pub fn a_move(writer: &mut Writer) {
        move_to(3, 8, 0)(writer)
    }

    pub fn move_to(card: u64, zone: u64, seat: u64) -> impl Fn(&mut Writer) {
        move |writer| {
            writer.map(1);
            writer.text("Move");
            writer.map(4);
            writer.text("card");
            writer.unsigned(card);
            writer.text("to");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(zone);
            writer.text("seat");
            writer.unsigned(seat);
            writer.text("index");
            writer.unsigned(0);
        }
    }

    pub fn reset(writer: &mut Writer) {
        writer.text("Reset");
    }

    pub fn wrapped(variant: &'static str, body: impl Fn(&mut Writer)) -> impl Fn(&mut Writer) {
        move |writer| {
            writer.map(1);
            writer.text(variant);
            body(writer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::table::fixtures::placed;

    #[test]
    fn a_request_yields_its_seat_count_actor_and_action() {
        let bytes = request(&[9], 3, 2, &game(&[1, 2]));
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.plugin_state, vec![9]);
        assert_eq!(parsed.players, 3);
        assert_eq!(parsed.seat, 2);
        assert_eq!(parsed.action, Action::Game(vec![1, 2]));
        assert_eq!(
            parse(&request(&[], 2, 0, &a_move)).unwrap().action,
            Action::Move {
                card: 3,
                to: Some(8),
                seat: 0,
                index: 0,
                hidden: false,
            }
        );
        assert_eq!(
            parse(&request(&[], 2, 0, &reset)).unwrap().action,
            Action::Reset
        );
        assert_eq!(
            parse(&request(&[], 2, 0, &|writer| writer.text("Elsewhere")))
                .unwrap()
                .action,
            Action::Other
        );
        assert_eq!(parse(&[0xa1, 0x61]), None);
        assert_eq!(parse(&[]), None);
    }

    fn action_of(body: impl Fn(&mut Writer)) -> Action {
        parse(&request(&[], 2, 1, &body)).unwrap().action
    }

    #[test]
    fn every_table_edit_parses_to_its_own_arm() {
        let annotate = wrapped("Annotate", |writer| {
            writer.map(3);
            writer.text("card");
            writer.unsigned(5);
            writer.text("key");
            writer.text("exhausted");
            writer.text("value");
            writer.bytes(&[1]);
        });
        assert_eq!(
            action_of(annotate),
            Action::Annotate {
                card: 5,
                key: "exhausted".into(),
                value: Some(vec![1])
            }
        );
        let cleared = wrapped("Annotate", |writer| {
            writer.map(3);
            writer.text("card");
            writer.unsigned(5);
            writer.text("key");
            writer.text("hidden");
            writer.text("value");
            writer.null();
        });
        assert_eq!(
            action_of(cleared),
            Action::Annotate {
                card: 5,
                key: "hidden".into(),
                value: None
            }
        );
        let counter = wrapped("Counter", |writer| {
            writer.map(3);
            writer.text("target");
            writer.map(1);
            writer.text("Card");
            writer.unsigned(7);
            writer.text("counter");
            writer.unsigned(3);
            writer.text("delta");
            writer.signed(-2);
        });
        assert_eq!(
            action_of(counter),
            Action::Counter {
                target: Target::Card(7),
                counter: 3,
                delta: -2
            }
        );
        let table_counter = wrapped("Counter", |writer| {
            writer.map(3);
            writer.text("target");
            writer.text("Table");
            writer.text("counter");
            writer.unsigned(0);
            writer.text("delta");
            writer.unsigned(1);
        });
        assert_eq!(
            action_of(table_counter),
            Action::Counter {
                target: Target::Table,
                counter: 0,
                delta: 1
            }
        );
        let reveal = wrapped("Reveal", |writer| {
            writer.map(2);
            writer.text("card");
            writer.unsigned(9);
            writer.text("face");
            writer.map(1);
            writer.text("name");
            writer.text("Defy");
        });
        assert_eq!(
            action_of(reveal),
            Action::Reveal {
                card: 9,
                face: Face::named("Defy")
            }
        );
        let deal = wrapped("Deal", |writer| {
            writer.map(2);
            writer.text("cards");
            writer.array(3);
            writer.unsigned(1);
            writer.unsigned(2);
            writer.unsigned(3);
            writer.text("to");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(1);
        });
        assert_eq!(
            action_of(deal),
            Action::Deal {
                zone: Some(1),
                count: 3
            }
        );
        let dealt_to_hand = wrapped("Deal", |writer| {
            writer.map(2);
            writer.text("cards");
            writer.array(0);
            writer.text("to");
            writer.text("Hand");
        });
        assert_eq!(
            action_of(dealt_to_hand),
            Action::Deal {
                zone: None,
                count: 0
            }
        );
        let clear = wrapped("Clear", |writer| {
            writer.map(1);
            writer.text("seat");
            writer.unsigned(1);
        });
        assert_eq!(action_of(clear), Action::Clear { seat: 1 });
        let join = wrapped("Join", |writer| {
            writer.map(1);
            writer.text("name");
            writer.text("ada");
        });
        assert_eq!(action_of(join), Action::Join);
        let genesis = wrapped("Genesis", |writer| {
            writer.map(1);
            writer.text("name");
            writer.text("rae");
        });
        assert_eq!(action_of(genesis), Action::Other);
        let spawn = wrapped("Spawn", |writer| {
            writer.map(3);
            writer.text("face");
            writer.map(6);
            writer.text("name");
            writer.text("Sprite");
            writer.text("kind");
            writer.text("Unit");
            writer.text("might");
            writer.unsigned(3);
            writer.text("energy");
            writer.unsigned(2);
            writer.text("power");
            writer.unsigned(1);
            writer.text("domain");
            writer.array(1);
            writer.text("Calm");
            writer.text("to");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(9);
            writer.text("seat");
            writer.unsigned(1);
        });
        assert_eq!(
            action_of(spawn),
            Action::Spawn {
                face: Face::named("Sprite")
                    .with_kind("Unit")
                    .with_might(Some(3))
                    .with_cost(Some(2), Some(1))
                    .with_domain(vec!["Calm".into()]),
                zone: Some(9),
                seat: 1
            }
        );
        let onto_board = wrapped("Move", |writer| {
            writer.map(4);
            writer.text("card");
            writer.unsigned(3);
            writer.text("to");
            writer.text("Board");
            writer.text("seat");
            writer.unsigned(1);
            writer.text("index");
            writer.unsigned(0);
        });
        assert_eq!(
            action_of(onto_board),
            Action::Move {
                card: 3,
                to: Some(crate::table::BOARD),
                seat: 1,
                index: 0,
                hidden: false,
            }
        );
    }

    #[test]
    fn a_request_carries_the_table_it_was_decided_against() {
        let bytes = request_at(
            &[],
            2,
            1,
            &[placed(3, 7, 1, "Fury Rune"), placed(4, 7, 1, "Calm Rune")],
            &[4],
            &move_to(3, 8, 1),
        );
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.table.cards.len(), 2);
        assert!(parsed.table.card(4).unwrap().exhausted);
        assert!(!parsed.table.card(3).unwrap().exhausted);
    }

    #[test]
    fn verdicts_encode_as_the_engines_map() {
        let bytes = Verdict::advance(vec![1]).encode();
        assert_eq!(
            &bytes[..9],
            &[0xa2, 0x66, b'a', b'c', b'c', b'e', b'p', b't', 0xf5]
        );
        assert_eq!(Verdict::reject().encode().last(), Some(&0xf6));
        assert_eq!(accept_all(&[]), Verdict::accept().encode());
        let with = Verdict::accept()
            .with_effects(vec![
                Effect::exhaust(3),
                Effect::ready(4),
                Effect::score(1, 0, -1),
                Effect::Move {
                    card: 5,
                    zone: 7,
                    seat: 1,
                    index: BOTTOM,
                },
                Effect::Spawn {
                    face: Face::named("Sprite").with_kind("Unit").with_might(Some(3)),
                    zone: 9,
                    seat: 0,
                    owner: None,
                },
                Effect::Despawn { card: 6 },
            ])
            .encode();
        assert_eq!(with[0], 0xa3);
        assert!(with.windows(7).any(|window| window == b"effects"));
        assert!(with.windows(6).any(|window| window == b"Sprite"));
        assert!(!with.windows(5).any(|window| window == b"owner"));
        let owned = Verdict::accept()
            .with_effects(vec![Effect::Spawn {
                face: Face::named("Gold").with_kind("Gear"),
                zone: 9,
                seat: 1,
                owner: Some(1),
            }])
            .encode();
        assert!(owned.windows(5).any(|window| window == b"owner"));
        assert_eq!(owned.last(), Some(&1));
        let full = Verdict::accept()
            .with_effects(vec![Effect::Spawn {
                face: Face::named("Sprite")
                    .with_cost(Some(2), Some(1))
                    .with_domain(vec!["Calm".into()]),
                zone: crate::table::BOARD,
                seat: 1,
                owner: None,
            }])
            .encode();
        for key in ["energy", "power", "domain", "Calm", "Board"] {
            assert!(
                full.windows(key.len())
                    .any(|window| window == key.as_bytes()),
                "{key} is written"
            );
        }
        assert!(!full.windows(6).any(|window| window == b"Plugin"));
    }

    #[test]
    fn the_information_effects_write_the_engines_keys() {
        let bytes = Verdict::accept()
            .with_effects(vec![
                Effect::Reveal { card: 4 },
                Effect::Peek { card: 5, seat: 1 },
            ])
            .encode();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.map_len(), Some(3));
        assert_eq!(reader.key(), Some("accept"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("plugin_state"));
        reader.skip().unwrap();
        assert_eq!(reader.key(), Some("effects"));
        assert_eq!(reader.array_len(), Some(2));
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("Reveal"));
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("card"));
        assert_eq!(reader.unsigned(), Some(4));
        assert_eq!(reader.map_len(), Some(1));
        assert_eq!(reader.key(), Some("Peek"));
        assert_eq!(reader.map_len(), Some(2));
        assert_eq!(reader.key(), Some("card"));
        assert_eq!(reader.unsigned(), Some(5));
        assert_eq!(reader.key(), Some("seat"));
        assert_eq!(reader.unsigned(), Some(1));
        assert_eq!(reader.item(), None);
    }

    #[test]
    fn a_refusal_carries_its_reason_under_the_reason_key() {
        let bytes = Verdict::refuse("runes are paid for you").encode();
        assert_eq!(bytes[0], 0xa3);
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.map_len(), Some(3));
        assert_eq!(reader.key(), Some("accept"));
        assert_eq!(reader.item(), Some(Item::Simple(20)));
        assert_eq!(reader.key(), Some("plugin_state"));
        assert_eq!(reader.item(), Some(Item::Simple(22)));
        assert_eq!(reader.key(), Some("reason"));
        assert_eq!(reader.item(), Some(Item::Text("runes are paid for you")));
        assert_eq!(reader.item(), None);
        let silent = Verdict::reject().encode();
        assert!(!silent.windows(6).any(|window| window == b"reason"));
        assert_eq!(silent[0], 0xa2);
    }
}
