use crate::host::solo_move;
use crate::pins::hash_hex;
use crate::proto::{HostMsg, SeatInfo, WireIntent, MAX_MODULE_BYTES, MODULE_CHUNK_BYTES};
use agni_core::{CardFace, Table};
use agni_sim::abi::{encode, PluginViewRequest};
use agni_sim::engine::{
    fold_log_shadowed, fold_shadowed, Engine, EngineFault, FoldMode, NativeEngine, PluginModule,
};
use agni_sim::log::{FoldError, LogAction, LogEntry, LogState};
use agni_sim::view::{apply_deltas, view_to_table, TableView};
use agni_sim::wire::PluginView;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const PENDING_CAP: usize = 32;

pub const MAX_OPEN_ASSEMBLIES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleTransferError {
    Oversized {
        hash: [u8; 32],
        total: u32,
    },
    Chunk {
        hash: [u8; 32],
        offset: u32,
        expected: u32,
    },
    Mismatch {
        hash: [u8; 32],
        got: [u8; 32],
    },
    TooManyOpen {
        hash: [u8; 32],
    },
}

impl fmt::Display for ModuleTransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversized { hash, total } => write!(
                f,
                "the host offered {total} bytes for module {} — more than the {MAX_MODULE_BYTES} a module may be",
                hash_hex(hash)
            ),
            Self::Chunk {
                hash,
                offset,
                expected,
            } => write!(
                f,
                "the host's chunk of module {} landed at {offset}, not {expected}",
                hash_hex(hash)
            ),
            Self::Mismatch { hash, got } => write!(
                f,
                "the host's bytes for module {} hash to {} — refusing them",
                hash_hex(hash),
                hash_hex(got)
            ),
            Self::TooManyOpen { hash } => write!(
                f,
                "the host started module {} while {MAX_OPEN_ASSEMBLIES} transfers were already open",
                hash_hex(hash)
            ),
        }
    }
}

impl std::error::Error for ModuleTransferError {}

struct Assembly {
    total: u32,
    bytes: Vec<u8>,
}

pub struct ModuleInbox {
    open: BTreeMap<[u8; 32], Assembly>,
    done: BTreeMap<[u8; 32], Vec<u8>>,
}

impl Default for ModuleInbox {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleInbox {
    pub const fn new() -> Self {
        Self {
            open: BTreeMap::new(),
            done: BTreeMap::new(),
        }
    }

    pub fn receive(&mut self, msg: &HostMsg) -> Result<Option<[u8; 32]>, ModuleTransferError> {
        let HostMsg::Module {
            hash,
            total,
            offset,
            bytes,
        } = msg
        else {
            return Ok(None);
        };
        self.chunk(*hash, *total, *offset, bytes)
    }

    pub fn chunk(
        &mut self,
        hash: [u8; 32],
        total: u32,
        offset: u32,
        bytes: &[u8],
    ) -> Result<Option<[u8; 32]>, ModuleTransferError> {
        if total as usize > MAX_MODULE_BYTES {
            self.open.remove(&hash);
            return Err(ModuleTransferError::Oversized { hash, total });
        }
        if !self.open.contains_key(&hash) {
            if self.open.len() >= MAX_OPEN_ASSEMBLIES {
                return Err(ModuleTransferError::TooManyOpen { hash });
            }
            self.open.insert(
                hash,
                Assembly {
                    total,
                    bytes: Vec::new(),
                },
            );
        }
        let assembly = self
            .open
            .get_mut(&hash)
            .expect("the assembly was just opened");
        let expected = assembly.bytes.len() as u32;
        let fits = bytes.len() <= MODULE_CHUNK_BYTES
            && (offset as usize).saturating_add(bytes.len()) <= total as usize;
        if assembly.total != total || offset != expected || !fits {
            self.open.remove(&hash);
            return Err(ModuleTransferError::Chunk {
                hash,
                offset,
                expected,
            });
        }
        assembly.bytes.extend_from_slice(bytes);
        if assembly.bytes.len() < total as usize {
            return Ok(None);
        }
        let Assembly { bytes, .. } = self.open.remove(&hash).expect("the assembly just grew");
        let got = *blake3::hash(&bytes).as_bytes();
        if got != hash {
            return Err(ModuleTransferError::Mismatch { hash, got });
        }
        self.done.insert(hash, bytes);
        Ok(Some(hash))
    }

