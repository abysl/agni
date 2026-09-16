use agni_core::{CardFace, PlayerId, Zone};
use agni_engine_host::{CallFault, ModuleCall, WasmModule, PLUGIN_GAS_BUDGET};
use agni_harden::{harden, HardenConfig, HardenedModule};
use agni_net::session::{HostSession, WireIntent, WireZone};
use agni_plugin_sdk::decide::TOP;
use agni_plugin_sdk::dice::{commitment, Outcome};
use agni_plugin_sdk::prompt::Pick;
use agni_riftbound::{
    counter_table, zone_table, COUNTER_POINTS, COUNTER_XP, ZONE_BASE, ZONE_BATTLEFIELD_FIRST,
    ZONE_CHAIN, ZONE_HAND, ZONE_LEGEND, ZONE_MAIN_DECK, ZONE_RUNE_DECK, ZONE_RUNE_POOL, ZONE_TRASH,
};
use agni_riftbound_turns::state::PromptWhy;
use agni_riftbound_turns::{GameBlob, Mode, Phase, TurnEvent};
use agni_sim::abi::decode;
use agni_sim::engine::{EngineFault, NativeEngine, PluginModule};
use agni_sim::log::{LogAction, TableConfig, Verdict};
use agni_sim::wire::{CounterTarget, PluginView};
use serde_bytes::ByteBuf;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

const TABLE_CARDS: usize = 150;
const CEILING_PERCENT: u64 = 25;
const UNITS_AT_BATTLEFIELDS: usize = 12;
const DEATHKNELLS: usize = 4;

