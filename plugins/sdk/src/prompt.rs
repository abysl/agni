use crate::blob::{MapReader, MapWriter};
use crate::cbor::{Item, Reader, Writer};
use crate::view::PromptSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Card(u32),
    Zone(u16),
    Seat(u8),
    Item(u16),
    Mode(u8),
    Name(u16),
    Yes,
    No,
    Done,
    Skip,
    Cancel,
}

impl Answer {
    pub fn value(self) -> Option<u32> {
        match self {
            Answer::Card(card) => Some(card),
            Answer::Zone(zone) => Some(u32::from(zone)),
            Answer::Seat(seat) => Some(u32::from(seat)),
            Answer::Item(item) => Some(u32::from(item)),
            Answer::Mode(mode) => Some(u32::from(mode)),
            Answer::Name(name) => Some(u32::from(name)),
            _ => None,
        }
    }

    pub fn closes(self) -> bool {
        matches!(self, Answer::Done | Answer::Skip | Answer::Cancel)
    }

    pub fn write(self, writer: &mut Writer) {
        let (key, value) = match self {
            Answer::Card(card) => ("Card", u64::from(card)),
            Answer::Zone(zone) => ("Zone", u64::from(zone)),
            Answer::Seat(seat) => ("Seat", u64::from(seat)),
            Answer::Item(item) => ("Item", u64::from(item)),
            Answer::Mode(mode) => ("Mode", u64::from(mode)),
            Answer::Name(name) => ("Name", u64::from(name)),
            Answer::Yes => return writer.text("Yes"),
            Answer::No => return writer.text("No"),
            Answer::Done => return writer.text("Done"),
            Answer::Skip => return writer.text("Skip"),
            Answer::Cancel => return writer.text("Cancel"),
        };
        writer.map(1);
        writer.text(key);
        writer.unsigned(value);
    }