    pub fn progress(&self, hash: [u8; 32]) -> Option<(usize, usize)> {
        self.open
            .get(&hash)
            .map(|assembly| (assembly.bytes.len(), assembly.total as usize))
    }

    pub fn bytes(&self, hash: [u8; 32]) -> Option<&[u8]> {
        self.done.get(&hash).map(Vec::as_slice)
    }

    pub fn take(&mut self, hash: [u8; 32]) -> Option<Vec<u8>> {
        self.done.remove(&hash)
    }

    pub fn forget(&mut self, hash: [u8; 32]) {
        self.open.remove(&hash);
    }

    pub fn clear_open(&mut self) {
        self.open.clear();
    }
}

pub struct ClientSession {
    seat: u8,
    roster: Vec<SeatInfo>,
    log: Vec<LogEntry>,
    engine: Box<dyn Engine>,
    plugin: Option<Box<dyn PluginModule>>,
    state_cache: LogState,
    mirror: TableView,
    faces: BTreeMap<u32, CardFace>,
    pending: Vec<WireIntent>,
}

impl ClientSession {
    pub fn from_welcome(seat: u8, roster: Vec<SeatInfo>, entries: Vec<LogEntry>) -> Self {
        Self::from_welcome_with(seat, roster, entries, Box::new(NativeEngine::new()), None)
            .expect("the native engine folds a welcome")
    }

    pub fn from_welcome_with(
        seat: u8,
        roster: Vec<SeatInfo>,
        entries: Vec<LogEntry>,
        engine: Box<dyn Engine>,
        plugin: Option<Box<dyn PluginModule>>,
    ) -> Result<Self, EngineFault> {
        let mut session = Self {
            seat,
            roster,
            log: Vec::new(),
            engine,
            plugin,
            state_cache: LogState::new(),
            mirror: TableView::default(),
            faces: BTreeMap::new(),
            pending: Vec::new(),
        };
        for entry in &entries {
            session.harvest_reveal(entry);
        }
        session.harvest_spawned();
        if session.plugin.is_some() {
            for entry in &entries {
                let Self {
                    engine,
                    plugin,
                    state_cache,
                    ..
                } = &mut session;
                let outcome = fold_shadowed(
                    &mut **engine,
                    plugin.as_deref_mut(),
                    state_cache,
                    entry,
                    FoldMode::Sequenced,
                    seat,
                )?;
                apply_deltas(&mut session.mirror, &outcome.deltas);
            }
        } else {
            let Self {
                engine,
                state_cache,
                ..
            } = &mut session;
            let outcome = fold_log_shadowed(&mut **engine, state_cache, &entries, seat)?;
            apply_deltas(&mut session.mirror, &outcome.deltas);
        }
        session.log = entries;
        Ok(session)
    }

    fn harvest_reveal(&mut self, entry: &LogEntry) {
        if let LogAction::Reveal { card, face } = &entry.action {
            self.faces.insert(*card, face.clone());
        }
    }

    fn harvest_spawned(&mut self) {
        for card in self.state_cache.table.cards() {
            if !card.face.is_hidden() && !self.faces.contains_key(&card.id.0) {
                self.faces.insert(card.id.0, card.face.clone());
            }
        }
    }

    pub fn seat(&self) -> u8 {
        self.seat
    }

    pub fn roster(&self) -> &[SeatInfo] {
        &self.roster
    }