fn built_module(env_var: &str, package: &str, artifact: &str) -> PathBuf {
    if let Ok(path) = std::env::var(env_var) {
        return PathBuf::from(path);
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("workspace root resolves");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&workspace)
        .args([
            "build",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status()
        .expect("cargo runs");
    assert!(status.success(), "building {package} for wasm32 failed");
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target"));
    target.join(format!("wasm32-unknown-unknown/release/{artifact}"))
}

fn plugin_module() -> &'static HardenedModule {
    static HARDENED: OnceLock<HardenedModule> = OnceLock::new();
    HARDENED.get_or_init(|| {
        let raw = std::fs::read(built_module(
            "AGNI_RIFTBOUND_WASM",
            "agni-riftbound-plugin",
            "riftbound_plugin.wasm",
        ))
        .expect("module reads");
        harden(&raw, &HardenConfig::default()).expect("module survives the pipeline")
    })
}

#[derive(Debug, Clone)]
struct Sample {
    call: &'static str,
    label: String,
    gas: u64,
    exhausted: bool,
    request: usize,
    reply: usize,
    blob: usize,
}

impl Sample {
    fn percent(&self) -> u64 {
        self.gas * 100 / PLUGIN_GAS_BUDGET
    }

    fn permille(&self) -> u64 {
        self.gas * 1000 / PLUGIN_GAS_BUDGET
    }
}

#[derive(Default)]
struct Meter {
    samples: Mutex<Vec<Sample>>,
    label: Mutex<String>,
}

impl Meter {
    fn set(&self, label: impl Into<String>) {
        *self.label.lock().unwrap() = label.into();
    }

    fn since(&self, mark: usize) -> Vec<Sample> {
        self.samples.lock().unwrap()[mark..].to_vec()
    }

    fn mark(&self) -> usize {
        self.samples.lock().unwrap().len()
    }
}

struct Metered {
    module: WasmModule,
    hash: [u8; 32],
    meter: Arc<Meter>,
}

impl Metered {
    fn load(meter: Arc<Meter>) -> Self {
        let hardened = plugin_module();
        assert_eq!(
            hardened.report.gas_limit, PLUGIN_GAS_BUDGET,
            "the baked limit and the per-call budget agree"
        );
        let mut module =
            WasmModule::instantiate(&hardened.bytes, PLUGIN_GAS_BUDGET).expect("instantiates");
        module.abi_version().expect("abi_version answers");
        Self {
            module,
            hash: hardened.hash,
            meter,
        }
    }

    fn measured(
        &mut self,
        call: &'static str,
        request: &[u8],
        blob_of: fn(&[u8]) -> usize,
    ) -> Result<Vec<u8>, CallFault> {
        let result = self.module.call(call, request);
        let left = self.module.gas_left().unwrap_or(0);
        let exhausted = left < 0 || matches!(result, Err(CallFault::GasExhausted));
        let gas = if exhausted {
            PLUGIN_GAS_BUDGET
        } else {
            PLUGIN_GAS_BUDGET.saturating_sub(left as u64)
        };
        let (reply, blob) = match &result {
            Ok(reply) => (reply.len(), blob_of(reply)),
            Err(_) => (0, 0),
        };
        let mut samples = self.meter.samples.lock().unwrap();
        if let Ok(dir) = std::env::var("AGNI_GAS_DUMP_DIR") {
            let path = PathBuf::from(dir).join(format!("{:03}-{call}.cbor", samples.len()));
            std::fs::write(path, request).expect("the request dumps");
        }
        samples.push(Sample {
            call,
            label: self.meter.label.lock().unwrap().clone(),
            gas,
            exhausted,
            request: request.len(),
            reply,
            blob,
        });
        result
    }
}

fn verdict_blob(reply: &[u8]) -> usize {
    decode::<Verdict>(reply)
        .and_then(|verdict| verdict.plugin_state)
        .map(|blob| blob.len())
        .unwrap_or(0)
}

fn no_blob(_: &[u8]) -> usize {
    0
}

impl PluginModule for Metered {
    fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault> {
        match self.measured("decide", request, verdict_blob) {
            Ok(reply) => Ok(decode(&reply).unwrap_or_else(Verdict::reject)),
            Err(CallFault::GasExhausted) | Err(CallFault::Trapped(_)) => Ok(Verdict::reject()),
            Err(CallFault::Broken(reason)) => Err(EngineFault(reason)),
        }
    }

    fn view(&mut self, request: &[u8]) -> Result<PluginView, EngineFault> {
        match self.measured("view", request, no_blob) {
            Ok(reply) => Ok(decode(&reply).unwrap_or_default()),
            Err(CallFault::GasExhausted) | Err(CallFault::Trapped(_)) => Ok(PluginView::default()),
            Err(CallFault::Broken(reason)) => Err(EngineFault(reason)),
        }
    }

    fn module_hash(&self) -> Option<[u8; 32]> {
        Some(self.hash)
    }
}

fn unit(name: &str, energy: u8, power: u8, domain: &str, might: u8) -> CardFace {
    CardFace::named(name)
        .with_kind("Unit")
        .with_cost(Some(energy), Some(power))
        .with_domain(vec![domain.to_string()])
        .with_might(Some(might))
}

fn spell(name: &str, energy: u8, power: u8, domain: &str) -> CardFace {
    CardFace::named(name)
        .with_kind("Spell")
        .with_cost(Some(energy), Some(power))
        .with_domain(vec![domain.to_string()])
}

fn rune(domain: &str) -> CardFace {
    CardFace::named(format!("{domain} Rune"))
        .with_kind("Rune")
        .with_domain(vec![domain.to_string()])
}

fn battlefield(name: &str) -> CardFace {
    CardFace::named(name).with_kind("Battlefield")
}

fn open_table(meter: Arc<Meter>) -> (HostSession, u8) {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            zones: zone_table(),
            counters: counter_table(),
            ..TableConfig::default()
        },
        Box::new(NativeEngine::new()),
        Some(Box::new(Metered::load(meter))),
    )
    .unwrap();
    let (ada, _) = host.join("ada").unwrap();
    (host, ada)
}

fn act(host: &mut HostSession, seat: u8, event: TurnEvent) {
    host.intent(
        seat,
        WireIntent::Game {
            data: ByteBuf::from(event.encode()),
        },
    )
    .unwrap_or_else(|error| panic!("seat {seat} {event:?} is accepted: {error}"));
}

fn moved(host: &mut HostSession, seat: u8, card: u32, to: u16, to_seat: u8) {
    host.intent(
        seat,
        WireIntent::Move {
            card,
            to: WireZone::Plugin(to),
            seat: to_seat,
            index: TOP,
        },
    )
    .unwrap_or_else(|error| panic!("{card} -> zone {to} is legal: {error}"));
}

fn deal(host: &mut HostSession, seat: u8, faces: Vec<CardFace>, zone: u16) -> Vec<u32> {
    let (entry, _) = host
        .deal_to(seat, faces, WireZone::Plugin(zone))
        .expect("the deal is accepted before the start");
    let LogAction::Deal { cards, .. } = &entry.action else {
        panic!("a deal entry");
    };
    cards.clone()
}

fn blob(host: &HostSession) -> GameBlob {
    GameBlob::decode(&host.view().plugin_state).expect("the host view carries a blob")
}

