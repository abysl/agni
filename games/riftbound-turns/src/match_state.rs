use agni_plugin_sdk::blob::{MapReader, MapWriter};
use agni_plugin_sdk::cbor::{Reader, Writer};

pub const MATCH_WINS_REQUIRED: u8 = 2;
pub const SEATS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    Won(u8),
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartChoice {
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentGame {
    pub first: u8,
    pub battlefields: [String; SEATS],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedGame {
    pub first: u8,
    pub result: GameResult,
    pub battlefields: [String; SEATS],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartPolicy {
    Opening { chooser: u8 },
    Loser { chooser: u8 },
    Fixed { actor: u8, first: u8 },
    Unavailable,
}

impl StartPolicy {
    pub fn first_for(&self, actor: u8, choice: StartChoice) -> Option<u8> {
        match *self {
            Self::Opening { chooser } | Self::Loser { chooser } if actor == chooser => {
                Some(match choice {
                    StartChoice::First => chooser,
                    StartChoice::Last => other(chooser),
                })
            }
            _ => None,
        }
    }

    pub fn allows_first(&self, actor: u8, first: u8) -> bool {
        match *self {
            Self::Opening { chooser } | Self::Loser { chooser } => {
                actor == chooser && (first == chooser || first == other(chooser))
            }
            Self::Fixed {
                actor: expected,
                first: expected_first,
            } => actor == expected && first == expected_first,
            Self::Unavailable => false,
        }
    }

    pub fn actor(&self) -> Option<u8> {
        match *self {
            Self::Opening { chooser } | Self::Loser { chooser } => Some(chooser),
            Self::Fixed { actor, .. } => Some(actor),
            Self::Unavailable => None,
        }
    }

    pub fn fixed_first(&self) -> Option<u8> {
        match *self {
            Self::Fixed { first, .. } => Some(first),
            Self::Opening { .. } | Self::Loser { .. } | Self::Unavailable => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionPolicy {
    Unused {
        used: [Vec<String>; SEATS],
        sideboard_allowed: bool,
    },
    Exact {
        battlefields: [String; SEATS],
        sideboard_allowed: bool,
    },
    Unavailable,
}

impl SelectionPolicy {
    pub fn sideboard_allowed(&self) -> bool {
        match self {
            Self::Unused {
                sideboard_allowed, ..
            }
            | Self::Exact {
                sideboard_allowed, ..
            } => *sideboard_allowed,
            Self::Unavailable => false,
        }
    }

    pub fn required(&self) -> Option<&[String; SEATS]> {
        match self {
            Self::Exact { battlefields, .. } => Some(battlefields),
            Self::Unused { .. } | Self::Unavailable => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSummary {
    pub generation: u64,
    pub wins: [u8; SEATS],
    pub previous: Option<CompletedGame>,
    pub current: Option<CurrentGame>,
    pub result: Option<GameResult>,
    pub start: StartPolicy,
    pub selection: SelectionPolicy,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    Recorded,
    AlreadyRecorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchError {
    InvalidSeat,
    MatchComplete,
    GameAlreadyStarted,
    GameAlreadyFinished,
    UnfinishedGame,
    DuplicateReset,
    ResultAlreadyRecorded,
    WrongActor,
    WrongFirst,
    ChoiceUnavailable,
    BattlefieldMissing,
    BattlefieldUsed,
    GenerationOverflow,
    ScoreOverflow,
    InvalidState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchState {
    pub generation: u64,
    pub opening_chooser: u8,
    pub wins: [u8; SEATS],
    pub previous: Option<CompletedGame>,
    pub current: Option<CurrentGame>,
    pub result: Option<GameResult>,
    pub used: [Vec<String>; SEATS],
}

impl MatchState {
    pub fn new(opening_chooser: u8) -> Result<Self, MatchError> {
        valid_seat(opening_chooser)?;
        Ok(Self {
            generation: 0,
            opening_chooser,
            wins: [0; SEATS],
            previous: None,
            current: None,
            result: None,
            used: [Vec::new(), Vec::new()],
        })
    }

    pub fn start_policy(&self) -> StartPolicy {
        if self.complete() || self.current.is_some() || valid_seat(self.opening_chooser).is_err() {
            return StartPolicy::Unavailable;
        }
        match self.previous.as_ref() {
            None => StartPolicy::Opening {
                chooser: self.opening_chooser,
            },
            Some(game) => match game.result {
                GameResult::Won(winner) => StartPolicy::Loser {
                    chooser: other(winner),
                },
                GameResult::Draw => StartPolicy::Fixed {
                    actor: game.first,
                    first: game.first,
                },
            },
        }
    }

    pub fn selection_policy(&self) -> SelectionPolicy {
        if self.complete() || self.current.is_some() || valid_seat(self.opening_chooser).is_err() {
            return SelectionPolicy::Unavailable;
        }
        match self.previous.as_ref() {
            Some(CompletedGame {
                result: GameResult::Draw,
                battlefields,
                ..
            }) => SelectionPolicy::Exact {
                battlefields: battlefields.clone(),
                sideboard_allowed: false,
            },
            Some(CompletedGame {
                result: GameResult::Won(_),
                ..
            }) => SelectionPolicy::Unused {
                used: self.used.clone(),
                sideboard_allowed: true,
            },
            None => SelectionPolicy::Unused {
                used: self.used.clone(),
                sideboard_allowed: false,
            },
        }
    }

    pub fn summary(&self) -> MatchSummary {
        MatchSummary {
            generation: self.generation,
            wins: self.wins,
            previous: self.previous.clone(),
            current: self.current.clone(),
            result: self.result,
            start: self.start_policy(),
            selection: self.selection_policy(),
            complete: self.complete(),
        }
    }

    pub fn complete(&self) -> bool {
        self.wins.iter().any(|wins| *wins >= MATCH_WINS_REQUIRED)
    }

    pub fn start_with_choice(
        &mut self,
        actor: u8,
        choice: StartChoice,
        battlefields: [&str; SEATS],
    ) -> Result<u8, MatchError> {
        if self.complete() {
            return Err(MatchError::MatchComplete);
        }
        if self.current.is_some() {
            return Err(if self.result.is_some() {
                MatchError::GameAlreadyFinished
            } else {
                MatchError::GameAlreadyStarted
            });
        }
        let policy = self.start_policy();
        if policy.actor().is_some_and(|expected| expected != actor) {
            return Err(MatchError::WrongActor);
        }
        let first = policy
            .first_for(actor, choice)
            .ok_or(MatchError::ChoiceUnavailable)?;
        self.start_with_first(actor, first, battlefields)
    }

    pub fn start_with_first(
        &mut self,
        actor: u8,
        first: u8,
        battlefields: [&str; SEATS],
    ) -> Result<u8, MatchError> {
        if self.complete() {
            return Err(MatchError::MatchComplete);
        }
        if self.current.is_some() {
            return Err(if self.result.is_some() {
                MatchError::GameAlreadyFinished
            } else {
                MatchError::GameAlreadyStarted
            });
        }
        valid_seat(actor)?;
        valid_seat(first)?;
        if !self.start_policy().allows_first(actor, first) {
            return Err(
                if self
                    .start_policy()
                    .actor()
                    .is_some_and(|seat| seat != actor)
                {
                    MatchError::WrongActor
                } else {
                    MatchError::WrongFirst
                },
            );
        }
        let battlefields = [
            self.canonical_battlefield_selection(0, battlefields[0])?,
            self.canonical_battlefield_selection(1, battlefields[1])?,
        ];
        self.current = Some(CurrentGame {
            first,
            battlefields,
        });
        Ok(first)
    }

    pub fn canonical_battlefield_selection(
        &self,
        seat: u8,
        raw_name: &str,
    ) -> Result<String, MatchError> {
        valid_seat(seat)?;
        if self.complete() {
            return Err(MatchError::MatchComplete);
        }
        if self.current.is_some() {
            return Err(if self.result.is_some() {
                MatchError::GameAlreadyFinished
            } else {
                MatchError::GameAlreadyStarted
            });
        }
        let name = canonical_battlefield_name(raw_name).ok_or(MatchError::BattlefieldMissing)?;
        match self.selection_policy() {
            SelectionPolicy::Unused { ref used, .. } => {
                if used[usize::from(seat)].iter().any(|used| used == &name) {
                    Err(MatchError::BattlefieldUsed)
                } else {
                    Ok(name)
                }
            }
            SelectionPolicy::Exact {
                ref battlefields, ..
            } => {
                if battlefields[usize::from(seat)] == name {
                    Ok(name)
                } else {
                    Err(MatchError::BattlefieldUsed)
                }
            }
            SelectionPolicy::Unavailable => Err(MatchError::MatchComplete),
        }
    }

    pub fn record_result(&mut self, result: GameResult) -> Result<RecordOutcome, MatchError> {
        let Some(current) = self.current.as_ref() else {
            return Err(MatchError::UnfinishedGame);
        };
        if let Some(previous) = self.result {
            return if previous == result {
                Ok(RecordOutcome::AlreadyRecorded)
            } else {
                Err(MatchError::ResultAlreadyRecorded)
            };
        }
        if let GameResult::Won(seat) = result {
            valid_seat(seat)?;
            let wins = self.wins[usize::from(seat)];
            self.wins[usize::from(seat)] = wins.checked_add(1).ok_or(MatchError::ScoreOverflow)?;
            for owner in 0..SEATS {
                insert_used(&mut self.used[owner], &current.battlefields[owner]);
            }
        }
        self.result = Some(result);
        Ok(RecordOutcome::Recorded)
    }

    pub fn reset_for_next_game(&mut self) -> Result<(), MatchError> {
        let Some(current) = self.current.as_ref() else {
            return Err(if self.previous.is_some() {
                MatchError::DuplicateReset
            } else {
                MatchError::UnfinishedGame
            });
        };
        let Some(result) = self.result else {
            return Err(MatchError::UnfinishedGame);
        };
        if self.complete() {
            return Err(MatchError::MatchComplete);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(MatchError::GenerationOverflow)?;
        let current = current.clone();
        self.previous = Some(CompletedGame {
            first: current.first,
            result,
            battlefields: current.battlefields,
        });
        self.current = None;
        self.result = None;
        self.generation = generation;
        Ok(())
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut map = MapWriter::new();
        map.field("g").unsigned(self.generation);
        map.field("o").unsigned(u64::from(self.opening_chooser));
        let wins = map.field("w");
        wins.array(SEATS);
        for win in self.wins {
            wins.unsigned(u64::from(win));
        }
        let used = map.field("u");
        used.array(SEATS);
        for names in &self.used {
            write_names(used, names);
        }
        write_completed(map.field("p"), self.previous.as_ref());
        write_current(map.field("c"), self.current.as_ref());
        write_result(map.field("r"), self.result);
        map.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader::new(bytes);
        let mut map = MapReader::open(&mut reader)?;
        let mut generation = None;
        let mut opening_chooser = None;
        let mut wins = None;
        let mut used = None;
        let mut previous = None;
        let mut current = None;
        let mut result = None;
        let mut seen_generation = false;
        let mut seen_opening = false;
        let mut seen_wins = false;
        let mut seen_used = false;
        while let Some(key) = map.field() {
            match key {
                "g" => {
                    generation = Some(map.value().unsigned()?);
                    seen_generation = true;
                }
                "o" => {
                    opening_chooser = Some(u8_of(map.value())?);
                    seen_opening = true;
                }
                "w" => {
                    wins = Some(read_wins(map.value())?);
                    seen_wins = true;
                }
                "u" => {
                    used = Some(read_used(map.value())?);
                    seen_used = true;
                }
                "p" => previous = Some(read_completed(map.value())?),
                "c" => current = Some(read_current(map.value())?),
                "r" => result = Some(read_result(map.value())?),
                _ => map.skip_unknown()?,
            }
        }
        map.finish()?;
        if !seen_generation
            || !seen_opening
            || !seen_wins
            || !seen_used
            || reader.position() != bytes.len()
        {
            return None;
        }
        let state = Self {
            generation: generation?,
            opening_chooser: opening_chooser?,
            wins: wins?,
            previous: previous?,
            current: current?,
            result: result?,
            used: used?,
        };
        state.validate().ok().map(|()| state)
    }

    fn validate(&self) -> Result<(), MatchError> {
        valid_seat(self.opening_chooser)?;
        if self.wins.iter().any(|wins| *wins > MATCH_WINS_REQUIRED)
            || self.wins.iter().all(|wins| *wins >= MATCH_WINS_REQUIRED)
        {
            return Err(MatchError::InvalidState);
        }
        if (self.generation == 0) != self.previous.is_none() {
            return Err(MatchError::InvalidState);
        }
        let current_game_count = self
            .generation
            .saturating_add(u64::from(self.result.is_some()));
        let total_wins = u64::from(self.wins[0]) + u64::from(self.wins[1]);
        if total_wins > current_game_count {
            return Err(MatchError::InvalidState);
        }
        if self.current.is_none() && self.result.is_some() {
            return Err(MatchError::InvalidState);
        }
        if self.complete() {
            let Some(GameResult::Won(winner)) = self.result else {
                return Err(MatchError::InvalidState);
            };
            if self.wins[usize::from(winner)] != MATCH_WINS_REQUIRED {
                return Err(MatchError::InvalidState);
            }
        }
        for names in &self.used {
            if !is_sorted_unique(names)
                || names
                    .iter()
                    .any(|name| canonical_battlefield_name(name) != Some(name.clone()))
            {
                return Err(MatchError::InvalidState);
            }
        }
        for seat in 0..SEATS {
            if self.used[seat].len() != total_wins as usize {
                return Err(MatchError::InvalidState);
            }
        }
        if let Some(current) = &self.current {
            valid_seat(current.first)?;
            validate_pair(&current.battlefields)?;
        }
        if let Some(previous) = &self.previous {
            valid_seat(previous.first)?;
            validate_pair(&previous.battlefields)?;
            if let GameResult::Won(seat) = previous.result {
                valid_seat(seat)?;
            }
        }
        if let (Some(previous), Some(current)) = (&self.previous, &self.current) {
            match previous.result {
                GameResult::Won(_) => {
                    if (0..SEATS)
                        .any(|seat| previous.battlefields[seat] == current.battlefields[seat])
                    {
                        return Err(MatchError::InvalidState);
                    }
                }
                GameResult::Draw => {
                    if current.first != previous.first
                        || current.battlefields != previous.battlefields
                    {
                        return Err(MatchError::InvalidState);
                    }
                }
            }
        }
        if let Some(GameResult::Won(seat)) = self.result {
            valid_seat(seat)?;
        }
        let mut known_wins = [0_u8; SEATS];
        let mut winning_pairs = Vec::new();
        if let Some(previous) = &self.previous {
            if let GameResult::Won(seat) = previous.result {
                if self.wins[usize::from(seat)] == 0 {
                    return Err(MatchError::InvalidState);
                }
                known_wins[usize::from(seat)] += 1;
                winning_pairs.push(previous.battlefields.clone());
            }
        }
        if let (Some(current), Some(GameResult::Won(seat))) = (&self.current, self.result) {
            if self.wins[usize::from(seat)] == 0 {
                return Err(MatchError::InvalidState);
            }
            known_wins[usize::from(seat)] += 1;
            winning_pairs.push(current.battlefields.clone());
        }
        if (0..SEATS).any(|seat| self.wins[seat] < known_wins[seat]) {
            return Err(MatchError::InvalidState);
        }
        require_used_pairs(&self.used, &winning_pairs)?;
        Ok(())
    }
}

fn other(seat: u8) -> u8 {
    match seat {
        0 => 1,
        _ => 0,
    }
}

fn valid_seat(seat: u8) -> Result<(), MatchError> {
    (usize::from(seat) < SEATS)
        .then_some(())
        .ok_or(MatchError::InvalidSeat)
}

pub fn canonical_battlefield_name(name: &str) -> Option<String> {
    let mut words = name.split_whitespace();
    let first = words.next()?;
    let mut output = first.to_lowercase();
    for word in words {
        output.push(' ');
        output.push_str(&word.to_lowercase());
    }
    Some(output)
}

fn validate_pair(names: &[String; SEATS]) -> Result<(), MatchError> {
    if names[0].is_empty() || names[1].is_empty() {
        return Err(MatchError::BattlefieldMissing);
    }
    if names
        .iter()
        .any(|name| canonical_battlefield_name(name) != Some(name.clone()))
    {
        return Err(MatchError::BattlefieldMissing);
    }
    Ok(())
}

fn insert_used(names: &mut Vec<String>, name: &str) {
    match names.binary_search_by(|candidate| candidate.as_str().cmp(name)) {
        Ok(_) => {}
        Err(index) => names.insert(index, name.to_owned()),
    }
}

fn is_sorted_unique(names: &[String]) -> bool {
    names.windows(2).all(|pair| pair[0] < pair[1])
}

fn require_used_pairs(
    used: &[Vec<String>; SEATS],
    winning_pairs: &[[String; SEATS]],
) -> Result<(), MatchError> {
    for seat in 0..SEATS {
        for pair in winning_pairs {
            let required_name = &pair[seat];
            let required_count = winning_pairs
                .iter()
                .filter(|candidate| candidate[seat] == *required_name)
                .count();
            let actual_count = used[seat]
                .iter()
                .filter(|name| *name == required_name)
                .count();
            if actual_count < required_count {
                return Err(MatchError::InvalidState);
            }
        }
    }
    Ok(())
}

fn write_names(writer: &mut Writer, names: &[String]) {
    writer.array(names.len());
    for name in names {
        writer.text(name);
    }
}

fn read_names(reader: &mut Reader<'_>) -> Option<Vec<String>> {
    let len = reader.array_len()?;
    let mut names = Vec::with_capacity(len);
    for _ in 0..len {
        names.push(reader.key()?.to_owned());
    }
    is_sorted_unique(&names).then_some(names)
}

fn write_pair(writer: &mut Writer, names: &[String; SEATS]) {
    writer.array(SEATS);
    writer.text(&names[0]);
    writer.text(&names[1]);
}

fn read_pair(reader: &mut Reader<'_>) -> Option<[String; SEATS]> {
    (reader.array_len()? == SEATS).then_some([reader.key()?.to_owned(), reader.key()?.to_owned()])
}

fn write_completed(writer: &mut Writer, game: Option<&CompletedGame>) {
    let Some(game) = game else {
        writer.map(0);
        return;
    };
    let mut map = MapWriter::new();
    map.field("f").unsigned(u64::from(game.first));
    map.field("r").unsigned(result_code(game.result));
    write_pair(map.field("b"), &game.battlefields);
    map.write_into(writer);
}

fn read_completed(reader: &mut Reader<'_>) -> Option<Option<CompletedGame>> {
    let mut map = MapReader::open(reader)?;
    if map.remaining() == 0 {
        map.finish()?;
        return Some(None);
    }
    let mut first = None;
    let mut result = None;
    let mut battlefields = None;
    while let Some(key) = map.field() {
        match key {
            "f" => first = Some(u8_of(map.value())?),
            "r" => result = Some(result_of(map.value())?),
            "b" => battlefields = Some(read_pair(map.value())?),
            _ => map.skip_unknown()?,
        }
    }
    map.finish()?;
    Some(Some(CompletedGame {
        first: first?,
        result: result?,
        battlefields: battlefields?,
    }))
}

fn write_current(writer: &mut Writer, game: Option<&CurrentGame>) {
    let Some(game) = game else {
        writer.map(0);
        return;
    };
    let mut map = MapWriter::new();
    map.field("f").unsigned(u64::from(game.first));
    write_pair(map.field("b"), &game.battlefields);
    map.write_into(writer);
}

fn read_current(reader: &mut Reader<'_>) -> Option<Option<CurrentGame>> {
    let mut map = MapReader::open(reader)?;
    if map.remaining() == 0 {
        map.finish()?;
        return Some(None);
    }
    let mut first = None;
    let mut battlefields = None;
    while let Some(key) = map.field() {
        match key {
            "f" => first = Some(u8_of(map.value())?),
            "b" => battlefields = Some(read_pair(map.value())?),
            _ => map.skip_unknown()?,
        }
    }
    map.finish()?;
    Some(Some(CurrentGame {
        first: first?,
        battlefields: battlefields?,
    }))
}

fn write_result(writer: &mut Writer, result: Option<GameResult>) {
    let Some(result) = result else {
        writer.map(0);
        return;
    };
    let mut map = MapWriter::new();
    map.field("k").unsigned(result_code(result));
    map.write_into(writer);
}

fn read_result(reader: &mut Reader<'_>) -> Option<Option<GameResult>> {
    let mut map = MapReader::open(reader)?;
    if map.remaining() == 0 {
        map.finish()?;
        return Some(None);
    }
    let mut result = None;
    while let Some(key) = map.field() {
        match key {
            "k" => result = Some(result_of(map.value())?),
            _ => map.skip_unknown()?,
        }
    }
    map.finish()?;
    Some(Some(result?))
}

fn result_code(result: GameResult) -> u64 {
    match result {
        GameResult::Won(seat) => 1 + u64::from(seat),
        GameResult::Draw => 0,
    }
}

fn result_of(reader: &mut Reader<'_>) -> Option<GameResult> {
    match reader.unsigned()? {
        0 => Some(GameResult::Draw),
        1 => Some(GameResult::Won(0)),
        2 => Some(GameResult::Won(1)),
        _ => None,
    }
}

fn u8_of(reader: &mut Reader<'_>) -> Option<u8> {
    u8::try_from(reader.unsigned()?).ok()
}

fn read_wins(reader: &mut Reader<'_>) -> Option<[u8; SEATS]> {
    (reader.array_len()? == SEATS).then_some([u8_of(reader)?, u8_of(reader)?])
}

fn read_used(reader: &mut Reader<'_>) -> Option<[Vec<String>; SEATS]> {
    (reader.array_len()? == SEATS).then_some([read_names(reader)?, read_names(reader)?])
}