    pub fn read(reader: &mut Reader) -> Option<Self> {
        match reader.item()? {
            Item::Text("Yes") => Some(Answer::Yes),
            Item::Text("No") => Some(Answer::No),
            Item::Text("Done") => Some(Answer::Done),
            Item::Text("Skip") => Some(Answer::Skip),
            Item::Text("Cancel") => Some(Answer::Cancel),
            Item::Map(1) => {
                let key = reader.key()?;
                let value = reader.unsigned()?;
                Some(match key {
                    "Card" => Answer::Card(u32::try_from(value).ok()?),
                    "Zone" => Answer::Zone(u16::try_from(value).ok()?),
                    "Seat" => Answer::Seat(u8::try_from(value).ok()?),
                    "Item" => Answer::Item(u16::try_from(value).ok()?),
                    "Mode" => Answer::Mode(u8::try_from(value).ok()?),
                    "Name" => Answer::Name(u16::try_from(value).ok()?),
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    pub fn encode(self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.write(&mut writer);
        writer.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        Self::read(&mut Reader::new(bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub label: String,
    pub card: Option<u32>,
    pub answer: Answer,
}

impl Opt {
    pub fn new(label: impl Into<String>, answer: Answer) -> Self {
        Self {
            label: label.into(),
            card: None,
            answer,
        }
    }

    pub fn card(label: impl Into<String>, card: u32) -> Self {
        Self {
            label: label.into(),
            card: Some(card),
            answer: Answer::Card(card),
        }
    }

    pub fn write(&self, writer: &mut Writer) {
        let mut map = MapWriter::new();
        map.field("label").text(&self.label);
        if let Some(card) = self.card {
            map.field("card").unsigned(u64::from(card));
        }
        self.answer.write(map.field("answer"));
        map.write_into(writer);
    }

    pub fn read(reader: &mut Reader) -> Option<Self> {
        let mut map = MapReader::open(reader)?;
        let mut label = None;
        let mut card = None;
        let mut answer = None;
        while let Some(key) = map.field() {
            match key {
                "label" => {
                    label = match map.value().item()? {
                        Item::Text(text) => Some(text.to_string()),
                        _ => return None,
                    }
                }
                "card" => card = Some(u32::try_from(map.value().unsigned()?).ok()?),
                "answer" => answer = Some(Answer::read(map.value())?),
                _ => map.skip_unknown()?,
            }
        }
        map.finish()?;
        Some(Self {
            label: label?,
            card,
            answer: answer?,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.write(&mut writer);
        writer.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        Self::read(&mut Reader::new(bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub prompt: u16,
    pub option: u16,
}

impl Pick {
    pub const LEN: usize = 4;

    pub fn encode(self) -> [u8; Self::LEN] {
        let [p0, p1] = self.prompt.to_le_bytes();
        let [o0, o1] = self.option.to_le_bytes();
        [p0, p1, o0, o1]
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let [p0, p1, o0, o1] = bytes.try_into().ok()?;
        Some(Self {
            prompt: u16::from_le_bytes([p0, p1]),
            option: u16::from_le_bytes([o0, o1]),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickRefusal {
    Stale { open: u16, sent: u16 },
    NotYourPrompt { seat: u8 },
    NoSuchOption { option: u16, count: usize },
}

impl PickRefusal {
    pub fn label(self) -> String {
        match self {
            PickRefusal::Stale { open, sent } => {
                format!("that question is gone (prompt {sent}, now {open})")
            }
            PickRefusal::NotYourPrompt { seat } => format!("the question is for {{seat {seat}}}"),
            PickRefusal::NoSuchOption { option, count } => {
                format!("option {option} is not one of the {count} offered")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prompt {
    pub id: u16,
    pub seat: u8,
    pub min: u8,
    pub max: u8,
    pub picked: Vec<u32>,
    pub cancel: bool,
}

impl Prompt {
    pub fn new(id: u16, seat: u8, min: u8, max: u8) -> Self {
        Self {
            id,
            seat,
            min,
            max: max.max(1),
            picked: Vec::new(),
            cancel: false,
        }
    }

    pub fn cancellable(mut self) -> Self {
        self.cancel = true;
        self
    }

    pub fn is_multi(&self) -> bool {
        self.max > 1
    }

    pub fn is_full(&self) -> bool {
        self.picked.len() >= usize::from(self.max)
    }

    pub fn can_finish(&self) -> bool {
        self.picked.len() >= usize::from(self.min)
    }

    pub fn can_skip(&self) -> bool {
        self.min == 0 && self.picked.is_empty()
    }

    pub fn is_picked(&self, value: u32) -> bool {
        self.picked.contains(&value)
    }

    pub fn record(&mut self, answer: Answer) -> bool {
        let Some(value) = answer.value() else {
            return false;
        };
        if self.is_picked(value) || self.is_full() {
            return false;
        }
        self.picked.push(value);
        true
    }

    pub fn answer(&mut self, answer: Answer) -> Option<bool> {
        if answer.closes() || answer.value().is_none() {
            return Some(true);
        }
        if !self.record(answer) {
            return None;
        }
        Some(!self.is_multi())
    }

    pub fn closers(&self) -> Vec<Opt> {
        let mut closers = Vec::new();
        if self.is_multi() && self.can_finish() {
            closers.push(Opt::new("done", Answer::Done));
        }
        if self.can_skip() {
            closers.push(Opt::new("skip", Answer::Skip));
        }
        if self.cancel {
            closers.push(Opt::new("cancel", Answer::Cancel));
        }
        closers
    }

    pub fn numbered(&self, candidates: Vec<Opt>) -> Vec<Opt> {
        if self.is_full() {
            return self.closers();
        }
        let mut options: Vec<Opt> = candidates
            .into_iter()
            .filter(|opt| {
                opt.answer
                    .value()
                    .is_none_or(|value| !self.is_picked(value))
            })
            .collect();
        options.extend(self.closers());
        options
    }

    pub fn resolve(&self, seat: u8, pick: Pick, options: &[Opt]) -> Result<Answer, PickRefusal> {
        if pick.prompt != self.id {
            return Err(PickRefusal::Stale {
                open: self.id,
                sent: pick.prompt,
            });
        }
        if seat != self.seat {
            return Err(PickRefusal::NotYourPrompt { seat: self.seat });
        }
        options
            .get(usize::from(pick.option))
            .map(|opt| opt.answer)
            .ok_or(PickRefusal::NoSuchOption {
                option: pick.option,
                count: options.len(),
            })
    }

    pub fn summary(&self, why: impl Into<String>) -> PromptSummary {
        PromptSummary {
            seat: self.seat,
            why: why.into(),
            min: self.min,
            max: self.max,
            picked: u8::try_from(self.picked.len()).unwrap_or(u8::MAX),
            optional: self.min == 0,
        }
    }

    pub fn write(&self, writer: &mut Writer) {
        let mut map = MapWriter::new();
        map.field("id").unsigned(u64::from(self.id));
        map.field("seat").unsigned(u64::from(self.seat));
        if self.min != 0 {
            map.field("min").unsigned(u64::from(self.min));
        }
        map.field("max").unsigned(u64::from(self.max));
        if !self.picked.is_empty() {
            let picked = map.field("picked");
            picked.array(self.picked.len());
            for value in &self.picked {
                picked.unsigned(u64::from(*value));
            }
        }
        if self.cancel {
            map.field("cancel").bool(true);
        }
        map.write_into(writer);
    }

    pub fn read(reader: &mut Reader) -> Option<Self> {
        let mut map = MapReader::open(reader)?;
        let mut prompt = Prompt::default();
        let mut id = None;
        while let Some(key) = map.field() {
            match key {
                "id" => id = Some(u16::try_from(map.value().unsigned()?).ok()?),
                "seat" => prompt.seat = u8::try_from(map.value().unsigned()?).ok()?,
                "min" => prompt.min = u8::try_from(map.value().unsigned()?).ok()?,
                "max" => prompt.max = u8::try_from(map.value().unsigned()?).ok()?,
                "picked" => {
                    for _ in 0..map.value().array_len()? {
                        prompt
                            .picked
                            .push(u32::try_from(map.value().unsigned()?).ok()?);
                    }
                }
                "cancel" => {
                    prompt.cancel = match map.value().item()? {
                        Item::Simple(21) => true,
                        Item::Simple(20) => false,
                        _ => return None,
                    }
                }
                _ => map.skip_unknown()?,
            }
        }
        map.finish()?;
        prompt.id = id?;
        prompt.max = prompt.max.max(1);
        Some(prompt)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.write(&mut writer);
        writer.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        Self::read(&mut Reader::new(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_and_options_round_trip_through_cbor() {
        let answers = [
            Answer::Card(70000),
            Answer::Zone(9),
            Answer::Seat(1),
            Answer::Item(300),
            Answer::Mode(2),
            Answer::Name(300),
            Answer::Yes,
            Answer::No,
            Answer::Done,
            Answer::Skip,
            Answer::Cancel,
        ];
        for answer in answers {
            assert_eq!(Answer::decode(&answer.encode()), Some(answer));
        }
        assert_eq!(Answer::decode(&[0xa1, 0x61, b'X', 0x01]), None);
        assert_eq!(Answer::decode(b"\x63Nay"), None);
        assert_eq!(Answer::Card(3).value(), Some(3));
        assert_eq!(Answer::Done.value(), None);
        assert!(Answer::Cancel.closes());
        assert!(!Answer::Yes.closes());
        let opt = Opt::card("set aside {card 3}", 3);
        assert_eq!(Opt::decode(&opt.encode()), Some(opt.clone()));
        let plain = Opt::new("keep", Answer::Done);
        let bytes = plain.encode();
        assert!(!bytes.windows(4).any(|w| w == b"card"));
        assert_eq!(Opt::decode(&bytes), Some(plain));
        let mut with_extra = MapWriter::new();
        with_extra.field("label").text("x");
        with_extra.field("later").unsigned(1);
        Answer::Yes.write(with_extra.field("answer"));
        assert_eq!(
            Opt::decode(&with_extra.finish()),
            Some(Opt::new("x", Answer::Yes))
        );
    }

    #[test]
    fn a_pick_is_four_little_endian_bytes() {
        let pick = Pick {
            prompt: 0x0102,
            option: 0x0304,
        };
        assert_eq!(pick.encode(), [2, 1, 4, 3]);
        assert_eq!(Pick::decode(&pick.encode()), Some(pick));
        assert_eq!(Pick::decode(&[1, 2, 3]), None);
        assert_eq!(Pick::decode(&[1, 2, 3, 4, 5]), None);
    }

    #[test]
    fn a_pick_is_refused_when_stale_foreign_or_out_of_range() {
        let prompt = Prompt::new(7, 1, 1, 1);
        let options = vec![Opt::card("{card 3}", 3), Opt::card("{card 4}", 4)];
        let pick = |prompt, option| Pick { prompt, option };
        assert_eq!(prompt.resolve(1, pick(7, 1), &options), Ok(Answer::Card(4)));
        assert_eq!(
            prompt.resolve(1, pick(6, 0), &options),
            Err(PickRefusal::Stale { open: 7, sent: 6 })
        );
        assert_eq!(
            prompt.resolve(0, pick(7, 0), &options),
            Err(PickRefusal::NotYourPrompt { seat: 1 })
        );
        assert_eq!(
            prompt.resolve(1, pick(7, 2), &options),
            Err(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            })
        );
        for refusal in [
            PickRefusal::Stale { open: 7, sent: 6 },
            PickRefusal::NotYourPrompt { seat: 1 },
            PickRefusal::NoSuchOption {
                option: 2,
                count: 2,
            },
        ] {
            assert!(!refusal.label().is_empty());
        }
    }

    #[test]
    fn a_multi_select_accumulates_and_offers_done_skip_and_cancel_as_allowed() {
        let mut prompt = Prompt::new(2, 0, 0, 2).cancellable();
        let candidates = || vec![Opt::card("a", 10), Opt::card("b", 11), Opt::card("c", 12)];
        let labels = |options: &[Opt]| -> Vec<String> {
            options.iter().map(|opt| opt.label.clone()).collect()
        };
        assert_eq!(
            labels(&prompt.numbered(candidates())),
            ["a", "b", "c", "done", "skip", "cancel"]
        );
        assert!(prompt.record(Answer::Card(11)));
        assert!(!prompt.record(Answer::Card(11)));
        assert!(!prompt.record(Answer::Done));
        assert_eq!(
            labels(&prompt.numbered(candidates())),
            ["a", "c", "done", "cancel"]
        );
        assert!(prompt.record(Answer::Card(10)));
        assert!(prompt.is_full());
        assert!(!prompt.record(Answer::Card(12)));
        assert_eq!(labels(&prompt.numbered(candidates())), ["done", "cancel"]);
        let summary = prompt.summary("mulligan");
        assert_eq!(
            summary,
            PromptSummary {
                seat: 0,
                why: "mulligan".into(),
                min: 0,
                max: 2,
                picked: 2,
                optional: true,
            }
        );

        let mut strict = Prompt::new(3, 1, 2, 3);
        assert!(strict.closers().is_empty());
        strict.record(Answer::Card(1));
        assert!(strict.closers().is_empty());
        strict.record(Answer::Card(2));
        assert_eq!(labels(&strict.closers()), ["done"]);
        assert!(!strict.summary("targets").optional);

        let single = Prompt::new(4, 1, 1, 1);
        assert!(single.closers().is_empty());
        assert!(!single.is_multi());
        assert_eq!(Prompt::new(5, 0, 0, 0).max, 1);
    }

    #[test]
    fn an_answer_closes_a_single_select_and_keeps_a_multi_select_open() {
        let mut single = Prompt::new(1, 0, 1, 1);
        assert_eq!(single.answer(Answer::Card(3)), Some(true));
        assert_eq!(single.picked, [3]);
        let mut multi = Prompt::new(2, 0, 0, 2).cancellable();
        assert_eq!(multi.answer(Answer::Card(3)), Some(false));
        assert_eq!(multi.answer(Answer::Card(3)), None);
        assert_eq!(multi.answer(Answer::Card(4)), Some(false));
        assert_eq!(multi.answer(Answer::Card(5)), None);
        assert_eq!(multi.answer(Answer::Done), Some(true));
        assert_eq!(multi.picked, [3, 4]);
        let mut confirm = Prompt::new(3, 1, 1, 1);
        assert_eq!(confirm.answer(Answer::No), Some(true));
        assert!(confirm.picked.is_empty());
        let mut optional = Prompt::new(4, 1, 0, 1);
        assert_eq!(optional.answer(Answer::Skip), Some(true));
        assert_eq!(optional.answer(Answer::Cancel), Some(true));
    }

    #[test]
    fn a_prompt_round_trips_and_omits_its_defaults() {
        let bare = Prompt::new(1, 0, 0, 1);
        let bytes = bare.encode();
        assert_eq!(bytes[0], 0xa3);
        assert_eq!(Prompt::decode(&bytes), Some(bare));
        let mut full = Prompt::new(9, 1, 1, 2).cancellable();
        full.record(Answer::Zone(300));
        let bytes = full.encode();
        assert_eq!(bytes[0], 0xa6);
        assert_eq!(Prompt::decode(&bytes), Some(full));
        let mut unknown = MapWriter::new();
        unknown.field("id").unsigned(4);
        unknown.field("later").text("ignored");
        unknown.field("max").unsigned(2);
        let decoded = Prompt::decode(&unknown.finish()).unwrap();
        assert_eq!((decoded.id, decoded.max, decoded.seat), (4, 2, 0));
        let mut no_id = MapWriter::new();
        no_id.field("max").unsigned(2);
        assert_eq!(Prompt::decode(&no_id.finish()), None);
        assert_eq!(Prompt::decode(&[0x01]), None);
        let mut zero = MapWriter::new();
        zero.field("id").unsigned(5);
        zero.field("max").unsigned(0);
        let clamped = Prompt::decode(&zero.finish()).unwrap();
        assert_eq!(clamped.max, 1);
        assert!(!clamped.is_full());
        let mut missing = MapWriter::new();
        missing.field("id").unsigned(6);
        assert_eq!(Prompt::decode(&missing.finish()).unwrap().max, 1);
    }
}