fn cards_in(host: &HostSession, seat: u8, zone: u16) -> Vec<u32> {
    host.state()
        .table
        .in_area(PlayerId(seat), Zone::Plugin(zone))
        .map(|card| card.id.0)
        .collect()
}

fn kind_of(host: &HostSession, card: u32) -> String {
    host.face_of(card)
        .and_then(|(_, face)| face.kind)
        .unwrap_or_default()
}

fn name_of(host: &HostSession, card: u32) -> String {
    host.face_of(card)
        .map(|(_, face)| face.name)
        .unwrap_or_default()
}

fn points(host: &HostSession, seat: u8) -> i32 {
    host.state()
        .counter(CounterTarget::Seat(seat), COUNTER_POINTS)
        .unwrap_or(0)
}

fn table_size(host: &HostSession) -> usize {
    host.state().table.cards().len()
}

fn labels(view: &PluginView) -> Vec<String> {
    view.affordances
        .iter()
        .map(|affordance| affordance.label.clone())
        .collect()
}

fn pick(host: &mut HostSession, seat: u8, option: u16) {
    let prompt = blob(host).prompt.expect("a prompt is open").id;
    act(host, seat, TurnEvent::Pick(Pick { prompt, option }));
}

fn roll_for_first(host: &mut HostSession, ada: u8) -> u8 {
    let mut secrets = [[1u8; 8], [2u8; 8]];
    for attempt in 0..32u8 {
        secrets[0][0] = attempt;
        act(
            host,
            0,
            TurnEvent::CommitRoll {
                commit: commitment(&secrets[0]),
            },
        );
        act(
            host,
            ada,
            TurnEvent::CommitRoll {
                commit: commitment(&secrets[1]),
            },
        );
        act(host, 0, TurnEvent::RevealRoll { secret: secrets[0] });
        act(host, ada, TurnEvent::RevealRoll { secret: secrets[1] });
        let lobby = blob(host);
        if let Outcome::Winner(seat) = lobby.roll().expect("in the lobby").outcome() {
            return seat;
        }
    }
    panic!("a d6 decides within a few rounds")
}

fn start_enforced(host: &mut HostSession, ada: u8, winner: u8) {
    act(
        host,
        winner,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(host, winner, TurnEvent::StartGame { first_player: 0 });
    for seat in [0, ada] {
        let keep = cards_in(host, seat, ZONE_HAND).len() as u16;
        pick(host, seat, keep);
    }
    let started = blob(host);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
}

fn answer(host: &mut HostSession, meter: &Meter, prefix: &str) -> bool {
    let held = blob(host);
    let Some(prompt) = held.prompt else {
        return false;
    };
    let seat = prompt.seat;
    meter.set(format!("{prefix} · view for the prompt ({:?})", held.why));
    let offered = labels(&host.plugin_view(seat));
    let closing = |word: &str| {
        offered
            .iter()
            .position(|label| label == word)
            .map(|index| index as u16)
    };
    let option = match held.why {
        Some(PromptWhy::GroupMove { .. }) => closing("done").unwrap_or(0),
        Some(PromptWhy::Mulligan) => closing("keep").unwrap_or(0),
        _ => 0,
    };
    meter.set(format!(
        "{prefix} · {{seat {seat}}} answers {:?} with {}",
        held.why,
        offered.get(option as usize).cloned().unwrap_or_default()
    ));
    pick(host, seat, option);
    true
}

fn settle(host: &mut HostSession, meter: &Meter, prefix: &str) {
    for _ in 0..128 {
        if answer(host, meter, prefix) {
            continue;
        }
        let held = blob(host);
        let seat = match (held.priority, held.showdown.as_ref()) {
            (Some(priority), _) => priority.active,
            (None, Some(showdown)) => showdown.focus(),
            (None, None) => return,
        };
        meter.set(format!(
            "{prefix} · {{seat {seat}}} passes (chain {}, queue {})",
            held.chain.len(),
            held.queue.len()
        ));
        act(host, seat, TurnEvent::Pass);
    }
    panic!("{prefix}: the table never settles");
}

fn view_both(
    host: &mut HostSession,
    meter: &Meter,
    ada: u8,
    label: &str,
) -> (PluginView, PluginView) {
    meter.set(format!("{label} · seat 0"));
    let mine = host.plugin_view(0);
    meter.set(format!("{label} · seat {ada}"));
    let theirs = host.plugin_view(ada);
    for view in [&mine, &theirs] {
        assert!(
            !view.status.is_empty(),
            "{label}: a blank status means the presenter faulted or ran out of gas"
        );
    }
    (mine, theirs)
}

fn print_samples(title: &str, samples: &[Sample]) {
    println!();
    println!("{title}");
    println!(
        "  {:<6} {:>12} {:>7} {:>9} {:>9} {:>8}  label",
        "call", "gas", "budget", "request", "reply", "blob"
    );
    for sample in samples {
        println!(
            "  {:<6} {:>12} {:>6}.{}% {:>8}B {:>8}B {:>7}B  {}{}",
            sample.call,
            sample.gas,
            sample.permille() / 10,
            sample.permille() % 10,
            sample.request,
            sample.reply,
            sample.blob,
            sample.label,
            if sample.exhausted { "  EXHAUSTED" } else { "" }
        );
    }
}

fn worst<'a>(samples: &'a [Sample], call: &str) -> Option<&'a Sample> {
    samples
        .iter()
        .filter(|sample| sample.call == call)
        .max_by_key(|sample| sample.gas)
}

