use crate::abi::{
    decode, encode, DecideRequest, FoldLogRequest, FoldRequest, ViewRequest, ENGINE_ABI_VERSION,
};
use crate::log::{
    decode_state, encode_state, fold_begin, fold_entry, fold_finish, validate, FoldError, LogEntry,
    LogState, Verdict,
};
use crate::view::{diff_views, table_view, TableView, ViewDelta};
use crate::wire::PluginView;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;
use std::fmt;

pub const ENGINE_GAS_BUDGET: u64 = 10_000_000_000;
pub const PLUGIN_GAS_BUDGET: u64 = 100_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFault(pub String);

impl fmt::Display for EngineFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "engine fault: {}", self.0)
    }
}

impl std::error::Error for EngineFault {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoldMode {
    Sequenced,
    Admission,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldOutcome {
    pub result: Result<(), FoldError>,
    pub deltas: Vec<ViewDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldLogOutcome {
    pub applied: u64,
    pub deltas: Vec<ViewDelta>,
}

pub trait Engine: Send + Sync {
    fn fold_entry(
        &mut self,
        entry: &LogEntry,
        verdict: Option<Verdict>,
        mode: FoldMode,
        viewer: u8,
    ) -> Result<FoldOutcome, EngineFault>;
    fn fold_log(&mut self, entries: &[LogEntry], viewer: u8)
        -> Result<FoldLogOutcome, EngineFault>;
    fn decide_request(&mut self, entry: &LogEntry) -> Result<Option<Vec<u8>>, EngineFault>;
    fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault>;
    fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault>;
    fn view(&mut self, viewer: u8) -> Result<TableView, EngineFault>;
    fn engine_hash(&self) -> Option<[u8; 32]>;
}

#[derive(Debug, Default)]
pub struct NativeEngine {
    state: LogState,
    views: BTreeMap<u8, TableView>,
}

impl NativeEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(&mut self, viewer: u8) -> Vec<ViewDelta> {
        let next = table_view(&self.state, viewer);
        let prev = self.views.entry(viewer).or_default();
        let deltas = diff_views(prev, &next);
        *prev = next;
        deltas
    }
}

pub fn native_fold_entry(
    state: &mut LogState,
    entry: &LogEntry,
    verdict: Option<Verdict>,
    mode: FoldMode,
) -> Result<(), FoldError> {
    match mode {
        FoldMode::Admission => {
            validate(state, entry)?;
            if let Some(verdict) = &verdict {
                if !verdict.accept {
                    return Err(verdict.refusal());
                }
            }
            fold_begin(state, entry)?;
            fold_finish(state, entry, verdict.unwrap_or_else(Verdict::accept))
        }
        FoldMode::Sequenced => {
            fold_begin(state, entry)?;
            fold_finish(state, entry, verdict.unwrap_or_else(Verdict::accept))
        }
    }
}

pub fn native_decide_request(state: &LogState, entry: &LogEntry) -> Option<DecideRequest> {
    validate(state, entry).ok()?;
    let mut preview = state.clone();
    preview.next_seq += 1;
    let plugin_state = std::mem::take(&mut preview.plugin_state);
    Some(DecideRequest {
        plugin_state: ByteBuf::from(plugin_state.into_vec()),
        state: preview,
        entry: entry.clone(),
    })
}

impl Engine for NativeEngine {
    fn fold_entry(
        &mut self,
        entry: &LogEntry,
        verdict: Option<Verdict>,
        mode: FoldMode,
        viewer: u8,
    ) -> Result<FoldOutcome, EngineFault> {
        let result = native_fold_entry(&mut self.state, entry, verdict, mode);
        let deltas = if mode == FoldMode::Admission && result.is_err() {
            Vec::new()
        } else {
            self.emit(viewer)
        };
        Ok(FoldOutcome { result, deltas })
    }

