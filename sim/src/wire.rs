pub use agni_core::CardFace as WireFace;
use agni_core::CardFace;
pub use agni_core::Zone as WireZone;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneKind {
    Hand,
    Deck,
    Discard,
    Stack,
    Battlefield,
    Aux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneOwner {
    PerSeat,
    Shared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneVisibility {
    All,
    Owner,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneLayout {
    Fan,
    Pile,
    Row,
    Grid,
    Spread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZonePlace {
    Fan,
    Inner,
    Outer,
    Center,
    Offstage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneDecl {
    pub id: u16,
    pub name: String,
    pub kind: ZoneKind,
    pub owner: ZoneOwner,
    pub visibility: ZoneVisibility,
    pub layout: ZoneLayout,
    pub place: ZonePlace,
    pub span: u8,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneSpec {
    pub id: u16,
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ZoneKind,
    pub owner: ZoneOwner,
    pub visibility: ZoneVisibility,
    pub layout: ZoneLayout,
    pub place: ZonePlace,
    pub span: u8,
}

impl From<&ZoneSpec> for ZoneDecl {
    fn from(spec: &ZoneSpec) -> Self {
        Self {
            id: spec.id,
            name: spec.name.into(),
            kind: spec.kind,
            owner: spec.owner,
            visibility: spec.visibility,
            layout: spec.layout,
            place: spec.place,
            span: spec.span,
            label: spec.label.into(),
        }
    }
}

pub fn encode_zone_table(zones: &[ZoneDecl]) -> Vec<u8> {
    crate::abi::encode(&zones)
}

pub fn decode_zone_table(bytes: &[u8]) -> Option<Vec<ZoneDecl>> {
    crate::abi::decode(bytes)
}

pub fn zone_decl(zones: &[ZoneDecl], zone: WireZone) -> Option<&ZoneDecl> {
    match zone {
        WireZone::Plugin(id) => zones.iter().find(|decl| decl.id == id),
        _ => None,
    }
}

pub fn zone_visibility(zones: &[ZoneDecl], zone: WireZone) -> Option<ZoneVisibility> {
    match zone {
        WireZone::Hand => Some(ZoneVisibility::Owner),
        WireZone::Board => Some(ZoneVisibility::All),
        WireZone::Plugin(_) => zone_decl(zones, zone).map(|decl| decl.visibility),
    }
}

pub fn zone_kind(zones: &[ZoneDecl], zone: WireZone) -> Option<ZoneKind> {
    match zone {
        WireZone::Hand => Some(ZoneKind::Hand),
        WireZone::Board => Some(ZoneKind::Aux),
        WireZone::Plugin(_) => zone_decl(zones, zone).map(|decl| decl.kind),
    }
}

pub fn sheds_state(kind: ZoneKind) -> bool {
    matches!(kind, ZoneKind::Hand | ZoneKind::Deck | ZoneKind::Discard)
}

pub fn zone_owner(zones: &[ZoneDecl], zone: WireZone) -> Option<ZoneOwner> {
    match zone {
        WireZone::Hand | WireZone::Board => Some(ZoneOwner::PerSeat),
        WireZone::Plugin(_) => zone_decl(zones, zone).map(|decl| decl.owner),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyDecl {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterScope {
    Seat,
    Card,
    Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterPlace {
    SeatPlate,
    CardBadge,
    Center,
}

pub const DEFAULT_COUNTER_COLOR: [u8; 3] = [214, 214, 220];

fn default_counter_color() -> [u8; 3] {
    DEFAULT_COUNTER_COLOR
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterDecl {
    pub id: u16,
    pub name: String,
    pub label: String,
    pub scope: CounterScope,
    pub start: i32,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub step: i32,
    pub place: CounterPlace,
    #[serde(default = "default_counter_color")]
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterSpec {
    pub id: u16,
    pub name: &'static str,
    pub label: &'static str,
    pub scope: CounterScope,
    pub start: i32,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub step: i32,
    pub place: CounterPlace,
    pub color: [u8; 3],
}

impl From<&CounterSpec> for CounterDecl {
    fn from(spec: &CounterSpec) -> Self {
        Self {
            id: spec.id,
            name: spec.name.to_string(),
            label: spec.label.to_string(),
            scope: spec.scope,
            start: spec.start,
            min: spec.min,
            max: spec.max,
            step: spec.step,
            place: spec.place,
            color: spec.color,
        }
    }
}

impl CounterDecl {
    pub fn luma(&self) -> u32 {
        let [r, g, b] = self.color;
        (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000
    }

    pub fn ink(&self) -> [u8; 3] {
        if self.luma() >= 140 {
            [18, 18, 22]
        } else {
            [242, 242, 246]
        }
    }

    pub fn clamp(&self, value: i32) -> i32 {
        let value = match self.min {
            Some(min) => value.max(min),
            None => value,
        };
        match self.max {
            Some(max) => value.min(max),
            None => value,
        }
    }

    pub fn accepts(&self, target: &CounterTarget) -> bool {
        matches!(
            (self.scope, target),
            (CounterScope::Seat, CounterTarget::Seat(_))
                | (CounterScope::Card, CounterTarget::Card(_))
                | (CounterScope::Table, CounterTarget::Table)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CounterTarget {
    Table,
    Seat(u8),
    Card(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CounterValue {
    pub target: CounterTarget,
    pub counter: u16,
    pub value: i32,
}

pub fn counter_decl(decls: &[CounterDecl], id: u16) -> Option<&CounterDecl> {
    decls.iter().find(|decl| decl.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub display: String,
    pub zones: Vec<ZoneDecl>,
    pub hotkeys: Vec<HotkeyDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<CounterDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<TokenDecl>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub despawn_any: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenDecl {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub might: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub temporary: bool,
}

impl TokenDecl {
    pub fn face(&self) -> CardFace {
        CardFace::named(self.name.clone())
            .with_kind(self.kind.clone())
            .with_might(self.might)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AffordanceKind {
    #[default]
    Plain,
    Commit {
        roll: u32,
    },
    Reveal {
        roll: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Affordance {
    pub label: String,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub kind: AffordanceKind,
    #[serde(default)]
    pub data: ByteBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<u32>,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PromptSummary {
    pub seat: u8,
    pub why: String,
    pub min: u8,
    pub max: u8,
    pub picked: u8,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LegalKind {
    Play { accelerate: bool },
    March,
    Activate { ability: u8 },
    React,
    Answer,
    Hide,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Legal {
    pub card: u32,
    pub kinds: Vec<LegalKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Origin {
    Card(u32),
    Item(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TargetRef {
    Card(u32),
    Seat(u8),
    Zone(u16),
    Item(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ArrowKind {
    Spell,
    Ability,
    Attack,
    Counter,
    Combat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Arrow {
    pub from: Origin,
    pub to: TargetRef,
    pub kind: ArrowKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct ChainRow {
    pub item: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<u32>,
    pub seat: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TurnInfo {
    pub number: u32,
    pub seat: u8,
    pub phase: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<String>,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SeatInfo {
    pub seat: u8,
    pub points: i32,
    pub victory: i32,
    #[serde(default)]
    pub xp: i32,
    #[serde(default)]
    pub hand: u32,
    #[serde(default)]
    pub deck: u32,
    #[serde(default)]
    pub runes_ready: u32,
    #[serde(default)]
    pub runes_total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Waiting {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<u8>,
    #[serde(default)]
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PluginView {
    #[serde(default)]
    pub status: Vec<String>,
    #[serde(default)]
    pub affordances: Vec<Affordance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<PromptSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legal: Vec<Legal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrows: Vec<Arrow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<ChainRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<TurnInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<SeatInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<Waiting>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub narration: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<u16>,
}

impl PluginView {
    pub fn is_hidden(&self, index: usize) -> bool {
        u16::try_from(index).is_ok_and(|index| self.hidden.contains(&index))
    }

    pub fn shown(&self) -> impl Iterator<Item = (usize, &Affordance)> + '_ {
        self.affordances
            .iter()
            .enumerate()
            .filter(|(index, _)| !self.is_hidden(*index))
    }
}

pub fn decode_plugin_view(bytes: &[u8]) -> Option<PluginView> {
    crate::abi::decode(bytes)
}

pub fn encode_plugin_manifest(manifest: &PluginManifest) -> Vec<u8> {
    crate::abi::encode(manifest)
}

pub fn decode_plugin_manifest(bytes: &[u8]) -> Option<PluginManifest> {
    crate::abi::decode(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DealTarget {
    Zone(String),
    Spread(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealGroup {
    pub target: DealTarget,
    pub faces: Vec<CardFace>,
    pub shuffle: bool,
    #[serde(default)]
    pub draw: u32,
}

pub fn hidden_face() -> CardFace {
    CardFace::hidden()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_light_counter_takes_dark_ink_and_a_dark_one_takes_light() {
        let chip = |color: [u8; 3]| super::CounterDecl {
            id: 0,
            name: "c".into(),
            label: "C".into(),
            scope: super::CounterScope::Card,
            start: 0,
            min: None,
            max: None,
            step: 1,
            place: super::CounterPlace::CardBadge,
            color,
        };
        assert_eq!(chip([26, 26, 30]).ink(), [242, 242, 246]);
        assert_eq!(chip([232, 230, 224]).ink(), [18, 18, 22]);
        assert_eq!(chip([200, 70, 62]).ink(), [242, 242, 246]);
        assert_eq!(chip([96, 176, 108]).ink(), [18, 18, 22]);
    }

    #[test]
    fn a_manifest_without_colors_still_decodes_with_a_default() {
        #[derive(serde::Serialize)]
        struct Terse {
            id: u16,
            name: String,
            label: String,
            scope: super::CounterScope,
            start: i32,
            min: Option<i32>,
            max: Option<i32>,
            step: i32,
            place: super::CounterPlace,
        }
        let bytes = crate::abi::encode(&Terse {
            id: 4,
            name: "legacy".into(),
            label: "Legacy".into(),
            scope: super::CounterScope::Seat,
            start: 0,
            min: None,
            max: None,
            step: 1,
            place: super::CounterPlace::SeatPlate,
        });
        let decoded: super::CounterDecl = crate::abi::decode(&bytes).expect("an old decl decodes");
        assert_eq!(decoded.color, super::DEFAULT_COUNTER_COLOR);
    }

    use super::*;

    #[test]
    fn a_plugin_manifest_round_trips_through_its_cbor_schema() {
        let manifest = PluginManifest {
            name: "riftbound".into(),
            version: "0.1.0".into(),
            display: "Riftbound".into(),
            zones: vec![ZoneDecl {
                id: 3,
                name: "legend".into(),
                kind: ZoneKind::Aux,
                owner: ZoneOwner::PerSeat,
                visibility: ZoneVisibility::All,
                layout: ZoneLayout::Row,
                place: ZonePlace::Inner,
                span: 2,
                label: "Legend".into(),
            }],
            tokens: vec![TokenDecl {
                name: "Sprite".into(),
                kind: "Unit".into(),
                might: Some(3),
                art: Some("ogn-274-298".into()),
                temporary: true,
            }],
            counters: vec![CounterDecl {
                id: 0,
                name: "life".into(),
                label: "Life".into(),
                scope: CounterScope::Seat,
                start: 20,
                min: None,
                max: None,
                step: 1,
                place: CounterPlace::SeatPlate,
                color: [232, 230, 224],
            }],
            hotkeys: vec![HotkeyDecl {
                key: "e".into(),
                action: "toggle-exhaust".into(),
            }],
            despawn_any: true,
        };
        let bytes = encode_plugin_manifest(&manifest);
        assert_eq!(decode_plugin_manifest(&bytes).unwrap(), manifest);
        assert_eq!(
            encode_plugin_manifest(&decode_plugin_manifest(&bytes).unwrap()),
            bytes
        );
        assert!(decode_plugin_manifest(b"not cbor").is_none());
    }

    #[test]
    fn a_view_without_cards_or_a_prompt_decodes_as_before_and_carries_them_when_sent() {
        let bare = PluginView {
            status: vec!["turn 1".into()],
            affordances: vec![Affordance {
                label: "pass".into(),
                ..Affordance::default()
            }],
            winner: None,
            prompt: None,
            ..Default::default()
        };
        let bytes = crate::abi::encode(&bare);
        assert!(!bytes.windows(4).any(|window| window == b"card"));
        assert!(!bytes.windows(6).any(|window| window == b"prompt"));
        assert_eq!(decode_plugin_view(&bytes).unwrap(), bare);
        let asked = PluginView {
            affordances: vec![Affordance {
                label: "set aside {card 3}".into(),
                card: Some(3),
                ..Affordance::default()
            }],
            prompt: Some(PromptSummary {
                seat: 1,
                why: "mulligan".into(),
                min: 0,
                max: 2,
                picked: 1,
                optional: true,
            }),
            ..bare
        };
        let decoded = decode_plugin_view(&crate::abi::encode(&asked)).unwrap();
        assert_eq!(decoded, asked);
        assert_eq!(decoded.affordances[0].card, Some(3));
        assert_eq!(decoded.prompt.as_ref().unwrap().picked, 1);
    }

    #[test]
    fn a_view_without_legal_or_arrows_writes_neither_key_and_an_old_view_still_decodes() {
        let bare = PluginView {
            status: vec!["turn 1".into()],
            ..Default::default()
        };
        let bytes = crate::abi::encode(&bare);
        assert!(!bytes.windows(5).any(|window| window == b"legal"));
        assert!(!bytes.windows(6).any(|window| window == b"arrows"));
        assert_eq!(decode_plugin_view(&bytes).unwrap(), bare);

        #[derive(serde::Serialize)]
        struct Older {
            status: Vec<String>,
            affordances: Vec<Affordance>,
            unknown: u32,
        }
        let older = crate::abi::encode(&Older {
            status: vec!["turn 1".into()],
            affordances: Vec::new(),
            unknown: 7,
        });
        let decoded = decode_plugin_view(&older).expect("an older view decodes");
        assert_eq!(decoded.legal, Vec::new());
        assert_eq!(decoded.arrows, Vec::new());
        assert_eq!(decoded.status, ["turn 1"]);
    }

    #[test]
    fn legal_rows_and_arrows_round_trip_and_a_newer_view_survives_unknown_keys() {
        let view = PluginView {
            status: vec!["turn 3".into()],
            legal: vec![
                Legal {
                    card: 70,
                    kinds: vec![LegalKind::Play { accelerate: true }],
                    zones: vec![9, 12],
                    hidden: Vec::new(),
                },
                Legal {
                    card: 50,
                    kinds: vec![LegalKind::March, LegalKind::Activate { ability: 1 }],
                    zones: vec![8, 9],
                    hidden: Vec::new(),
                },
                Legal {
                    card: 71,
                    kinds: vec![LegalKind::React, LegalKind::Answer],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                },
                Legal {
                    card: 72,
                    kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
                    zones: vec![12],
                    hidden: vec![9],
                },
            ],
            arrows: vec![
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
                    from: Origin::Card(52),
                    to: TargetRef::Item(4),
                    kind: ArrowKind::Counter,
                },
                Arrow {
                    from: Origin::Card(50),
                    to: TargetRef::Zone(9),
                    kind: ArrowKind::Attack,
                },
                Arrow {
                    from: Origin::Card(50),
                    to: TargetRef::Card(60),
                    kind: ArrowKind::Combat,
                },
            ],
            chain: vec![
                ChainRow {
                    item: 4,
                    card: Some(71),
                    seat: 0,
                },
                ChainRow {
                    item: 5,
                    card: None,
                    seat: 1,
                },
            ],
            ..Default::default()
        };
        let bytes = crate::abi::encode(&view);
        assert_eq!(decode_plugin_view(&bytes).unwrap(), view);
        assert_eq!(
            crate::abi::encode(&decode_plugin_view(&bytes).unwrap()),
            bytes
        );

        #[derive(serde::Serialize)]
        struct Newer {
            status: Vec<String>,
            affordances: Vec<Affordance>,
            legal: Vec<Legal>,
            arrows: Vec<Arrow>,
            sparkles: Vec<u8>,
        }
        let newer = crate::abi::encode(&Newer {
            status: view.status.clone(),
            affordances: Vec::new(),
            legal: view.legal.clone(),
            arrows: view.arrows.clone(),
            sparkles: vec![1, 2, 3],
        });
        let decoded = decode_plugin_view(&newer).expect("a key we do not know is skipped");
        assert_eq!(decoded.legal, view.legal);
        assert_eq!(decoded.arrows, view.arrows);
        assert!(
            decoded.chain.is_empty(),
            "a view without a chain defaults it back to nothing"
        );
    }

    #[test]
    fn a_legal_row_without_zones_omits_the_key_and_defaults_it_back() {
        let row = Legal {
            card: 71,
            kinds: vec![LegalKind::React],
            zones: Vec::new(),
            hidden: Vec::new(),
        };
        let bytes = crate::abi::encode(&row);
        assert!(!bytes.windows(5).any(|window| window == b"zones"));
        assert!(!bytes.windows(6).any(|window| window == b"hidden"));
        let back: Legal = crate::abi::decode(&bytes).unwrap();
        assert_eq!(back, row);
    }

    #[test]
    fn the_structured_fields_round_trip_and_stay_off_the_wire_when_empty() {
        let bare = PluginView {
            status: vec!["roll for first player".into()],
            ..Default::default()
        };
        let bytes = crate::abi::encode(&bare);
        for key in ["turn", "seats", "waiting", "narration", "primary", "hidden"] {
            assert!(
                !bytes
                    .windows(key.len())
                    .any(|window| window == key.as_bytes()),
                "{key} is absent from an empty view"
            );
        }
        let filled = PluginView {
            status: vec!["turn 3 · {seat 1} · action phase · rules enforced".into()],
            affordances: vec![
                Affordance {
                    label: "pass".into(),
                    ..Affordance::default()
                },
                Affordance {
                    label: "concede".into(),
                    ..Affordance::default()
                },
            ],
            turn: Some(TurnInfo {
                number: 3,
                seat: 1,
                phase: "action phase".into(),
                phases: vec!["setup".into(), "action phase".into()],
                mode: "rules enforced".into(),
            }),
            seats: vec![SeatInfo {
                seat: 0,
                points: 2,
                victory: 8,
                xp: 1,
                hand: 5,
                deck: 31,
                runes_ready: 3,
                runes_total: 6,
            }],
            waiting: Some(Waiting {
                seat: Some(1),
                what: "their action phase".into(),
            }),
            narration: vec!["{seat 0} plays {card 4}".into()],
            primary: Some(0),
            hidden: vec![1],
            ..Default::default()
        };
        let decoded = decode_plugin_view(&crate::abi::encode(&filled)).unwrap();
        assert_eq!(decoded, filled);
        assert!(decoded.is_hidden(1));
        assert!(!decoded.is_hidden(0));
        assert!(!decoded.is_hidden(9));
        let shown: Vec<usize> = decoded.shown().map(|(index, _)| index).collect();
        assert_eq!(shown, [0]);

        #[derive(serde::Serialize)]
        struct Older {
            status: Vec<String>,
            affordances: Vec<Affordance>,
        }
        let older = crate::abi::encode(&Older {
            status: vec!["turn 1".into()],
            affordances: Vec::new(),
        });
        let decoded = decode_plugin_view(&older).expect("a view without the fields decodes");
        assert_eq!(decoded.turn, None);
        assert!(decoded.seats.is_empty());
        assert_eq!(decoded.waiting, None);
        assert!(decoded.narration.is_empty());
        assert_eq!(decoded.primary, None);
        assert!(decoded.hidden.is_empty());
    }

    #[test]
    fn a_deal_group_survives_a_cbor_round_trip() {
        let group = DealGroup {
            target: DealTarget::Spread("battlefield-".into()),
            faces: vec![hidden_face()],
            shuffle: true,
            draw: 4,
        };
        let bytes = crate::abi::encode(&group);
        let back: DealGroup = crate::abi::decode(&bytes).unwrap();
        assert_eq!(back, group);
    }

    #[test]
    fn a_zone_spec_builds_its_declaration() {
        let spec = ZoneSpec {
            id: 7,
            name: "trash",
            label: "Trash",
            kind: ZoneKind::Discard,
            owner: ZoneOwner::PerSeat,
            visibility: ZoneVisibility::All,
            layout: ZoneLayout::Pile,
            place: ZonePlace::Outer,
            span: 3,
        };
        let decl = ZoneDecl::from(&spec);
        assert_eq!(decl.id, 7);
        assert_eq!(decl.name, "trash");
        assert_eq!(decl.label, "Trash");
        assert_eq!(decl.layout, ZoneLayout::Pile);
    }
}
