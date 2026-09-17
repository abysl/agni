use agni_core::{CardFace, Zone};
use agni_sim::abi::{decode, encode};
use agni_sim::log::{LogAction, LogEntry};
use agni_sim::wire::{CounterTarget, DealGroup};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

pub const WIRE_VERSION: u32 = 10;

pub const MAX_CHAT_BYTES: usize = 2000;

pub fn chat_text(text: &str) -> Result<String, &'static str> {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_CHAT_BYTES {
        return Err("Chat must contain 1–2000 bytes");
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("Chat contains unsupported control characters");
    }
    Ok(text.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoProposal {
    pub id: u64,
    pub requester: u8,
    pub actions: u32,
    pub waiting: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoStatus {
    pub revision: u64,
    pub available: u32,
    pub proposal: Option<UndoProposal>,
}

pub const MODULE_CHUNK_BYTES: usize = 256 << 10;

pub const MAX_MODULE_BYTES: usize = 64 << 20;

pub const SEAT_PICKABLE_COLORS: u8 = 5;

pub fn default_seat_color(seat: u8) -> u8 {
    seat % SEAT_PICKABLE_COLORS
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatInfo {
    pub seat: u8,
    pub name: String,
    pub host: bool,
    pub connected: bool,
    pub color: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playmat: Option<String>,
}

pub fn roster_playmat(roster: &[SeatInfo], seat: u8) -> Option<&str> {
    roster
        .iter()
        .find(|info| info.seat == seat)
        .and_then(|info| info.playmat.as_deref())
}

pub fn roster_color(roster: &[SeatInfo], seat: u8) -> u8 {
    roster
        .iter()
        .find(|info| info.seat == seat)
        .map(|info| info.color)
        .unwrap_or_else(|| default_seat_color(seat))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireIntent {
    Move {
        card: u32,
        to: Zone,
        seat: u8,
        index: u32,
    },
    MoveHidden {
        card: u32,
        to: Zone,
        seat: u8,
        index: u32,
    },
    Reveal {
        card: u32,
    },
    Annotate {
        card: u32,
        key: String,
        value: Option<ByteBuf>,
    },
    Game {
        data: ByteBuf,
    },
    Counter {
        target: CounterTarget,
        counter: u16,
        delta: i32,
    },
    Spawn {
        face: CardFace,
        to: Zone,
        seat: u8,
    },
}

impl From<WireIntent> for LogAction {
    fn from(intent: WireIntent) -> Self {
        match intent {
            WireIntent::Move {
                card,
                to,
                seat,
                index,
            } => LogAction::Move {
                card,
                to,
                seat,
                index,
                hidden: false,
            },
            WireIntent::MoveHidden {
                card,
                to,
                seat,
                index,
            } => LogAction::Move {
                card,
                to,
                seat,
                index,
                hidden: true,
            },
            WireIntent::Reveal { card } => LogAction::Reveal {
                card,
                face: CardFace::hidden(),
            },
            WireIntent::Annotate { card, key, value } => LogAction::Annotate { card, key, value },
            WireIntent::Game { data } => LogAction::Game { data },
            WireIntent::Counter {
                target,
                counter,
                delta,
            } => LogAction::Counter {
                target,
                counter,
                delta,
            },
            WireIntent::Spawn { face, to, seat } => LogAction::Spawn { face, to, seat },
        }
    }
}

impl TryFrom<LogAction> for WireIntent {
    type Error = LogAction;

    fn try_from(action: LogAction) -> Result<Self, LogAction> {
        match action {
            LogAction::Move {
                card,
                to,
                seat,
                index,
                hidden: false,
            } => Ok(WireIntent::Move {
                card,
                to,
                seat,
                index,
            }),
            LogAction::Move {
                card,
                to,
                seat,
                index,
                hidden: true,
            } => Ok(WireIntent::MoveHidden {
                card,
                to,
                seat,
                index,
            }),
            LogAction::Annotate { card, key, value } => {
                Ok(WireIntent::Annotate { card, key, value })
            }
            LogAction::Game { data } => Ok(WireIntent::Game { data }),
            LogAction::Counter {
                target,
                counter,
                delta,
            } => Ok(WireIntent::Counter {
                target,
                counter,
                delta,
            }),
            LogAction::Spawn { face, to, seat } => Ok(WireIntent::Spawn { face, to, seat }),
            LogAction::Reveal { card, .. } => Ok(WireIntent::Reveal { card }),
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    Chat {
        text: String,
    },
    RequestUndo {
        actions: u32,
        revision: u64,
    },
    VoteUndo {
        id: u64,
        accept: bool,
    },
    Join {
        name: String,
        version: u32,
    },
    Intent {
        intent: WireIntent,
    },
    DealDeck {
        groups: Vec<DealGroup>,
    },
    ReloadDeck {
        groups: Vec<DealGroup>,
    },
    PickColor {
        color: u8,
    },
    PickPlaymat {
        playmat: Option<String>,
    },
    NewGame,
    NeedModule {
        #[serde(with = "serde_bytes")]
        hash: [u8; 32],
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostMsg {
    Chat {
        id: u64,
        seat: u8,
        text: String,
    },
    Undo {
        status: UndoStatus,
    },
    RolledBack {
        next_seq: u64,
        faces: Vec<(u32, CardFace)>,
    },
    Welcome {
        version: u32,
        seat: u8,
        roster: Vec<SeatInfo>,
        log: Vec<LogEntry>,
    },
    Roster {
        roster: Vec<SeatInfo>,
    },
    Entry {
        entry: LogEntry,
    },
    Faces {
        faces: Vec<(u32, CardFace)>,
    },
    End {
        reason: String,
    },
    Notice {
        text: String,
    },
    Module {
        #[serde(with = "serde_bytes")]
        hash: [u8; 32],
        total: u32,
        offset: u32,
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    NoModule {
        #[serde(with = "serde_bytes")]
        hash: [u8; 32],
        reason: String,
    },
}

pub fn module_chunks(hash: [u8; 32], bytes: &[u8]) -> Vec<HostMsg> {
    let total = bytes.len() as u32;
    if bytes.is_empty() {
        return vec![HostMsg::Module {
            hash,
            total,
            offset: 0,
            bytes: Vec::new(),
        }];
    }
    bytes
        .chunks(MODULE_CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| HostMsg::Module {
            hash,
            total,
            offset: (index * MODULE_CHUNK_BYTES) as u32,
            bytes: chunk.to_vec(),
        })
        .collect()
}

pub fn encode_client(msg: &ClientMsg) -> Vec<u8> {
    encode(msg)
}

pub fn decode_client(bytes: &[u8]) -> Option<ClientMsg> {
    decode(bytes)
}

pub fn encode_host(msg: &HostMsg) -> Vec<u8> {
    encode(msg)
}

pub fn decode_host(bytes: &[u8]) -> Option<HostMsg> {
    decode(bytes)
}

pub fn version_mismatch(theirs: u32) -> Option<String> {
    (theirs != WIRE_VERSION).then(|| {
        format!("wire protocol {theirs} does not match this build's {WIRE_VERSION} — update the older peer")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::Value;

    #[test]
    fn chat_is_bounded_and_round_trips_without_a_claimed_sender() {
        assert_eq!(chat_text(" hello ").unwrap(), "hello");
        assert!(chat_text("  ").is_err());
        assert!(chat_text(&"é".repeat(1001)).is_err());
        assert!(chat_text("bad\0text").is_err());
        let request = ClientMsg::Chat {
            text: "hello".into(),
        };
        assert_eq!(decode_client(&encode_client(&request)), Some(request));
        let reply = HostMsg::Chat {
            id: 1,
            seat: 2,
            text: "hello".into(),
        };
        assert_eq!(decode_host(&encode_host(&reply)), Some(reply));
    }

    fn padded(variant: &str, fields: Vec<(&str, Value)>) -> Vec<u8> {
        let mut fields: Vec<(Value, Value)> = fields
            .into_iter()
            .map(|(key, value)| (Value::Text(key.into()), value))
            .collect();
        fields.push((Value::Text("later".into()), Value::Text("ignored".into())));
        encode(&Value::Map(vec![(
            Value::Text(variant.into()),
            Value::Map(fields),
        )]))
    }

    #[test]
    fn need_module_round_trips_and_skips_unknown_keys() {
        let msg = ClientMsg::NeedModule { hash: [7; 32] };
        assert_eq!(decode_client(&encode_client(&msg)).unwrap(), msg);
        let bytes = padded("NeedModule", vec![("hash", Value::Bytes(vec![7; 32]))]);
        assert_eq!(decode_client(&bytes).unwrap(), msg);
        assert!(decode_client(&padded(
            "NeedModule",
            vec![("hash", Value::Bytes(vec![7; 31]))]
        ))
        .is_none());
    }

    #[test]
    fn module_frames_round_trip_and_skip_unknown_keys() {
        let msg = HostMsg::Module {
            hash: [9; 32],
            total: 5,
            offset: 2,
            bytes: vec![1, 2, 3],
        };
        assert_eq!(decode_host(&encode_host(&msg)).unwrap(), msg);
        let bytes = padded(
            "Module",
            vec![
                ("hash", Value::Bytes(vec![9; 32])),
                ("total", Value::Integer(5.into())),
                ("offset", Value::Integer(2.into())),
                ("bytes", Value::Bytes(vec![1, 2, 3])),
            ],
        );
        assert_eq!(decode_host(&bytes).unwrap(), msg);
        let refusal = HostMsg::NoModule {
            hash: [9; 32],
            reason: "not pinned".into(),
        };
        assert_eq!(decode_host(&encode_host(&refusal)).unwrap(), refusal);
        let bytes = padded(
            "NoModule",
            vec![
                ("hash", Value::Bytes(vec![9; 32])),
                ("reason", Value::Text("not pinned".into())),
            ],
        );
        assert_eq!(decode_host(&bytes).unwrap(), refusal);
    }

    #[test]
    fn a_module_is_chunked_under_the_frame_limit_and_an_empty_one_still_answers() {
        let bytes: Vec<u8> = (0..(MODULE_CHUNK_BYTES * 2 + 5)).map(|i| i as u8).collect();
        let frames = module_chunks([1; 32], &bytes);
        assert_eq!(frames.len(), 3);
        let mut rebuilt = Vec::new();
        for (index, frame) in frames.iter().enumerate() {
            let HostMsg::Module {
                hash,
                total,
                offset,
                bytes: chunk,
            } = frame
            else {
                panic!("a module frame");
            };
            assert_eq!(*hash, [1; 32]);
            assert_eq!(*total as usize, bytes.len());
            assert_eq!(*offset as usize, index * MODULE_CHUNK_BYTES);
            assert!(chunk.len() <= MODULE_CHUNK_BYTES);
            assert!(encode_host(frame).len() < crate::table::MAX_FRAME_BYTES);
            rebuilt.extend_from_slice(chunk);
        }
        assert_eq!(rebuilt, bytes);
        assert_eq!(module_chunks([2; 32], &[]).len(), 1);
    }
}