    pub fn set_roster(&mut self, roster: Vec<SeatInfo>) {
        self.roster = roster;
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    pub fn state(&self) -> &LogState {
        &self.state_cache
    }

    pub fn plugin_view(&mut self, seat: u8) -> PluginView {
        let Some(plugin) = self.plugin.as_deref_mut() else {
            return PluginView::default();
        };
        let request = encode(&PluginViewRequest::seen_by(
            &self.state_cache,
            seat,
            &self.faces,
        ));
        match plugin.view(&request) {
            Ok(view) => view,
            Err(error) => PluginView {
                status: vec![format!("the table's presenter faulted: {error}")],
                ..PluginView::default()
            },
        }
    }

    pub fn next_seq(&self) -> u64 {
        self.state_cache.next_seq
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn add_faces(&mut self, faces: Vec<(u32, CardFace)>) {
        for (id, face) in faces {
            self.faces.insert(id, face);
        }
    }

    #[must_use = "a refused entry means the replica and host disagree"]
    pub fn apply(&mut self, entry: LogEntry) -> Result<bool, EngineFault> {
        self.harvest_reveal(&entry);
        let seat = self.seat;
        let Self {
            engine,
            plugin,
            state_cache,
            ..
        } = self;
        let outcome = fold_shadowed(
            &mut **engine,
            plugin.as_deref_mut(),
            state_cache,
            &entry,
            FoldMode::Sequenced,
            seat,
        )?;
        if outcome.result == Err(FoldError::BadSeq) {
            return Ok(false);
        }
        apply_deltas(&mut self.mirror, &outcome.deltas);
        if outcome.result.is_ok() {
            self.harvest_spawned();
            match entry.action {
                LogAction::Move { card, hidden, .. } => {
                    if let Some(position) = self.pending.iter().position(|mine| {
                        matches!(mine, WireIntent::Move { card: pending, .. } if *pending == card)
                    }) {
                        self.pending.remove(position);
                    }
                    if hidden && entry.seat != self.seat {
                        self.faces.remove(&card);
                    }
                }
                LogAction::Reset => {
                    self.pending.clear();
                    self.faces.clear();
                }
                LogAction::Clear { .. } => {
                    let live: BTreeSet<u32> =
                        self.mirror.cards.iter().map(|card| card.id).collect();
                    self.pending.retain(|intent| match intent {
                        WireIntent::Move { card, .. } => live.contains(card),
                        _ => true,
                    });
                    self.faces.retain(|id, _| live.contains(id));
                }
                _ => {}
            }
        }
        self.log.push(entry);
        Ok(outcome.result.is_ok())
    }

    pub fn optimistic(&mut self, intent: WireIntent) {
        if self.pending.len() >= PENDING_CAP {
            self.pending.remove(0);
        }
        self.pending.push(intent);
    }

    pub fn rollback(
        &mut self,
        next_seq: u64,
        faces: Vec<(u32, CardFace)>,
    ) -> Result<(), EngineFault> {
        let count = self
            .log
            .iter()
            .take_while(|entry| entry.seq < next_seq)
            .count();
        if count == 0 || count >= self.log.len() || self.log[count].seq != next_seq {
            return Err(EngineFault("invalid rollback boundary".into()));
        }
        let backup = agni_sim::log::encode_state(&self.state_cache);
        self.engine
            .restore(&agni_sim::log::encode_state(&LogState::new()))?;
        let mut state = LogState::new();
        let mut mirror = TableView::default();
        let result = (|| {
            for entry in &self.log[..count] {
                let outcome = fold_shadowed(
                    &mut *self.engine,
                    self.plugin.as_deref_mut(),
                    &mut state,
                    entry,
                    FoldMode::Sequenced,
                    self.seat,
                )?;
                outcome
                    .result
                    .map_err(|error| EngineFault(format!("rollback replay: {error}")))?;
                apply_deltas(&mut mirror, &outcome.deltas);
            }
            Ok::<_, EngineFault>(())
        })();
        if let Err(error) = result {
            self.engine.restore(&backup)?;
            return Err(error);
        }
        self.log.truncate(count);
        self.state_cache = state;
        self.mirror = mirror;
        self.faces.clear();
        self.harvest_spawned();
        self.add_faces(faces);
        self.pending.clear();
        Ok(())
    }

    pub fn view(&self) -> &TableView {
        &self.mirror
    }

    pub fn table(&self) -> Table {
        let mut table = view_to_table(&self.mirror, &self.faces);
        for intent in &self.pending {
            solo_move(&mut table, intent);
        }
        table
    }
}
