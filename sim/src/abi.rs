use crate::engine::{FoldLogOutcome, FoldMode, FoldOutcome};
use crate::log::{LogEntry, LogState, Verdict};
use crate::view::TableView;
use crate::wire::{zone_visibility, ZoneVisibility};
use agni_core::{CardFace, PlayerId};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

pub const ENGINE_ABI_VERSION: u32 = 3;

pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).expect("abi message encodes");
    out
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    ciborium::from_reader(bytes).ok()
}

pub type Reply<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldRequest {
    pub entry: LogEntry,
    pub verdict: Option<Verdict>,
    pub mode: FoldMode,
    pub viewer: u8,
}

pub type FoldReply = Reply<FoldOutcome>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldLogRequest {
    pub entries: Vec<LogEntry>,
    pub viewer: u8,
}

pub type FoldLogReply = Reply<FoldLogOutcome>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewRequest {
    pub viewer: u8,
}

pub type ViewReply = Reply<TableView>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecideRequest {
    pub plugin_state: ByteBuf,
    pub state: LogState,
    pub entry: LogEntry,
}

pub type DecideRequestReply = Reply<Option<ByteBuf>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginViewRequest {
    pub plugin_state: ByteBuf,
    pub state: LogState,
    pub seat: u8,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub faces: BTreeMap<u32, CardFace>,
}

impl PluginViewRequest {
    pub fn of(state: &LogState, seat: u8) -> Self {
        Self {
            plugin_state: state.plugin_state.clone(),
            state: state.clone(),
            seat,
            faces: BTreeMap::new(),
        }
    }

    pub fn seen_by(state: &LogState, seat: u8, known: &BTreeMap<u32, CardFace>) -> Self {
        let mut request = Self::of(state, seat);
        request.faces = own_faces(state, seat, known);
        request
    }
}

pub fn own_faces(
    state: &LogState,
    seat: u8,
    known: &BTreeMap<u32, CardFace>,
) -> BTreeMap<u32, CardFace> {
    state
        .table
        .cards()
        .iter()
        .filter(|card| card.face.is_hidden())
        .filter(|card| {
            let owned = card.seat == PlayerId(seat)
                && zone_visibility(&state.zones, card.zone) == Some(ZoneVisibility::Owner);
            owned || state.peeked_by(card.id.0, seat)
        })
        .filter_map(|card| {
            known
                .get(&card.id.0)
                .filter(|face| !face.is_hidden())
                .map(|face| (card.id.0, face.clone()))
        })
        .collect()
}

pub type SnapshotReply = Reply<ByteBuf>;

pub type RestoreReply = Reply<()>;