    fn fold_log(
        &mut self,
        entries: &[LogEntry],
        viewer: u8,
    ) -> Result<FoldLogOutcome, EngineFault> {
        let mut applied = 0;
        for entry in entries {
            if fold_entry(&mut self.state, entry).is_ok() {
                applied += 1;
            }
        }
        let deltas = self.emit(viewer);
        Ok(FoldLogOutcome { applied, deltas })
    }

    fn decide_request(&mut self, entry: &LogEntry) -> Result<Option<Vec<u8>>, EngineFault> {
        Ok(native_decide_request(&self.state, entry).map(|request| encode(&request)))
    }

    fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault> {
        Ok(encode_state(&self.state))
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault> {
        let state =
            decode_state(bytes).ok_or_else(|| EngineFault("snapshot does not decode".into()))?;
        self.state = state;
        self.views.clear();
        Ok(())
    }

    fn view(&mut self, viewer: u8) -> Result<TableView, EngineFault> {
        Ok(table_view(&self.state, viewer))
    }

    fn engine_hash(&self) -> Option<[u8; 32]> {
        None
    }
}

pub trait PluginModule: Send + Sync {
    fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault>;

    fn view(&mut self, _request: &[u8]) -> Result<PluginView, EngineFault> {
        Ok(PluginView::default())
    }