fn summarise(title: &str, samples: &[Sample]) -> (u64, u64) {
    let decides: Vec<&Sample> = samples.iter().filter(|s| s.call == "decide").collect();
    let views: Vec<&Sample> = samples.iter().filter(|s| s.call == "view").collect();
    let mean = |set: &[&Sample]| {
        if set.is_empty() {
            0
        } else {
            set.iter().map(|s| s.gas).sum::<u64>() / set.len() as u64
        }
    };
    let worst_decide = worst(samples, "decide");
    let worst_view = worst(samples, "view");
    println!();
    println!("{title} · summary against a {PLUGIN_GAS_BUDGET} gas budget");
    println!(
        "  decide × {:<4} mean {:>12} ({}.{}%)",
        decides.len(),
        mean(&decides),
        mean(&decides) * 1000 / PLUGIN_GAS_BUDGET / 10,
        mean(&decides) * 1000 / PLUGIN_GAS_BUDGET % 10
    );
    if let Some(sample) = worst_decide {
        println!(
            "  worst decide {:>12} ({}.{}%) · {}",
            sample.gas,
            sample.permille() / 10,
            sample.permille() % 10,
            sample.label
        );
    }
    println!(
        "  view   × {:<4} mean {:>12} ({}.{}%)",
        views.len(),
        mean(&views),
        mean(&views) * 1000 / PLUGIN_GAS_BUDGET / 10,
        mean(&views) * 1000 / PLUGIN_GAS_BUDGET % 10
    );
    if let Some(sample) = worst_view {
        println!(
            "  worst view   {:>12} ({}.{}%) · {}",
            sample.gas,
            sample.permille() / 10,
            sample.permille() % 10,
            sample.label
        );
    }
    let blobs: Vec<usize> = decides
        .iter()
        .filter(|s| s.blob > 0)
        .map(|s| s.blob)
        .collect();
    if !blobs.is_empty() {
        println!(
            "  blob bytes   min {} · mean {} · max {} over {} accepted entries",
            blobs.iter().min().unwrap(),
            blobs.iter().sum::<usize>() / blobs.len(),
            blobs.iter().max().unwrap(),
            blobs.len()
        );
    }
    let requests: Vec<usize> = samples.iter().map(|s| s.request).collect();
    println!(
        "  request bytes min {} · max {}",
        requests.iter().min().copied().unwrap_or(0),
        requests.iter().max().copied().unwrap_or(0)
    );
    (
        worst_decide.map(|s| s.percent()).unwrap_or(0),
        worst_view.map(|s| s.percent()).unwrap_or(0),
    )
}

fn deck(prefix: &str, domain: &str, n: usize) -> Vec<CardFace> {
    (0..n)
        .map(|index| match index % 5 {
            0 => unit(&format!("{prefix} Vanguard {index}"), 2, 1, domain, 3),
            1 => spell(&format!("{prefix} Bolt {index}"), 1, 0, domain),
            2 => unit(&format!("{prefix} Warden {index}"), 2, 0, domain, 2),
            _ => unit(&format!("{prefix} Recruit {index}"), 1, 0, domain, 1),
        })
        .collect()
}

fn pad_to_table_size(host: &mut HostSession, ada: u8) {
    let missing = TABLE_CARDS.saturating_sub(table_size(host));
    if missing > 0 {
        deal(host, ada, deck("Reserve", "Calm", missing), ZONE_MAIN_DECK);
    }
    assert_eq!(
        table_size(host),
        TABLE_CARDS,
        "the benchmark table is exactly {TABLE_CARDS} cards"
    );
}

