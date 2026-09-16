use crate::pins::{engine_blob_ref, hash_hex};
use crate::proto::{
    default_seat_color, module_chunks, HostMsg, SeatInfo, WireIntent, SEAT_PICKABLE_COLORS,
};
use agni_core::{CardFace, CardId, Intent, PlayerId, Rng, Table, Zone};
use agni_sim::abi::{encode, PluginViewRequest};
use agni_sim::engine::{
    fold_shadowed, native_decide_request, Engine, EngineFault, FoldMode, NativeEngine, PluginModule,
};
use agni_sim::log::{fold_entry, validate, FoldError, LogAction, LogEntry, LogState, TableConfig};
use agni_sim::view::{apply_deltas, view_to_table, TableView};
use agni_sim::wire::PluginView;
use agni_sim::wire::{
    zone_visibility, DealGroup, DealTarget, ZoneDecl, ZoneKind, ZoneOwner, ZoneVisibility,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub type OwnerFaces = Vec<(u32, CardFace)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HiddenPlay {
    seat: PlayerId,
    zone: Zone,
    by: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    Refused(FoldError),
    Engine(EngineFault),
    UnknownSeat(u8),
    NoDealTarget,
    NoFace(u32),
    Partial {
        applied: Vec<LogEntry>,
        error: Box<SessionError>,
    },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(error) => write!(f, "{error}"),
            Self::Engine(fault) => write!(f, "{fault}"),
            Self::UnknownSeat(seat) => write!(f, "seat {seat} is not at this table"),
            Self::NoDealTarget => f.write_str("this table has no zones that deal can land in"),
            Self::NoFace(card) => write!(f, "the dealer holds no face for card {card}"),
            Self::Partial { applied, error } => write!(
                f,
                "{} of the intent's entries folded before it stopped: {error}",
                applied.len()
            ),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<FoldError> for SessionError {
    fn from(error: FoldError) -> Self {
        Self::Refused(error)
    }
}

impl From<EngineFault> for SessionError {
    fn from(fault: EngineFault) -> Self {
        Self::Engine(fault)
    }
}

impl SessionError {
    pub fn applied(&self) -> &[LogEntry] {
        match self {
            Self::Partial { applied, .. } => applied,
            _ => &[],
        }
    }

    pub fn is_engine_fault(&self) -> bool {
        match self {
            Self::Engine(_) => true,
            Self::Partial { error, .. } => error.is_engine_fault(),
            _ => false,
        }
    }

    pub fn is_refusal(&self) -> bool {
        match self {
            Self::Refused(error) => error.is_rejected(),
            Self::Partial { error, .. } => error.is_refusal(),
            _ => false,
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Refused(error) => error.reason(),
            Self::Partial { error, .. } => error.reason(),
            _ => None,
        }
    }
}

pub struct HostSession {
    roster: Vec<SeatInfo>,
    log: Vec<LogEntry>,
    engine: Box<dyn Engine>,
    plugin: Option<Box<dyn PluginModule>>,
    state_cache: LogState,
    mirror: TableView,
    dealer: BTreeMap<u32, CardFace>,
    sent_faces: BTreeSet<(u8, u32)>,
    hidden_plays: BTreeMap<u32, HiddenPlay>,
    seat_nodes: BTreeMap<String, u8>,
    next_card_id: u32,
    served: BTreeMap<[u8; 32], Vec<u8>>,
}

impl HostSession {
    pub fn new(host_name: &str) -> Self {
        Self::with_config(host_name, TableConfig::default())
    }

    pub fn with_config(host_name: &str, config: TableConfig) -> Self {
        Self::with_engine(host_name, config, Box::new(NativeEngine::new()), None)
            .expect("the native engine folds its own genesis")
    }

    pub fn with_engine(
        host_name: &str,
        mut config: TableConfig,
        engine: Box<dyn Engine>,
        plugin: Option<Box<dyn PluginModule>>,
    ) -> Result<Self, SessionError> {
        if let Some(hash) = engine.engine_hash() {
            config.engine = Some(engine_blob_ref(hash));
        }
        if let Some(hash) = plugin.as_ref().and_then(|plugin| plugin.module_hash()) {
            config.plugin = Some(engine_blob_ref(hash));
        }
        let mut session = Self {
            roster: vec![SeatInfo {
                seat: 0,
                name: host_name.into(),
                host: true,
                connected: true,
                color: default_seat_color(0),
                playmat: None,
            }],
            log: Vec::new(),
            engine,
            plugin,
            state_cache: LogState::new(),
            mirror: TableView::default(),
            dealer: BTreeMap::new(),
            sent_faces: BTreeSet::new(),
            hidden_plays: BTreeMap::new(),
            seat_nodes: BTreeMap::new(),
            next_card_id: 0,
            served: BTreeMap::new(),
        };
        session.append(
            0,
            LogAction::Genesis {
                name: host_name.into(),
                config,
            },
        )?;
        Ok(session)
    }

    pub fn host_from(host_name: &str, solo: &Table) -> Self {
        Self::host_from_with(
            host_name,
            solo,
            TableConfig::default(),
            Box::new(NativeEngine::new()),
            None,
        )
        .expect("the native engine folds a solo table")
    }

    pub fn host_from_with(
        host_name: &str,
        solo: &Table,
        config: TableConfig,
        engine: Box<dyn Engine>,
        plugin: Option<Box<dyn PluginModule>>,
    ) -> Result<Self, SessionError> {
        let zoned = !config.zones.is_empty();
        let mut session = Self::with_engine(host_name, config, engine, plugin)?;
        if zoned || solo.is_empty() {
            return Ok(session);
        }
        let ids: Vec<u32> = solo.cards().iter().map(|card| card.id.0).collect();
        for card in solo.cards() {
            session.dealer.insert(card.id.0, card.face.clone());
        }
        session.next_card_id = ids.iter().max().map(|id| id + 1).unwrap_or(0);
        session.append(
            0,
            LogAction::Deal {
                cards: ids,
                to: Zone::Hand,
            },
        )?;
        let mut board_seats: Vec<PlayerId> = solo
            .cards()
            .iter()
            .filter(|card| card.zone == Zone::Board)
            .map(|card| card.seat)
            .collect();
        board_seats.sort();
        board_seats.dedup();
        for seat in board_seats {
            let placed: Vec<u32> = solo
                .in_area(seat, Zone::Board)
                .map(|card| card.id.0)
                .collect();
            for (index, card) in placed.into_iter().enumerate() {
                session.append(
                    0,
                    LogAction::Move {
                        card,
                        to: Zone::Board,
                        seat: seat.0,
                        index: index as u32,
                        hidden: false,
                    },
                )?;
                let face = session
                    .dealer
                    .get(&card)
                    .cloned()
                    .ok_or(SessionError::NoFace(card))?;
                session.append(0, LogAction::Reveal { card, face })?;
            }
        }
        Ok(session)
    }

    fn append(&mut self, seat: u8, action: LogAction) -> Result<LogEntry, SessionError> {
        let entry = LogEntry::new(self.state_cache.next_seq, seat, action);
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
            FoldMode::Admission,
            0,
        )?;
        outcome.result?;
        apply_deltas(&mut self.mirror, &outcome.deltas);
        for card in self.state_cache.table.cards() {
            if !card.face.is_hidden() && !self.dealer.contains_key(&card.id.0) {
                self.dealer.insert(card.id.0, card.face.clone());
            }
        }
        let table = &self.state_cache.table;
        self.hidden_plays.retain(|card, at| {
            table
                .get(CardId(*card))
                .is_some_and(|held| (held.seat, held.zone) == (at.seat, at.zone))
        });
        if let LogAction::Move {
            card, hidden: true, ..
        } = &entry.action
        {
            let hider = entry.seat;
            self.sent_faces
                .retain(|(seat, held)| held != card || *seat == hider);
        }
        self.log.push(entry.clone());
        Ok(entry)
    }

    pub fn hidden_plays(&self) -> Vec<u32> {
        self.hidden_plays.keys().copied().collect()
    }

    fn seated(&self, seat: u8) -> Result<(), SessionError> {
        if self.roster.iter().any(|info| info.seat == seat) {
            Ok(())
        } else {
            Err(SessionError::UnknownSeat(seat))
        }
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
            &self.dealer,
        ));
        match plugin.view(&request) {
            Ok(view) => view,
            Err(error) => PluginView {
                status: vec![format!("the table's presenter faulted: {error}")],
                ..PluginView::default()
            },
        }
    }

    pub fn engine_hash(&self) -> Option<[u8; 32]> {
        self.engine.engine_hash()
    }

    pub fn pinned_hashes(&self) -> Vec<[u8; 32]> {
        self.engine
            .engine_hash()
            .into_iter()
            .chain(self.plugin.as_ref().and_then(|plugin| plugin.module_hash()))
            .collect()
    }

    pub fn serve_module(&mut self, bytes: Vec<u8>) -> Option<[u8; 32]> {
        let hash = *blake3::hash(&bytes).as_bytes();
        if !self.pinned_hashes().contains(&hash) {
            return None;
        }
        self.served.insert(hash, bytes);
        Some(hash)
    }

    pub fn served_modules(&self) -> Vec<[u8; 32]> {
        self.served.keys().copied().collect()
    }

    pub fn module_frames(&self, hash: [u8; 32]) -> Vec<HostMsg> {
        let hex = hash_hex(&hash);
        if !self.pinned_hashes().contains(&hash) {
            return vec![HostMsg::NoModule {
                hash,
                reason: format!("this table pinned no module {hex}"),
            }];
        }
        match self.served.get(&hash) {
            Some(bytes) => module_chunks(hash, bytes),
            None => vec![HostMsg::NoModule {
                hash,
                reason: format!("the host holds no bytes for its pinned module {hex}"),
            }],
        }
    }

    pub fn roster(&self) -> Vec<SeatInfo> {
        self.roster.clone()
    }

    pub fn seat_count(&self) -> usize {
        self.roster.len()
    }

    #[must_use = "the join entry must reach every other seat"]
    pub fn join(&mut self, name: &str) -> Result<(u8, LogEntry), SessionError> {
        let seat = self
            .roster
            .iter()
            .map(|info| info.seat + 1)
            .max()
            .unwrap_or(0);
        let entry = self.append(seat, LogAction::Join { name: name.into() })?;
        self.roster.push(SeatInfo {
            seat,
            name: name.into(),
            host: false,
            connected: true,
            color: self.free_color(default_seat_color(seat)),
            playmat: None,
        });
        Ok((seat, entry))
    }

    #[must_use = "a fresh join entry must reach every other seat"]
    pub fn join_as(
        &mut self,
        node: &str,
        name: &str,
    ) -> Result<(u8, Option<LogEntry>), SessionError> {
        if let Some(seat) = self.seat_nodes.get(node).copied() {
            if let Some(info) = self.roster.iter_mut().find(|info| info.seat == seat) {
                info.name = name.into();
                info.connected = true;
                return Ok((seat, None));
            }
        }
        let (seat, entry) = self.join(name)?;
        self.seat_nodes.insert(node.into(), seat);
        Ok((seat, Some(entry)))
    }

    pub fn seat_of_node(&self, node: &str) -> Option<u8> {
        self.seat_nodes.get(node).copied()
    }

    pub fn faces_owed_to(&self, seat: u8) -> OwnerFaces {
        self.state_cache
            .table
            .cards()
            .iter()
            .filter(|card| !self.state_cache.revealed.contains(&card.id.0))
            .filter(|card| self.face_owed_to(card, seat))
            .filter_map(|card| self.face_of(card.id.0))
            .collect()
    }

    fn face_owed_to(&self, card: &agni_core::Card, seat: u8) -> bool {
        let owned = card.seat == PlayerId(seat)
            && zone_visibility(&self.state_cache.zones, card.zone) == Some(ZoneVisibility::Owner);
        owned || self.state_cache.peeked_by(card.id.0, seat)
    }

    pub fn disconnect_all_guests(&mut self) {
        for info in self.roster.iter_mut() {
            if !info.host {
                info.connected = false;
            }
        }
    }

    fn color_taken(&self, color: u8, except: u8) -> bool {
        self.roster
            .iter()
            .any(|info| info.seat != except && info.color == color)
    }

    fn free_color(&self, preferred: u8) -> u8 {
        if !self.color_taken(preferred, u8::MAX) {
            return preferred;
        }
        (0..SEAT_PICKABLE_COLORS)
            .find(|color| !self.color_taken(*color, u8::MAX))
            .unwrap_or(preferred)
    }

    pub fn pick_color(&mut self, seat: u8, color: u8) -> bool {
        if color >= SEAT_PICKABLE_COLORS || self.color_taken(color, seat) {
            return false;
        }
        match self.roster.iter_mut().find(|info| info.seat == seat) {
            Some(info) => {
                info.color = color;
                true
            }
            None => false,
        }
    }

    pub fn pick_playmat(&mut self, seat: u8, playmat: Option<String>) -> bool {
        match self.roster.iter_mut().find(|info| info.seat == seat) {
            Some(info) => {
                let changed = info.playmat != playmat;
                info.playmat = playmat;
                changed
            }
            None => false,
        }
    }

    pub fn disconnect(&mut self, seat: u8) {
        if let Some(info) = self.roster.iter_mut().find(|info| info.seat == seat) {
            info.connected = false;
        }
    }

    #[must_use = "the deal entry must reach every seat and the faces their owner"]
    pub fn deal(
        &mut self,
        seat: u8,
        faces: Vec<CardFace>,
    ) -> Result<(LogEntry, OwnerFaces), SessionError> {
        self.deal_to(seat, faces, Zone::Hand)
    }

    #[must_use = "the deal entry must reach every seat and the faces their owner"]
    pub fn deal_to(
        &mut self,
        seat: u8,
        faces: Vec<CardFace>,
        to: Zone,
    ) -> Result<(LogEntry, OwnerFaces), SessionError> {
        self.seated(seat)?;
        if faces.is_empty() {
            return Err(FoldError::NoOp.into());
        }
        let visibility =
            zone_visibility(&self.state_cache.zones, to).ok_or(FoldError::UnknownZone)?;
        let ids: Vec<u32> = faces
            .iter()
            .map(|_| {
                let id = self.next_card_id;
                self.next_card_id += 1;
                id
            })
            .collect();
        let wire: OwnerFaces = if visibility == ZoneVisibility::None {
            Vec::new()
        } else {
            ids.iter()
                .zip(&faces)
                .map(|(id, face)| (*id, face.clone()))
                .collect()
        };
        for (id, face) in ids.iter().zip(faces) {
            self.dealer.insert(*id, face);
        }
        let entry = self.append(seat, LogAction::Deal { cards: ids, to })?;
        Ok((entry, wire))
    }

    pub fn zone_named(&self, name: &str) -> Option<&ZoneDecl> {
        self.state_cache.zones.iter().find(|decl| decl.name == name)
    }

    fn occupancy(&self, decl: &ZoneDecl, seat: u8) -> usize {
        let holder = match decl.owner {
            ZoneOwner::Shared => 0,
            ZoneOwner::PerSeat => seat,
        };
        self.state_cache
            .table
            .in_area(PlayerId(holder), Zone::Plugin(decl.id))
            .count()
    }

    fn hand_decl(&self) -> Option<ZoneDecl> {
        self.state_cache
            .zones
            .iter()
            .find(|decl| decl.kind == ZoneKind::Hand && decl.owner == ZoneOwner::PerSeat)
            .cloned()
    }

    fn draw_dealt(
        &mut self,
        seat: u8,
        from: &ZoneDecl,
        count: u32,
        entries: &mut Vec<LogEntry>,
        owner_faces: &mut OwnerFaces,
    ) -> Result<(), SessionError> {
        if count == 0 {
            return Ok(());
        }
        let Some(hand) = self.hand_decl() else {
            return Ok(());
        };
        let holder = match from.owner {
            ZoneOwner::Shared => 0,
            ZoneOwner::PerSeat => seat,
        };
        for _ in 0..count {
            let Some(card) = self
                .state_cache
                .table
                .in_area(PlayerId(holder), Zone::Plugin(from.id))
                .last()
                .map(|card| card.id.0)
            else {
                break;
            };
            let index = self
                .state_cache
                .table
                .in_area(PlayerId(seat), Zone::Plugin(hand.id))
                .count() as u32;
            let entry = self.append(
                seat,
                LogAction::Move {
                    card,
                    to: Zone::Plugin(hand.id),
                    seat,
                    index,
                    hidden: false,
                },
            )?;
            entries.push(entry);
            if hand.visibility != ZoneVisibility::All {
                if let Some(face) = self.face_of(card) {
                    owner_faces.push(face);
                }
            }
        }
        Ok(())
    }

    fn deal_seed(&self, seat: u8, groups: &[DealGroup]) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[seat]);
        hasher.update(&self.state_cache.next_seq.to_le_bytes());
        for group in groups {
            for face in &group.faces {
                hasher.update(face.name.as_bytes());
                hasher.update(&[0]);
            }
        }
        let hash = hasher.finalize();
        let mut seed = [0u8; 8];
        seed.copy_from_slice(&hash.as_bytes()[..8]);
        u64::from_le_bytes(seed)
    }

    fn deal_faces(
        &mut self,
        seat: u8,
        faces: Vec<CardFace>,
        decl: &ZoneDecl,
        entries: &mut Vec<LogEntry>,
        owner_faces: &mut OwnerFaces,
    ) -> Result<(), SessionError> {
        let (entry, wire) = self.deal_to(seat, faces, Zone::Plugin(decl.id))?;
        entries.push(entry);
        match decl.visibility {
            ZoneVisibility::All => {
                for (card, face) in wire {
                    entries.push(self.append(seat, LogAction::Reveal { card, face })?);
                }
            }
            ZoneVisibility::Owner => owner_faces.extend(wire),
            ZoneVisibility::None => {}
        }
        Ok(())
    }

    fn groups_land(&self, groups: &[DealGroup]) -> bool {
        !groups.is_empty()
            && groups.iter().all(|group| match &group.target {
                DealTarget::Zone(name) => self.zone_named(name).is_some(),
                DealTarget::Spread(prefix) => self
                    .state_cache
                    .zones
                    .iter()
                    .any(|decl| decl.name.starts_with(prefix.as_str())),
            })
    }

    #[must_use = "the clear entry must reach every seat"]
    pub fn clear_seat(&mut self, seat: u8) -> Result<Option<LogEntry>, SessionError> {
        self.seated(seat)?;
        let swept: Vec<u32> = self
            .state_cache
            .table
            .cards()
            .iter()
            .filter(|card| card.owner == PlayerId(seat))
            .map(|card| card.id.0)
            .collect();
        if swept.is_empty() {
            return Ok(None);
        }
        let entry = self.append(seat, LogAction::Clear { seat })?;
        for card in swept {
            self.dealer.remove(&card);
        }
        Ok(Some(entry))
    }

    #[must_use = "the entries must reach every seat and the faces their owner"]
    pub fn reload_groups(
        &mut self,
        seat: u8,
        groups: Vec<DealGroup>,
    ) -> Result<(Vec<LogEntry>, OwnerFaces), SessionError> {
        self.seated(seat)?;
        let staged: Vec<DealGroup> = groups
            .into_iter()
            .filter(|group| !group.faces.is_empty())
            .collect();
        if !self.groups_land(&staged) {
            return Err(SessionError::NoDealTarget);
        }
        let mut entries: Vec<LogEntry> = self.clear_seat(seat)?.into_iter().collect();
        let (dealt, owner_faces) = self.deal_groups(seat, staged)?;
        entries.extend(dealt);
        Ok((entries, owner_faces))
    }

    #[must_use = "the entries must reach every seat and the faces their owner"]
    pub fn deal_groups(
        &mut self,
        seat: u8,
        groups: Vec<DealGroup>,
    ) -> Result<(Vec<LogEntry>, OwnerFaces), SessionError> {
        self.seated(seat)?;
        let groups: Vec<DealGroup> = groups
            .into_iter()
            .filter(|group| !group.faces.is_empty())
            .collect();
        if !self.groups_land(&groups) {
            return Err(SessionError::NoDealTarget);
        }
        let mut rng = Rng::from_seed(self.deal_seed(seat, &groups));
        let mut entries = Vec::new();
        let mut owner_faces = Vec::new();
        for group in groups {
            let mut faces = group.faces;
            if group.shuffle {
                for i in (1..faces.len()).rev() {
                    let j = rng.below((i + 1) as u32) as usize;
                    faces.swap(i, j);
                }
            }
            match group.target {
                DealTarget::Zone(name) => {
                    let decl = self
                        .zone_named(&name)
                        .cloned()
                        .ok_or(SessionError::NoDealTarget)?;
                    self.deal_faces(seat, faces, &decl, &mut entries, &mut owner_faces)?;
                    self.draw_dealt(seat, &decl, group.draw, &mut entries, &mut owner_faces)?;
                }
                DealTarget::Spread(prefix) => {
                    let decls: Vec<ZoneDecl> = self
                        .state_cache
                        .zones
                        .iter()
                        .filter(|decl| decl.name.starts_with(prefix.as_str()))
                        .cloned()
                        .collect();
                    if decls.is_empty() {
                        return Err(SessionError::NoDealTarget);
                    }
                    let mut counts: Vec<usize> = decls
                        .iter()
                        .map(|decl| self.occupancy(decl, seat))
                        .collect();
                    let mut batches: Vec<Vec<CardFace>> = vec![Vec::new(); decls.len()];
                    for face in faces {
                        let pick = (0..decls.len())
                            .min_by_key(|&i| (counts[i], i))
                            .unwrap_or(0);
                        counts[pick] += 1;
                        batches[pick].push(face);
                    }
                    for (decl, batch) in decls.iter().zip(batches) {
                        if batch.is_empty() {
                            continue;
                        }
                        self.deal_faces(seat, batch, decl, &mut entries, &mut owner_faces)?;
                    }
                }
            }
        }
        Ok((entries, owner_faces))
    }

    pub fn face_of(&self, card: u32) -> Option<(u32, CardFace)> {
        self.dealer.get(&card).map(|face| (card, face.clone()))
    }

    #[must_use = "the entries must reach every seat"]
    pub fn intent(&mut self, from: u8, intent: WireIntent) -> Result<Vec<LogEntry>, SessionError> {
        let mut unhide = None;
        let hidden = match &intent {
            WireIntent::MoveHidden { card, to, seat, .. } => {
                let held_by = match agni_sim::wire::zone_owner(&self.state_cache.zones, *to) {
                    Some(agni_sim::wire::ZoneOwner::PerSeat) => *seat,
                    _ => 0,
                };
                Some((
                    *card,
                    HiddenPlay {
                        seat: PlayerId(held_by),
                        zone: *to,
                        by: from,
                    },
                ))
            }
            WireIntent::Reveal { card } => {
                if self.hidden_plays.get(card).is_some_and(|at| at.by != from) {
                    return Err(SessionError::Refused(FoldError::Rejected {
                        reason: Some("that is not your card to reveal".into()),
                    }));
                }
                unhide = self.hidden_plays.contains_key(card).then_some(*card);
                None
            }
            _ => None,
        };
        let action = match intent {
            WireIntent::Reveal { card } => LogAction::Reveal {
                card,
                face: self
                    .dealer
                    .get(&card)
                    .cloned()
                    .ok_or(SessionError::NoFace(card))?,
            },
            other => LogAction::from(other),
        };
        let probe = LogEntry::new(self.state_cache.next_seq, from, action.clone());
        validate(&self.state_cache, &probe)?;
        let reveal = match &action {
            LogAction::Move { .. } if hidden.is_some() => None,
            LogAction::Move { card, to, .. } => {
                let entering_public =
                    zone_visibility(&self.state_cache.zones, *to) == Some(ZoneVisibility::All);
                (entering_public && !self.state_cache.revealed.contains(card)).then_some(*card)
            }
            _ => None,
        };
        let Some(card) = reveal else {
            let displaced = hidden.map(|(card, at)| (card, self.hidden_plays.insert(card, at)));
            let entry = match self.append(from, action) {
                Ok(entry) => entry,
                Err(error) => {
                    if let Some((card, before)) = displaced {
                        match before {
                            Some(at) => self.hidden_plays.insert(card, at),
                            None => self.hidden_plays.remove(&card),
                        };
                    }
                    return Err(error);
                }
            };
            if let Some(card) = unhide {
                self.hidden_plays.remove(&card);
            }
            let mut entries = vec![entry];
            entries.extend(self.reveal_surfaced()?);
            return Ok(entries);
        };
        let face = self
            .dealer
            .get(&card)
            .cloned()
            .ok_or(SessionError::NoFace(card))?;
        let reveal = LogAction::Reveal { card, face };
        let from_hidden = self
            .state_cache
            .table
            .get(CardId(card))
            .map(|held| zone_visibility(&self.state_cache.zones, held.zone))
            == Some(Some(ZoneVisibility::None));
        let (first, second) = if from_hidden {
            (action, reveal)
        } else {
            self.decide_after(from, &reveal, &action)?;
            (reveal, action)
        };
        let mut entries = vec![self.append(from, first)?];
        match self.append(from, second) {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                return Err(SessionError::Partial {
                    applied: entries,
                    error: Box::new(error),
                })
            }
        }
        match self.reveal_surfaced() {
            Ok(revealed) => entries.extend(revealed),
            Err(error) => {
                return Err(SessionError::Partial {
                    applied: entries,
                    error: Box::new(error),
                })
            }
        }
        Ok(entries)
    }

    fn reveal_surfaced(&mut self) -> Result<Vec<LogEntry>, SessionError> {
        let surfaced: Vec<(u8, u32, CardFace)> = self
            .state_cache
            .table
            .cards()
            .iter()
            .filter(|card| !self.state_cache.revealed.contains(&card.id.0))
            .filter_map(|card| {
                let visibility = zone_visibility(&self.state_cache.zones, card.zone)?;
                let owed = self.state_cache.owed_reveals.contains(&card.id.0);
                let seat = match visibility {
                    ZoneVisibility::All if owed => self
                        .hidden_plays
                        .get(&card.id.0)
                        .map(|at| at.by)
                        .unwrap_or(0),
                    ZoneVisibility::All if !self.hidden_plays.contains_key(&card.id.0) => 0,
                    ZoneVisibility::Owner if owed => card.seat.0,
                    ZoneVisibility::None if owed => card.owner.0,
                    _ => return None,
                };
                let face = self.dealer.get(&card.id.0)?.clone();
                Some((seat, card.id.0, face))
            })
            .collect();
        let mut entries = Vec::new();
        for (seat, card, face) in surfaced {
            entries.push(self.append(seat, LogAction::Reveal { card, face })?);
            self.hidden_plays.remove(&card);
        }
        Ok(entries)
    }

    fn decide_after(
        &mut self,
        from: u8,
        first: &LogAction,
        then: &LogAction,
    ) -> Result<(), SessionError> {
        let Some(plugin) = self.plugin.as_deref_mut() else {
            return Ok(());
        };
        let mut preview = self.state_cache.clone();
        let opener = LogEntry::new(preview.next_seq, from, first.clone());
        fold_entry(&mut preview, &opener)?;
        let entry = LogEntry::new(preview.next_seq, from, then.clone());
        let Some(request) = native_decide_request(&preview, &entry) else {
            validate(&preview, &entry)?;
            return Ok(());
        };
        let verdict = plugin.decide(&encode(&request))?;
        if verdict.accept {
            Ok(())
        } else {
            Err(SessionError::Refused(verdict.refusal()))
        }
    }

    pub fn owed_faces(&mut self) -> Vec<(u8, (u32, CardFace))> {
        let mut owed = Vec::new();
        let seats: Vec<u8> = self.roster.iter().map(|info| info.seat).collect();
        for card in self.state_cache.table.cards() {
            if self.state_cache.revealed.contains(&card.id.0) {
                continue;
            }
            for seat in &seats {
                if !self.face_owed_to(card, *seat) {
                    continue;
                }
                let key = (*seat, card.id.0);
                if self.sent_faces.contains(&key) {
                    continue;
                }
                if let Some(face) = self.dealer.get(&card.id.0) {
                    owed.push((*seat, (card.id.0, face.clone())));
                    self.sent_faces.insert(key);
                }
            }
        }
        owed
    }

    #[must_use = "the reset entries must reach every seat and the faces their owner"]
    pub fn reset(
        &mut self,
        hands: Vec<(u8, Vec<CardFace>)>,
    ) -> Result<(Vec<LogEntry>, Vec<(u8, OwnerFaces)>), SessionError> {
        let mut entries = vec![self.append(0, LogAction::Reset)?];
        self.dealer.clear();
        self.sent_faces.clear();
        self.hidden_plays.clear();
        let mut faces_by_seat = Vec::new();
        for (seat, faces) in hands {
            if faces.is_empty() {
                continue;
            }
            let (entry, wire) = self.deal(seat, faces)?;
            entries.push(entry);
            faces_by_seat.push((seat, wire));
        }
        Ok((entries, faces_by_seat))
    }

    fn face_known_to(&self, card: u32, seat: u8) -> bool {
        if self.state_cache.revealed.contains(&card) || self.state_cache.peeked_by(card, seat) {
            return true;
        }
        if let Some(at) = self.hidden_plays.get(&card) {
            return at.by == seat;
        }
        self.state_cache
            .table
            .get(CardId(card))
            .is_some_and(|held| {
                held.owner.0 == seat
                    && zone_visibility(&self.state_cache.zones, held.zone)
                        != Some(ZoneVisibility::None)
            })
    }

    fn faces_seen_by(&self, seat: u8) -> BTreeMap<u32, CardFace> {
        self.dealer
            .iter()
            .filter(|(card, _)| self.face_known_to(**card, seat))
            .map(|(card, face)| (*card, face.clone()))
            .collect()
    }

    pub fn table_for(&self, seat: u8) -> Table {
        view_to_table(&self.mirror, &self.faces_seen_by(seat))
    }

    pub fn table(&self) -> Table {
        self.table_for(0)
    }

    pub fn view(&self) -> &TableView {
        &self.mirror
    }

    pub fn private_faces(&self, entries: &[LogEntry]) -> Vec<(u8, (u32, CardFace))> {
        let mut owed = Vec::new();
        for (card, seat) in &self.state_cache.peeks {
            if let Some(face) = self.face_of(*card) {
                owed.push((*seat, face));
            }
        }
        for entry in entries {
            let LogAction::Move { card, .. } = &entry.action else {
                continue;
            };
            if self.state_cache.revealed.contains(card) {
                continue;
            }
            let Some(current) = self.state_cache.table.get(CardId(*card)) else {
                continue;
            };
            if zone_visibility(&self.state_cache.zones, current.zone) != Some(ZoneVisibility::Owner)
            {
                continue;
            }
            if let Some(face) = self.face_of(*card) {
                owed.push((current.seat.0, face));
            }
        }
        owed
    }
}

pub fn solo_move(table: &mut Table, intent: &WireIntent) -> bool {
    match intent {
        WireIntent::Move {
            card,
            to,
            seat,
            index,
        }
        | WireIntent::MoveHidden {
            card,
            to,
            seat,
            index,
        } => table.apply(Intent::MoveCard {
            card: CardId(*card),
            to: *to,
            seat: PlayerId(*seat),
            index: *index as usize,
        }),
        _ => false,
    }
}