    fn module_hash(&self) -> Option<[u8; 32]> {
        None
    }
}

fn mediate<E: Engine + ?Sized, P: PluginModule + ?Sized>(
    engine: &mut E,
    plugin: Option<&mut P>,
    entry: &LogEntry,
) -> Result<Option<Verdict>, EngineFault> {
    match plugin {
        None => Ok(None),
        Some(plugin) => match engine.decide_request(entry)? {
            None => Ok(None),
            Some(request) => Ok(Some(plugin.decide(&request)?)),
        },
    }
}

pub fn fold_mediated<E: Engine + ?Sized, P: PluginModule + ?Sized>(
    engine: &mut E,
    plugin: Option<&mut P>,
    entry: &LogEntry,
    mode: FoldMode,
    viewer: u8,
) -> Result<FoldOutcome, EngineFault> {
    let verdict = mediate(engine, plugin, entry)?;
    engine.fold_entry(entry, verdict, mode, viewer)
}

pub fn fold_shadowed<E: Engine + ?Sized, P: PluginModule + ?Sized>(
    engine: &mut E,
    plugin: Option<&mut P>,
    shadow: &mut LogState,
    entry: &LogEntry,
    mode: FoldMode,
    viewer: u8,
) -> Result<FoldOutcome, EngineFault> {
    let verdict = mediate(engine, plugin, entry)?;
    let outcome = engine.fold_entry(entry, verdict.clone(), mode, viewer)?;
    let native = native_fold_entry(shadow, entry, verdict, mode);
    if !same_fold(&native, &outcome.result) {
        return Err(EngineFault(format!(
            "engine folded seq {} as {:?} but this build folds it as {:?}",
            entry.seq, outcome.result, native
        )));
    }
    Ok(outcome)
}

fn same_fold(native: &Result<(), FoldError>, engine: &Result<(), FoldError>) -> bool {
    match (native, engine) {
        (Err(FoldError::Rejected { .. }), Err(FoldError::Rejected { .. })) => true,
        _ => native == engine,
    }
}

pub fn fold_log_shadowed<E: Engine + ?Sized>(
    engine: &mut E,
    shadow: &mut LogState,
    entries: &[LogEntry],
    viewer: u8,
) -> Result<FoldLogOutcome, EngineFault> {
    let outcome = engine.fold_log(entries, viewer)?;
    let applied = entries
        .iter()
        .filter(|entry| fold_entry(shadow, entry).is_ok())
        .count() as u64;
    if applied != outcome.applied {
        return Err(EngineFault(format!(
            "engine applied {} of {} entries but this build applies {applied}",
            outcome.applied,
            entries.len()
        )));
    }
    Ok(outcome)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallFault {
    Trapped(String),
    GasExhausted,
    Broken(String),
}

impl CallFault {
    pub fn broken(context: &str, error: impl fmt::Display) -> Self {
        Self::Broken(format!("{context}: {error}"))
    }
}

impl fmt::Display for CallFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trapped(trap) => write!(f, "module trapped: {trap}"),
            Self::GasExhausted => f.write_str("module gas budget exhausted"),
            Self::Broken(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for CallFault {}

impl From<CallFault> for EngineFault {
    fn from(fault: CallFault) -> Self {
        EngineFault(match fault {
            CallFault::Trapped(trap) => format!("engine trapped: {trap}"),
            CallFault::GasExhausted => "engine gas budget exhausted".into(),
            CallFault::Broken(reason) => reason,
        })
    }
}

pub trait ModuleCall: Send + Sync {
    fn call(&mut self, name: &str, request: &[u8]) -> Result<Vec<u8>, CallFault>;
    fn abi_version(&mut self) -> Result<u32, CallFault>;
}

pub struct AbiEngine<M> {
    module: M,
    hash: [u8; 32],
}

impl<M: ModuleCall> AbiEngine<M> {
    pub fn load(mut module: M, hash: [u8; 32]) -> Result<Self, EngineFault> {
        let version = module.abi_version()?;
        if version != ENGINE_ABI_VERSION {
            return Err(EngineFault(format!(
                "engine abi version {version}, host speaks {ENGINE_ABI_VERSION}"
            )));
        }
        Ok(Self { module, hash })
    }

    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    fn exchange<T: serde::de::DeserializeOwned>(
        &mut self,
        name: &str,
        request: &[u8],
    ) -> Result<T, EngineFault> {
        let reply = self.module.call(name, request)?;
        let reply: Result<T, String> =
            decode(&reply).ok_or_else(|| EngineFault(format!("{name} reply does not decode")))?;
        reply.map_err(EngineFault)
    }
}

impl<M: ModuleCall> Engine for AbiEngine<M> {
    fn fold_entry(
        &mut self,
        entry: &LogEntry,
        verdict: Option<Verdict>,
        mode: FoldMode,
        viewer: u8,
    ) -> Result<FoldOutcome, EngineFault> {
        let request = encode(&FoldRequest {
            entry: entry.clone(),
            verdict,
            mode,
            viewer,
        });
        self.exchange("fold_entry", &request)
    }

    fn fold_log(
        &mut self,
        entries: &[LogEntry],
        viewer: u8,
    ) -> Result<FoldLogOutcome, EngineFault> {
        let request = encode(&FoldLogRequest {
            entries: entries.to_vec(),
            viewer,
        });
        self.exchange("fold_log", &request)
    }

    fn decide_request(&mut self, entry: &LogEntry) -> Result<Option<Vec<u8>>, EngineFault> {
        let reply: Option<ByteBuf> = self.exchange("decide_request", &encode(entry))?;
        Ok(reply.map(ByteBuf::into_vec))
    }

    fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault> {
        let reply: ByteBuf = self.exchange("snapshot", &[])?;
        Ok(reply.into_vec())
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault> {
        self.exchange("restore", &encode(&ByteBuf::from(bytes.to_vec())))
    }

    fn view(&mut self, viewer: u8) -> Result<TableView, EngineFault> {
        self.exchange("view", &encode(&ViewRequest { viewer }))
    }

    fn engine_hash(&self) -> Option<[u8; 32]> {
        Some(self.hash)
    }
}

pub struct AbiPlugin<M> {
    module: M,
    hash: [u8; 32],
}

impl<M: ModuleCall> AbiPlugin<M> {
    pub fn load(mut module: M, hash: [u8; 32]) -> Result<Self, EngineFault> {
        let version = module.abi_version()?;
        if version > crate::abi::PLUGIN_ABI_VERSION {
            return Err(EngineFault(format!(
                "plugin abi version {version}, host supports up to {}",
                crate::abi::PLUGIN_ABI_VERSION
            )));
        }
        Ok(Self { module, hash })
    }

    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    pub fn manifest_bytes(&mut self) -> Result<Vec<u8>, EngineFault> {
        Ok(self.module.call("manifest", &[])?)
    }
}

impl<M: ModuleCall> PluginModule for AbiPlugin<M> {
    fn module_hash(&self) -> Option<[u8; 32]> {
        Some(self.hash)
    }

    fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault> {
        match self.module.call("decide", request) {
            Ok(reply) => Ok(decode(&reply).unwrap_or_else(Verdict::reject)),
            Err(CallFault::GasExhausted) | Err(CallFault::Trapped(_)) => Ok(Verdict::reject()),
            Err(CallFault::Broken(reason)) => Err(EngineFault(reason)),
        }
    }

    fn view(&mut self, request: &[u8]) -> Result<PluginView, EngineFault> {
        match self.module.call("view", request) {
            Ok(reply) => Ok(decode(&reply).unwrap_or_default()),
            Err(CallFault::GasExhausted) | Err(CallFault::Trapped(_)) => Ok(PluginView::default()),
            Err(CallFault::Broken(reason)) => Err(EngineFault(reason)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{fold_entry_with, Decider, LogAction};
    use agni_core::Zone;

    fn scripted_entries() -> Vec<LogEntry> {
        vec![
            LogEntry::new(0, 0, LogAction::genesis("rae")),
            LogEntry::new(1, 1, LogAction::Join { name: "ada".into() }),
            LogEntry::new(
                2,
                0,
                LogAction::Deal {
                    cards: vec![0, 1, 2],
                    to: Zone::Hand,
                },
            ),
            LogEntry::new(
                3,
                0,
                LogAction::Game {
                    data: ByteBuf::from(vec![7]),
                },
            ),
            LogEntry::new(
                4,
                0,
                LogAction::Move {
                    card: 1,
                    to: Zone::Board,
                    seat: 0,
                    index: 0,
                    hidden: false,
                },
            ),
        ]
    }

    #[test]
    fn the_native_engine_folds_like_the_bare_fold() {
        let mut engine = NativeEngine::new();
        let mut bare = LogState::new();
        for entry in &scripted_entries() {
            let outcome = engine
                .fold_entry(entry, None, FoldMode::Sequenced, 0)
                .unwrap();
            assert_eq!(outcome.result, fold_entry(&mut bare, entry));
        }
        assert_eq!(engine.snapshot().unwrap(), encode_state(&bare));
    }

    #[test]
    fn admission_mode_refuses_without_consuming_the_slot() {
        let mut engine = NativeEngine::new();
        engine
            .fold_entry(&scripted_entries()[0], None, FoldMode::Admission, 0)
            .unwrap();
        let foreign = LogEntry::new(1, 5, LogAction::Reset);
        let hosted = LogEntry::new(1, 0, LogAction::Reset);
        let refused = engine
            .fold_entry(&foreign, None, FoldMode::Admission, 0)
            .unwrap();
        assert!(refused.result.is_err());
        assert!(refused.deltas.is_empty());
        let admitted = engine
            .fold_entry(&hosted, None, FoldMode::Admission, 0)
            .unwrap();
        assert_eq!(admitted.result, Ok(()));
        assert_eq!(engine.view(0).unwrap().next_seq, 2);
    }

    #[test]
    fn a_mediated_reject_matches_the_native_decider() {
        struct RejectGames;
        impl Decider for RejectGames {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, entry: &LogEntry) -> Verdict {
                Verdict {
                    accept: !matches!(entry.action, LogAction::Game { .. }),
                    plugin_state: None,
                    effects: Vec::new(),
                    reason: None,
                }
            }
        }
        struct RejectGamesModule;
        impl PluginModule for RejectGamesModule {
            fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault> {
                let request: DecideRequest =
                    decode(request).ok_or_else(|| EngineFault("bad request".into()))?;
                Ok(RejectGames.decide(&request.plugin_state, &request.state, &request.entry))
            }
        }
        let mut engine = NativeEngine::new();
        let mut module = RejectGamesModule;
        let mut bare = LogState::new();
        for entry in &scripted_entries() {
            let outcome = fold_mediated(
                &mut engine,
                Some(&mut module),
                entry,
                FoldMode::Sequenced,
                0,
            )
            .unwrap();
            assert_eq!(
                outcome.result,
                fold_entry_with(&mut bare, entry, &mut RejectGames)
            );
        }
        assert_eq!(engine.snapshot().unwrap(), encode_state(&bare));
    }

    #[test]
    fn a_restored_snapshot_resumes_the_fold() {
        let entries = scripted_entries();
        let mut engine = NativeEngine::new();
        for entry in &entries[..3] {
            engine
                .fold_entry(entry, None, FoldMode::Sequenced, 0)
                .unwrap();
        }
        let snapshot = engine.snapshot().unwrap();
        let mut resumed = NativeEngine::new();
        resumed.restore(&snapshot).unwrap();
        for entry in &entries[3..] {
            let a = engine
                .fold_entry(entry, None, FoldMode::Sequenced, 0)
                .unwrap();
            let b = resumed
                .fold_entry(entry, None, FoldMode::Sequenced, 0)
                .unwrap();
            assert_eq!(a.result, b.result);
        }
        assert_eq!(engine.snapshot().unwrap(), resumed.snapshot().unwrap());
    }

    struct Loopback {
        engine: NativeEngine,
        version: u32,
    }

    impl ModuleCall for Loopback {
        fn call(&mut self, name: &str, request: &[u8]) -> Result<Vec<u8>, CallFault> {
            Ok(match name {
                "fold_entry" => {
                    let request: FoldRequest = decode(request).unwrap();
                    encode(
                        &self
                            .engine
                            .fold_entry(
                                &request.entry,
                                request.verdict,
                                request.mode,
                                request.viewer,
                            )
                            .map_err(|fault| fault.0),
                    )
                }
                "snapshot" => encode(
                    &self
                        .engine
                        .snapshot()
                        .map(ByteBuf::from)
                        .map_err(|fault| fault.0),
                ),
                "view" => {
                    let request: ViewRequest = decode(request).unwrap();
                    encode(&self.engine.view(request.viewer).map_err(|fault| fault.0))
                }
                other => return Err(CallFault::broken(other, "unknown export")),
            })
        }

        fn abi_version(&mut self) -> Result<u32, CallFault> {
            Ok(self.version)
        }
    }

    #[test]
    fn the_abi_engine_speaks_the_same_fold_as_the_native_engine_behind_it() {
        let loopback = Loopback {
            engine: NativeEngine::new(),
            version: ENGINE_ABI_VERSION,
        };
        let mut abi = AbiEngine::load(loopback, [7; 32]).unwrap();
        let mut native = NativeEngine::new();
        for entry in &scripted_entries() {
            let via_abi = abi.fold_entry(entry, None, FoldMode::Sequenced, 1).unwrap();
            let via_native = native
                .fold_entry(entry, None, FoldMode::Sequenced, 1)
                .unwrap();
            assert_eq!(via_abi, via_native);
        }
        assert_eq!(abi.snapshot().unwrap(), native.snapshot().unwrap());
        assert_eq!(abi.view(1).unwrap(), native.view(1).unwrap());
        assert_eq!(abi.engine_hash(), Some([7; 32]));
    }

    #[test]
    fn an_abi_version_mismatch_is_refused_at_load() {
        let loopback = Loopback {
            engine: NativeEngine::new(),
            version: ENGINE_ABI_VERSION + 1,
        };
        match AbiEngine::load(loopback, [0; 32]) {
            Ok(_) => panic!("a mismatched abi version must be refused"),
            Err(refused) => assert!(refused.0.contains("abi version")),
        }
    }

    #[test]
    fn plugins_accept_supported_abis_and_refuse_unknown_ones_at_load() {
        for version in 0..=crate::abi::PLUGIN_ABI_VERSION + 1 {
            let module = Loopback {
                engine: NativeEngine::new(),
                version,
            };
            assert_eq!(
                AbiPlugin::load(module, [0; 32]).is_ok(),
                version <= crate::abi::PLUGIN_ABI_VERSION
            );
        }
    }
}