fn worst_fixture(meter: &Arc<Meter>) -> (HostSession, u8, u32) {
    let (mut host, ada) = open_table(meter.clone());
    meter.set("lobby");
    let winner = roll_for_first(&mut host, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    deal(&mut host, 0, deck("Mind", "Mind", 36), ZONE_MAIN_DECK);
    deal(&mut host, ada, deck("Calm", "Calm", 36), ZONE_MAIN_DECK);
    deal(&mut host, 0, vec![rune("Mind"); 12], ZONE_RUNE_DECK);
    deal(&mut host, ada, vec![rune("Calm"); 12], ZONE_RUNE_DECK);
    deal(&mut host, 0, vec![rune("Mind"); 7], ZONE_RUNE_POOL);
    deal(&mut host, 0, vec![battlefield("Crossroads")], bf1);
    deal(&mut host, ada, vec![battlefield("Sanctum")], bf2);
    let power = deal(
        &mut host,
        0,
        vec![
            spell("Unchecked Power", 7, 2, "Mind"),
            unit("Mind Keeper", 2, 0, "Mind", 2),
            spell("Mind Bolt", 1, 0, "Mind"),
            unit("Mind Seer", 3, 1, "Mind", 3),
        ],
        ZONE_HAND,
    )[0];
    let mut mine = vec![unit("Unsung Hero", 2, 0, "Mind", 5); DEATHKNELLS / 2];
    mine.extend(
        (0..UNITS_AT_BATTLEFIELDS / 2 - DEATHKNELLS / 2)
            .map(|index| unit(&format!("Guard {index}"), 2, 0, "Mind", 2)),
    );
    let mut theirs = vec![unit("Unsung Hero", 2, 0, "Calm", 5); DEATHKNELLS / 2];
    theirs.extend(
        (0..UNITS_AT_BATTLEFIELDS / 2 - DEATHKNELLS / 2)
            .map(|index| unit(&format!("Sentry {index}"), 2, 0, "Calm", 2)),
    );
    deal(&mut host, 0, mine, bf1);
    deal(&mut host, ada, theirs, bf2);
    deal(
        &mut host,
        0,
        (0..8)
            .map(|index| unit(&format!("Reservist {index}"), 1, 0, "Mind", 1))
            .collect(),
        ZONE_BASE,
    );
    deal(
        &mut host,
        ada,
        (0..8)
            .map(|index| unit(&format!("Militia {index}"), 1, 0, "Calm", 1))
            .collect(),
        ZONE_BASE,
    );
    pad_to_table_size(&mut host, ada);
    meter.set("start");
    start_enforced(&mut host, ada, winner);
    (host, ada, power)
}

fn units_at(host: &HostSession, zone: u16) -> usize {
    [0u8, 1u8]
        .into_iter()
        .flat_map(|seat| cards_in(host, seat, zone))
        .filter(|card| kind_of(host, *card) == "Unit")
        .count()
}

fn xp_of(host: &HostSession, seat: u8) -> i32 {
    host.state()
        .counter(CounterTarget::Seat(seat), COUNTER_XP)
        .unwrap_or(0)
}

fn legend(name: &str, domain: &str) -> CardFace {
    CardFace::named(name)
        .with_kind("Legend")
        .with_domain(vec![domain.to_string()])
}

fn m9_fixture(meter: &Arc<Meter>) -> (HostSession, u8, u32, u32, u16) {
    let (mut host, ada) = open_table(meter.clone());
    meter.set("lobby");
    let winner = roll_for_first(&mut host, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    deal(
        &mut host,
        0,
        vec![
            unit("Master Yi - Tempered", 4, 0, "Body", 4),
            unit("Master Yi - Tempered", 4, 0, "Body", 4),
            unit("Unsung Hero", 2, 0, "Mind", 5),
            unit("Guard A", 2, 0, "Mind", 2),
        ],
        bf1,
    );
    deal(
        &mut host,
        ada,
        vec![
            unit("Kha'Zix - Mutating Horror", 4, 1, "Chaos", 4),
            unit("Unsung Hero", 2, 0, "Calm", 5),
            unit("Sentry A", 2, 0, "Calm", 2),
        ],
        bf2,
    );
    deal(
        &mut host,
        0,
        vec![legend("Kha'Zix - Voidreaver", "Chaos")],
        ZONE_LEGEND,
    );
    deal(
        &mut host,
        ada,
        vec![legend("Master Yi - Wuju Bladesman", "Calm")],
        ZONE_LEGEND,
    );
    deal(&mut host, 0, deck("Mind", "Mind", 36), ZONE_MAIN_DECK);
    deal(&mut host, ada, deck("Calm", "Calm", 36), ZONE_MAIN_DECK);
    deal(&mut host, 0, vec![rune("Mind"); 12], ZONE_RUNE_DECK);
    deal(&mut host, ada, vec![rune("Calm"); 12], ZONE_RUNE_DECK);
    deal(&mut host, 0, vec![rune("Mind"); 7], ZONE_RUNE_POOL);
    deal(&mut host, ada, vec![rune("Calm"); 7], ZONE_RUNE_POOL);
    deal(&mut host, 0, vec![battlefield("Crossroads")], bf1);
    deal(&mut host, ada, vec![battlefield("Forbidding Waste")], bf2);
    let hand = deal(
        &mut host,
        0,
        vec![
            spell("Bellows Breath", 1, 1, "Mind"),
            unit("Mind Keeper", 2, 0, "Mind", 2),
            unit("Mind Seer", 3, 1, "Mind", 3),
        ],
        ZONE_HAND,
    );
    let base = deal(
        &mut host,
        0,
        vec![
            unit("Master Yi - Tempered", 4, 0, "Body", 4),
            unit("Scuttle Crab", 2, 0, "Calm", 0),
        ],
        ZONE_BASE,
    );
    deal(
        &mut host,
        ada,
        (0..6)
            .map(|index| unit(&format!("Militia {index}"), 1, 0, "Calm", 1))
            .collect(),
        ZONE_BASE,
    );
    pad_to_table_size(&mut host, ada);
    meter.set("start");
    start_enforced(&mut host, ada, winner);
    (host, ada, hand[0], base[0], bf2)
}

#[test]
#[ignore = "the gas benchmark: cargo test -p agni-net --test gas_bench --release -- --ignored --nocapture"]
fn a_kha_zix_mirror_with_hunts_auras_and_a_repeated_bellows_breath_stays_under_the_ceiling() {
    let meter = Arc::new(Meter::default());
    let (mut host, ada, breath, yi_home, bf2) = m9_fixture(&meter);
    assert_eq!(xp_of(&host, 0), 0, "an enforced game starts at 0 XP");
    let mark = meter.mark();
    view_both(
        &mut host,
        &meter,
        ada,
        "view · turn 1 open · two hunts on hold",
    );
    settle(&mut host, &meter, "turn 1 holds hunt");
    view_both(&mut host, &meter, ada, "view · turn 1 open");
    meter.set("Bellows Breath to the chain");
    moved(&mut host, 0, breath, ZONE_CHAIN, 0);
    settle(&mut host, &meter, "Bellows Breath repeated");
    view_both(&mut host, &meter, ada, "view · after Bellows Breath");
    let legend = cards_in(&host, 0, ZONE_LEGEND)[0];
    meter.set("Kha'Zix buff activation");
    act(
        &mut host,
        0,
        TurnEvent::Activate {
            source: legend,
            ability: 1,
        },
    );
    settle(&mut host, &meter, "Kha'Zix buff");
    meter.set("Yi marches into Forbidding Waste");
    moved(&mut host, 0, yi_home, bf2, 0);
    settle(&mut host, &meter, "showdown at the Waste");
    view_both(&mut host, &meter, ada, "view · after the combat");
    meter.set("turn 1 ends");
    act(&mut host, 0, TurnEvent::EndTurn);
    settle(&mut host, &meter, "turn 1 ends");
    view_both(&mut host, &meter, ada, "view · turn 2 opens");
    meter.set("turn 2 ends");
    act(&mut host, ada, TurnEvent::EndTurn);
    settle(&mut host, &meter, "turn 2 ends");
    view_both(&mut host, &meter, ada, "view · turn 3 opens · holds hunt");
    let samples = meter.since(mark);
    print_samples("M9 worst fixture", &samples);
    let (decide, view) = summarise("M9 worst fixture", &samples);
    assert!(
        !samples.iter().any(|sample| sample.exhausted),
        "no call ran out of gas"
    );
    assert!(decide < CEILING_PERCENT, "decide peaks at {decide}%");
    assert!(view < CEILING_PERCENT, "view peaks at {view}%");
}

#[test]
#[ignore = "the gas benchmark: cargo test -p agni-net --test gas_bench --release -- --ignored --nocapture"]
fn unchecked_power_with_twelve_units_and_four_deathknells_stays_under_a_quarter_of_the_budget() {
    let meter = Arc::new(Meter::default());
    let (mut host, ada, power) = worst_fixture(&meter);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    assert_eq!(
        units_at(&host, bf1) + units_at(&host, bf2),
        UNITS_AT_BATTLEFIELDS
    );
    let heroes = [0, ada]
        .into_iter()
        .flat_map(|seat| {
            cards_in(&host, seat, bf1)
                .into_iter()
                .chain(cards_in(&host, seat, bf2))
        })
        .filter(|card| name_of(&host, *card) == "Unsung Hero")
        .count();
    assert_eq!(heroes, DEATHKNELLS);
    let my_hand = cards_in(&host, 0, ZONE_HAND).len();
    let their_hand = cards_in(&host, ada, ZONE_HAND).len();
    let trash_before =
        cards_in(&host, 0, ZONE_TRASH).len() + cards_in(&host, ada, ZONE_TRASH).len();

    let mark = meter.mark();
    meter.set("Unchecked Power to the chain");
    moved(&mut host, 0, power, ZONE_CHAIN, 0);
    let staged = blob(&host);
    assert_eq!(staged.chain.len(), 1, "the spell waits on the chain");
    assert!(staged.prompt.is_none());
    view_both(
        &mut host,
        &meter,
        ada,
        "view · Unchecked Power on the chain",
    );
    settle(&mut host, &meter, "Unchecked Power");
    view_both(&mut host, &meter, ada, "view · after the sweep");

    let done = blob(&host);
    assert!(done.chain.is_empty() && done.queue.is_empty() && done.prompt.is_none());
    assert_eq!(
        units_at(&host, bf1) + units_at(&host, bf2),
        0,
        "twelve damage clears every unit at the battlefields"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_TRASH).len() + cards_in(&host, ada, ZONE_TRASH).len(),
        trash_before + UNITS_AT_BATTLEFIELDS + 1,
        "the twelve units and the spell are trashed"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        my_hand - 1 + 4,
        "two Mighty Deathknells draw two each for seat 0"
    );
    assert_eq!(
        cards_in(&host, ada, ZONE_HAND).len(),
        their_hand + 4,
        "two Mighty Deathknells draw two each for seat 1"
    );

    let samples = meter.since(mark);
    print_samples(
        &format!(
            "worst fixture · Unchecked Power resolving · {UNITS_AT_BATTLEFIELDS} units at battlefields · {DEATHKNELLS} Deathknells · {TABLE_CARDS}-card table"
        ),
        &samples,
    );
    let (decide, view) = summarise("worst fixture", &samples);
    assert!(
        !samples.iter().any(|sample| sample.exhausted),
        "no call ran out of gas"
    );
    assert!(
        decide < CEILING_PERCENT,
        "decide peaks at {decide}% of the budget, the ceiling is {CEILING_PERCENT}%"
    );
    assert!(
        view < CEILING_PERCENT,
        "view peaks at {view}% of the budget, the ceiling is {CEILING_PERCENT}%"
    );
}

fn hand_card_of_kind(host: &HostSession, seat: u8, kind: &str) -> Option<u32> {
    cards_in(host, seat, ZONE_HAND)
        .into_iter()
        .find(|card| kind_of(host, *card) == kind)
}

fn ready_unit_at_base(host: &HostSession, seat: u8) -> Option<u32> {
    cards_in(host, seat, ZONE_BASE)
        .into_iter()
        .filter(|card| kind_of(host, *card) == "Unit")
        .find(|card| host.state().annotation(*card, "exhausted") != Some(&[1u8][..]))
}

fn play_from_hand(host: &mut HostSession, meter: &Meter, seat: u8, kind: &str, turn: u16) -> bool {
    let Some(card) = hand_card_of_kind(host, seat, kind) else {
        return false;
    };
    let to = if kind == "Unit" {
        ZONE_BASE
    } else {
        ZONE_CHAIN
    };
    let label = format!(
        "turn {turn} · {{seat {seat}}} plays {}",
        name_of(host, card)
    );
    meter.set(&label);
    let intent = WireIntent::Move {
        card,
        to: WireZone::Plugin(to),
        seat,
        index: TOP,
    };
    if host.intent(seat, intent).is_err() {
        return false;
    }
    settle(host, meter, &label);
    true
}

fn march(host: &mut HostSession, meter: &Meter, seat: u8, to: u16, turn: u16) -> bool {
    let Some(unit) = ready_unit_at_base(host, seat) else {
        return false;
    };
    let label = format!(
        "turn {turn} · {{seat {seat}}} marches {} to zone {to}",
        name_of(host, unit)
    );
    meter.set(&label);
    let intent = WireIntent::Move {
        card: unit,
        to: WireZone::Plugin(to),
        seat,
        index: TOP,
    };
    if host.intent(seat, intent).is_err() {
        return false;
    }
    settle(host, meter, &label);
    true
}

fn plain_fixture(meter: &Arc<Meter>) -> (HostSession, u8) {
    let (mut host, ada) = open_table(meter.clone());
    meter.set("lobby");
    let winner = roll_for_first(&mut host, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    deal(&mut host, 0, deck("Fury", "Fury", 40), ZONE_MAIN_DECK);
    deal(&mut host, ada, deck("Calm", "Calm", 40), ZONE_MAIN_DECK);
    deal(&mut host, 0, vec![rune("Fury"); 12], ZONE_RUNE_DECK);
    deal(&mut host, ada, vec![rune("Calm"); 12], ZONE_RUNE_DECK);
    deal(&mut host, 0, vec![battlefield("Crossroads")], bf1);
    deal(&mut host, ada, vec![battlefield("Sanctum")], bf2);
    deal(
        &mut host,
        0,
        vec![
            unit("Vi", 3, 1, "Fury", 6),
            unit("Caitlyn", 2, 1, "Fury", 3),
            unit("Jayce", 2, 0, "Fury", 3),
            unit("Ekko", 1, 0, "Fury", 2),
        ],
        ZONE_BASE,
    );
    deal(
        &mut host,
        ada,
        vec![
            unit("Jinx", 3, 1, "Calm", 3),
            unit("Sett", 2, 0, "Calm", 2),
            unit("Yasuo", 2, 1, "Calm", 3),
        ],
        ZONE_BASE,
    );
    pad_to_table_size(&mut host, ada);
    meter.set("start");
    start_enforced(&mut host, ada, winner);
    (host, ada)
}

#[test]
#[ignore = "the gas benchmark: cargo test -p agni-net --test gas_bench --release -- --ignored --nocapture"]
fn a_plain_game_to_the_victory_score_reports_gas_and_blob_size_per_entry() {
    let meter = Arc::new(Meter::default());
    let (mut host, ada) = plain_fixture(&meter);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    let mark = meter.mark();
    let mut mid_game = None;
    let mut winner = None;
    for _ in 0..40 {
        let held = blob(&host);
        let turn = held.turn();
        let seat = held.turn_player();
        if turn == 3 {
            let (mine, theirs) = view_both(&mut host, &meter, ada, "mid-game view · turn 3");
            mid_game = Some((mine, theirs));
        }
        play_from_hand(&mut host, &meter, seat, "Unit", turn);
        play_from_hand(&mut host, &meter, seat, "Spell", turn);
        let target = if seat == 0 {
            if turn == 1 {
                bf1
            } else {
                bf2
            }
        } else {
            bf2
        };
        if seat == 0 || turn == 2 {
            march(&mut host, &meter, seat, target, turn);
        }
        meter.set(format!("turn {turn} · {{seat {seat}}} ends the turn"));
        act(&mut host, seat, TurnEvent::EndTurn);
        settle(
            &mut host,
            &meter,
            &format!("turn {turn} · after the hand-over"),
        );
        meter.set(format!("view · turn {} opens", turn + 1));
        let strip = host.plugin_view(0);
        if let Some(seat) = strip.winner {
            winner = Some(seat);
            break;
        }
    }
    assert_eq!(
        winner,
        Some(0),
        "seat 0 holds both battlefields to the victory score"
    );
    assert!(points(&host, 0) >= 8);
    let (mine, theirs) = mid_game.expect("turn 3 was reached");
    assert!(
        !mine.legal.is_empty(),
        "the turn player has legal plays on turn 3"
    );
    assert!(!theirs.status.is_empty());

    let samples = meter.since(mark);
    print_samples(
        &format!(
            "plain game · {TABLE_CARDS}-card table · every plugin call from turn 1 to the win"
        ),
        &samples,
    );
    let (decide, view) = summarise("plain game", &samples);
    let mid: Vec<Sample> = samples
        .iter()
        .filter(|sample| {
            sample.label.starts_with("mid-game view") || sample.label.starts_with("turn 3 ·")
        })
        .cloned()
        .collect();
    summarise("mid-game fixture · turn 3", &mid);
    assert!(!samples.iter().any(|sample| sample.exhausted));
    assert!(decide < CEILING_PERCENT, "decide peaks at {decide}%");
    assert!(view < CEILING_PERCENT, "view peaks at {view}%");
}
