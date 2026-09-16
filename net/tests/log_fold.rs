use agni_core::{CardFace, CardId, PlayerId, Zone};
use agni_net::session::{HostSession, WireFace, WireIntent, WireZone};
use agni_riftbound::{
    zone_table, ZONE_BASE, ZONE_BATTLEFIELD_FIRST, ZONE_HAND, ZONE_LEGEND, ZONE_MAIN_DECK,
    ZONE_RUNE_DECK,
};
use agni_sim::log::{
    decode_entry, encode_entry, encode_log, fold, fold_entry, fold_with, Decider, FoldError,
    LogAction, LogEntry, LogState, TableConfig, Verdict,
};
use agni_sim::wire::hidden_face;
use serde_bytes::ByteBuf;

fn faces(prefix: &str, n: usize) -> Vec<CardFace> {
    (0..n)
        .map(|i| CardFace::named(format!("{prefix} {i}")))
        .collect()
}

fn hand(state: &LogState, seat: u8) -> Vec<u32> {
    state
        .table
        .in_area(PlayerId(seat), Zone::Hand)
        .map(|card| card.id.0)
        .collect()
}

fn play(host: &mut HostSession, from: u8, card: u32, seat: u8, index: u32) {
    host.intent(
        from,
        WireIntent::Move {
            card,
            to: WireZone::Board,
            seat,
            index,
        },
    )
    .expect("scripted move is valid");
}

fn scripted() -> HostSession {
    let mut host = HostSession::new("rae");
    host.deal(0, faces("host", 3)).unwrap();
    let (ada, _) = host.join("ada").unwrap();
    host.deal(ada, faces("ada", 3)).unwrap();
    let host_hand = hand(host.state(), 0);
    play(&mut host, 0, host_hand[0], 0, 0);
    let ada_hand = hand(host.state(), ada);
    play(&mut host, ada, ada_hand[1], ada, 0);
    play(&mut host, ada, ada_hand[1], 0, 1);
    host.reset(vec![
        (0, faces("fresh-host", 2)),
        (ada, faces("fresh-ada", 2)),
    ])
    .unwrap();
    let fresh = hand(host.state(), ada);
    play(&mut host, ada, fresh[0], ada, 0);
    host
}

#[test]
fn the_same_log_folds_to_the_same_state_at_every_replica() {
    let host = scripted();
    let first = fold(host.log());
    let second = fold(host.log());
    assert_eq!(first, second);
    assert_eq!(first, *host.state());
}

#[test]
fn replaying_a_recorded_session_reproduces_the_final_table() {
    let host = scripted();
    let encoded = encode_log(host.log());
    let decoded: Vec<LogEntry> = ciborium::from_reader(encoded.as_slice()).unwrap();
    let replayed = fold(&decoded);
    assert_eq!(replayed.table, host.state().table);
    assert_eq!(replayed.seats, host.state().seats);
    assert_eq!(replayed.revealed, host.state().revealed);
}

#[test]
fn order_comes_only_from_seq_never_from_arrival() {
    let host = scripted();
    let shuffled: Vec<LogEntry> = host.log().iter().rev().cloned().collect();
    let mut replica = LogState::new();
    let mut settled = false;
    while !settled {
        settled = true;
        for entry in &shuffled {
            if entry.seq == replica.next_seq {
                assert_eq!(fold_entry(&mut replica, entry), Ok(()));
                settled = false;
            } else {
                assert_eq!(fold_entry(&mut replica, entry), Err(FoldError::BadSeq));
            }
        }
    }
    assert_eq!(replica, *host.state());
}

#[test]
fn entries_reencode_byte_identically() {
    let host = scripted();
    for entry in host.log() {
        let bytes = encode_entry(entry);
        let decoded = decode_entry(&bytes).unwrap();
        assert_eq!(&decoded, entry);
        assert_eq!(encode_entry(&decoded), bytes);
    }
}

fn burn(a: &mut LogState, b: &mut LogState, seat: u8, action: LogAction, expected: FoldError) {
    let entry = LogEntry::new(a.next_seq, seat, action);
    assert_eq!(fold_entry(a, &entry), Err(expected.clone()));
    assert_eq!(fold_entry(b, &entry), Err(expected));
    assert_eq!(a, b);
}

#[test]
fn invalid_entries_are_rejected_identically_at_every_replica() {
    let mut host = HostSession::new("rae");
    host.deal(0, faces("host", 3)).unwrap();
    let (ada, _) = host.join("ada").unwrap();
    host.deal(ada, faces("ada", 3)).unwrap();
    let host_hand = hand(host.state(), 0);
    play(&mut host, 0, host_hand[0], 0, 0);
    let mut a = fold(host.log());
    let mut b = fold(host.log());
    let before = a.table.clone();
    let ada_hand = hand(&a, ada);
    burn(
        &mut a,
        &mut b,
        0,
        LogAction::genesis("again"),
        FoldError::DuplicateGenesis,
    );
    burn(
        &mut a,
        &mut b,
        ada,
        LogAction::Move {
            card: host_hand[1],
            to: WireZone::Board,
            seat: ada,
            index: 0,
            hidden: false,
        },
        FoldError::ForeignHand,
    );
    burn(
        &mut a,
        &mut b,
        ada,
        LogAction::Move {
            card: ada_hand[0],
            to: WireZone::Hand,
            seat: 0,
            index: 0,
            hidden: false,
        },
        FoldError::ForeignHandTarget,
    );
    burn(
        &mut a,
        &mut b,
        0,
        LogAction::Move {
            card: 999,
            to: WireZone::Board,
            seat: 0,
            index: 0,
            hidden: false,
        },
        FoldError::UnknownCard,
    );
    burn(
        &mut a,
        &mut b,
        0,
        LogAction::Deal {
            cards: vec![host_hand[1]],
            to: WireZone::Hand,
        },
        FoldError::DuplicateCard,
    );
    burn(
        &mut a,
        &mut b,
        0,
        LogAction::Deal {
            cards: vec![500, 500],
            to: WireZone::Hand,
        },
        FoldError::DuplicateCard,
    );
    burn(
        &mut a,
        &mut b,
        7,
        LogAction::Deal {
            cards: vec![500],
            to: WireZone::Hand,
        },
        FoldError::UnknownSeat,
    );
    burn(
        &mut a,
        &mut b,
        0,
        LogAction::Reveal {
            card: host_hand[0],
            face: WireFace::from(&faces("forged", 1)[0]),
        },
        FoldError::FaceConflict,
    );
    burn(
        &mut a,
        &mut b,
        0,
        LogAction::Move {
            card: host_hand[1],
            to: WireZone::Hand,
            seat: 0,
            index: 0,
            hidden: false,
        },
        FoldError::NoOp,
    );
    burn(&mut a, &mut b, ada, LogAction::Reset, FoldError::NotTheHost);
    assert_eq!(a.table, before);
    let stale = LogEntry::new(a.next_seq + 5, 0, LogAction::Reset);
    let seq_before = a.next_seq;
    assert_eq!(fold_entry(&mut a, &stale), Err(FoldError::BadSeq));
    assert_eq!(a.next_seq, seq_before);
}

#[test]
fn a_reveal_is_idempotent_but_never_rewrites_a_public_face() {
    let mut host = HostSession::new("rae");
    host.deal(0, faces("host", 2)).unwrap();
    let card = hand(host.state(), 0)[0];
    play(&mut host, 0, card, 0, 0);
    let mut state = fold(host.log());
    let public = state.table.get(CardId(card)).unwrap().face.clone();
    let again = LogEntry::new(
        state.next_seq,
        0,
        LogAction::Reveal {
            card,
            face: WireFace::from(&public),
        },
    );
    assert_eq!(fold_entry(&mut state, &again), Ok(()));
    assert_eq!(state.table.get(CardId(card)).unwrap().face, public);
}

fn riftbound_host() -> (HostSession, u8) {
    let mut host = HostSession::with_config(
        "rae",
        TableConfig {
            engine: Some("blob:test-engine".into()),
            plugin: Some("blob:test-riftbound".into()),
            zones: zone_table(),
            options: Some(ByteBuf::from(vec![0xa0])),
            counters: Vec::new(),
            despawn_any: false,
        },
    );
    let (ada, _) = host.join("ada").unwrap();
    host.deal_to(0, faces("secret", 4), WireZone::Plugin(ZONE_MAIN_DECK))
        .unwrap();
    host.deal_to(0, faces("held", 2), WireZone::Plugin(ZONE_HAND))
        .unwrap();
    host.deal_to(ada, faces("arcane", 4), WireZone::Plugin(ZONE_MAIN_DECK))
        .unwrap();
    host.deal_to(ada, faces("legend", 1), WireZone::Plugin(ZONE_LEGEND))
        .unwrap();
    (host, ada)
}

fn in_zone(state: &LogState, seat: u8, zone: Zone) -> Vec<u32> {
    state
        .table
        .in_area(PlayerId(seat), zone)
        .map(|card| card.id.0)
        .collect()
}

fn exhaust(host: &mut HostSession, seat: u8, card: u32, on: bool) -> Vec<LogEntry> {
    host.intent(
        seat,
        WireIntent::Annotate {
            card,
            key: "exhausted".into(),
            value: on.then(|| ByteBuf::from(vec![0xf5])),
        },
    )
    .expect("annotate intent is valid")
}

fn riftbound_script() -> (HostSession, u8) {
    let (mut host, ada) = riftbound_host();
    let held = in_zone(host.state(), 0, Zone::Plugin(ZONE_HAND));
    host.intent(
        0,
        WireIntent::Move {
            card: held[0],
            to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST),
            seat: 0,
            index: 0,
        },
    )
    .expect("hand card reaches the battlefield");
    let deck = in_zone(host.state(), 0, Zone::Plugin(ZONE_MAIN_DECK));
    host.intent(
        0,
        WireIntent::Move {
            card: deck[0],
            to: WireZone::Plugin(ZONE_HAND),
            seat: 0,
            index: 0,
        },
    )
    .expect("a draw folds");
    let ada_deck = in_zone(host.state(), ada, Zone::Plugin(ZONE_MAIN_DECK));
    host.intent(
        ada,
        WireIntent::Move {
            card: ada_deck[0],
            to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST + 1),
            seat: ada,
            index: 0,
        },
    )
    .expect("a deck card flips onto a battlefield");
    let field = in_zone(host.state(), 0, Zone::Plugin(ZONE_BATTLEFIELD_FIRST));
    exhaust(&mut host, ada, field[0], true);
    exhaust(&mut host, ada, field[0], false);
    exhaust(&mut host, 0, field[0], true);
    host.intent(
        ada,
        WireIntent::Game {
            data: ByteBuf::from(vec![1, 2, 3]),
        },
    )
    .expect("a game entry folds");
    host.intent(
        0,
        WireIntent::Move {
            card: field[0],
            to: WireZone::Plugin(ZONE_MAIN_DECK),
            seat: 0,
            index: 0,
        },
    )
    .expect("a battlefield card returns to the deck");
    (host, ada)
}

#[test]
fn riftbound_zones_fold_identically_and_deck_faces_never_enter_the_log() {
    let (host, ada) = riftbound_script();
    let first = fold(host.log());
    let second = fold(host.log());
    assert_eq!(first, second);
    assert_eq!(first, *host.state());
    let encoded = encode_log(host.log());
    let log_text = String::from_utf8_lossy(&encoded).into_owned();
    assert!(!log_text.contains("secret"));
    assert!(!log_text.contains("arcane 1"));
    assert!(log_text.contains("arcane 0"));
    assert!(log_text.contains("held 0"));
    for card in in_zone(&first, 0, Zone::Plugin(ZONE_MAIN_DECK)) {
        assert_eq!(first.table.get(CardId(card)).unwrap().face, hidden_face());
    }
    for card in in_zone(&first, ada, Zone::Plugin(ZONE_MAIN_DECK)) {
        assert_eq!(first.table.get(CardId(card)).unwrap().face, hidden_face());
    }
}

#[test]
fn a_deck_deal_hands_out_no_wire_faces_but_the_sequencer_can_serve_the_owner() {
    let (mut host, _) = riftbound_host();
    let (_, wire) = host
        .deal_to(0, faces("buried", 2), WireZone::Plugin(ZONE_RUNE_DECK))
        .unwrap();
    assert!(wire.is_empty());
    let deck = in_zone(host.state(), 0, Zone::Plugin(ZONE_RUNE_DECK));
    let entries = host
        .intent(
            0,
            WireIntent::Move {
                card: deck[0],
                to: WireZone::Plugin(ZONE_HAND),
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert!(matches!(entries[0].action, LogAction::Move { .. }));
    assert!(entries[1..]
        .iter()
        .all(|entry| matches!(entry.action, LogAction::Reveal { .. })));
    let state = fold(host.log());
    assert_eq!(
        state.table.get(CardId(deck[0])).unwrap().face,
        hidden_face()
    );
    let (card, face) = host.face_of(deck[0]).unwrap();
    assert_eq!(card, deck[0]);
    assert_eq!(face.name, "buried 0");
}

#[test]
fn a_hand_card_played_to_a_battlefield_reveals_before_the_move() {
    let (mut host, _ada) = riftbound_host();
    let hand = in_zone(host.state(), 0, Zone::Plugin(ZONE_HAND));
    let entries = host
        .intent(
            0,
            WireIntent::Move {
                card: hand[0],
                to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST),
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert!(entries.len() >= 2);
    assert!(matches!(entries[0].action, LogAction::Reveal { .. }));
    assert!(matches!(entries[1].action, LogAction::Move { .. }));
    assert!(entries[2..]
        .iter()
        .all(|entry| matches!(entry.action, LogAction::Reveal { .. })));
    let state = fold(host.log());
    let card = state.table.get(CardId(hand[0])).unwrap();
    assert_eq!(card.zone, Zone::Plugin(ZONE_BATTLEFIELD_FIRST));
    assert!(!card.face.name.is_empty());
}

#[test]
fn a_hidden_play_stays_face_down_until_its_owner_reveals_it() {
    let (mut host, _ada) = riftbound_host();
    let hand = in_zone(host.state(), 0, Zone::Plugin(ZONE_HAND));
    let entries = host
        .intent(
            0,
            WireIntent::MoveHidden {
                card: hand[0],
                to: WireZone::Plugin(ZONE_BASE),
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert!(entries
        .iter()
        .all(|entry| !matches!(entry.action, LogAction::Reveal { card, .. } if card == hand[0])));
    let later = host
        .intent(
            0,
            WireIntent::Move {
                card: hand[1],
                to: WireZone::Plugin(ZONE_BASE),
                seat: 0,
                index: 1,
            },
        )
        .unwrap();
    assert!(later
        .iter()
        .all(|entry| !matches!(entry.action, LogAction::Reveal { card, .. } if card == hand[0])));
    assert!(!host.state().revealed.contains(&hand[0]));
    let shown = host
        .intent(0, WireIntent::Reveal { card: hand[0] })
        .unwrap();
    assert!(matches!(shown[0].action, LogAction::Reveal { card, .. } if card == hand[0]));
    assert!(host.state().revealed.contains(&hand[0]));
}

#[test]
fn a_deck_card_played_straight_to_a_battlefield_reveals_after_the_move() {
    let (mut host, ada) = riftbound_host();
    let deck = in_zone(host.state(), ada, Zone::Plugin(ZONE_MAIN_DECK));
    let entries = host
        .intent(
            ada,
            WireIntent::Move {
                card: deck[0],
                to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST),
                seat: ada,
                index: 0,
            },
        )
        .unwrap();
    assert!(entries.len() >= 2);
    assert!(matches!(entries[0].action, LogAction::Move { .. }));
    assert!(matches!(entries[1].action, LogAction::Reveal { .. }));
    let state = fold(host.log());
    assert_eq!(
        state.table.get(CardId(deck[0])).unwrap().face.name,
        "arcane 0"
    );
}

#[test]
fn tap_state_replicates_byte_identically_at_three_replicas() {
    let (host, _) = riftbound_script();
    let replicas = [fold(host.log()), fold(host.log()), fold(host.log())];
    for replica in &replicas {
        assert_eq!(replica.annotations, host.state().annotations);
    }
    let field = in_zone(host.state(), 0, Zone::Plugin(ZONE_BATTLEFIELD_FIRST));
    for card in field {
        assert_eq!(
            replicas[0].annotation(card, "exhausted"),
            host.state().annotation(card, "exhausted")
        );
    }
    let encodings: Vec<Vec<u8>> = replicas.iter().map(|_| encode_log(host.log())).collect();
    assert!(encodings.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn new_actions_survive_shuffled_arrival_ordered_only_by_seq() {
    let (host, _) = riftbound_script();
    let shuffled: Vec<LogEntry> = host.log().iter().rev().cloned().collect();
    let mut replica = LogState::new();
    let mut settled = false;
    while !settled {
        settled = true;
        for entry in &shuffled {
            if entry.seq == replica.next_seq {
                assert_eq!(fold_entry(&mut replica, entry), Ok(()));
                settled = false;
            } else {
                assert_eq!(fold_entry(&mut replica, entry), Err(FoldError::BadSeq));
            }
        }
    }
    assert_eq!(replica, *host.state());
}

#[test]
fn riftbound_entries_reencode_byte_identically() {
    let (host, _) = riftbound_script();
    for entry in host.log() {
        let bytes = encode_entry(entry);
        let decoded = decode_entry(&bytes).unwrap();
        assert_eq!(&decoded, entry);
        assert_eq!(encode_entry(&decoded), bytes);
    }
}

#[test]
fn a_public_face_sheds_deterministically_on_reentering_a_deck() {
    let (host, _) = riftbound_script();
    let state = fold(host.log());
    let deck = in_zone(&state, 0, Zone::Plugin(ZONE_MAIN_DECK));
    let returned = deck
        .iter()
        .find(|card| {
            host.log().iter().any(|entry| {
                matches!(&entry.action, LogAction::Reveal { card: revealed, .. } if revealed == *card)
            })
        })
        .copied()
        .expect("a once-public card sits in the deck");
    assert_eq!(
        state.table.get(CardId(returned)).unwrap().face,
        hidden_face()
    );
    assert!(!state.revealed.contains(&returned));
}

struct CountingDecider;

impl Decider for CountingDecider {
    fn decide(&mut self, blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
        let count = u64::from_le_bytes(blob.try_into().unwrap_or([0; 8]));
        Verdict {
            accept: true,
            plugin_state: Some(ByteBuf::from((count + 1).to_le_bytes().to_vec())),
            effects: Vec::new(),
            reason: None,
        }
    }
}

#[test]
fn plugin_state_threads_deterministically_through_a_recorded_log() {
    let (host, _) = riftbound_script();
    let first = fold_with(host.log(), &mut CountingDecider);
    let second = fold_with(host.log(), &mut CountingDecider);
    assert_eq!(first, second);
    let count = u64::from_le_bytes(first.plugin_state.as_slice().try_into().unwrap());
    assert_eq!(count, host.log().len() as u64);
    assert_eq!(first.table, host.state().table);
    assert!(fold(host.log()).plugin_state.is_empty());
    let snapshot = first.clone();
    assert_eq!(snapshot.plugin_state, first.plugin_state);
}

#[test]
fn a_reset_entry_clears_cards_and_reveals_but_keeps_seats() {
    let host = scripted();
    let state = host.state();
    assert_eq!(state.seats.len(), 2);
    assert_eq!(state.table.cards().len(), 4);
    assert_eq!(state.revealed.len(), 1);
    let mut wiped = state.clone();
    let entry = LogEntry::new(wiped.next_seq, 0, LogAction::Reset);
    assert_eq!(fold_entry(&mut wiped, &entry), Ok(()));
    assert!(wiped.table.cards().is_empty());
    assert!(wiped.revealed.is_empty());
    assert_eq!(wiped.seats.len(), 2);
}

struct Relocator {
    to: WireZone,
    on_move: bool,
}

impl agni_sim::engine::PluginModule for Relocator {
    fn decide(&mut self, request: &[u8]) -> Result<Verdict, agni_sim::engine::EngineFault> {
        let request: agni_sim::abi::DecideRequest = agni_sim::abi::decode(request)
            .ok_or_else(|| agni_sim::engine::EngineFault("bad request".into()))?;
        let card = match &request.entry.action {
            LogAction::Game { data } => {
                u32::from_le_bytes(data.as_slice().try_into().unwrap_or([0; 4]))
            }
            LogAction::Move { card, .. } if self.on_move => *card,
            _ => return Ok(Verdict::accept()),
        };
        Ok(
            Verdict::accept().with_effects(vec![agni_sim::log::Effect::Move {
                card,
                to: self.to,
                seat: 0,
                index: 0,
            }]),
        )
    }

    fn module_hash(&self) -> Option<[u8; 32]> {
        None
    }
}

#[test]
fn a_hidden_play_the_rules_move_elsewhere_is_revealed_where_it_lands() {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: zone_table(),
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
        Box::new(agni_sim::engine::NativeEngine::new()),
        Some(Box::new(Relocator {
            to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST + 1),
            on_move: false,
        })),
    )
    .unwrap();
    host.deal_to(0, faces("secret", 2), WireZone::Plugin(ZONE_HAND))
        .unwrap();
    let hand = in_zone(host.state(), 0, Zone::Plugin(ZONE_HAND));
    let hidden = host
        .intent(
            0,
            WireIntent::MoveHidden {
                card: hand[0],
                to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST),
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert!(hidden
        .iter()
        .all(|entry| !matches!(entry.action, LogAction::Reveal { .. })));
    assert_eq!(host.hidden_plays(), [hand[0]]);
    let elsewhere = host
        .intent(
            0,
            WireIntent::Game {
                data: ByteBuf::from(hand[1].to_le_bytes().to_vec()),
            },
        )
        .unwrap();
    assert!(elsewhere
        .iter()
        .all(|entry| !matches!(entry.action, LogAction::Reveal { card, .. } if card == hand[0])));
    assert_eq!(host.hidden_plays(), [hand[0]]);
    assert!(!host.state().revealed.contains(&hand[0]));
    let relocated = host
        .intent(
            0,
            WireIntent::Game {
                data: ByteBuf::from(hand[0].to_le_bytes().to_vec()),
            },
        )
        .unwrap();
    assert!(matches!(relocated[0].action, LogAction::Game { .. }));
    assert!(relocated[1..]
        .iter()
        .any(|entry| matches!(entry.action, LogAction::Reveal { card, .. } if card == hand[0])));
    assert!(host.hidden_plays().is_empty());
    assert!(host.state().revealed.contains(&hand[0]));
    let landed = host.state().table.get(CardId(hand[0])).unwrap();
    assert_eq!(landed.zone, Zone::Plugin(ZONE_BATTLEFIELD_FIRST + 1));
    assert_eq!(landed.face.name, "secret 0");
    assert_eq!(fold(host.log()).revealed, host.state().revealed);
}

#[test]
fn a_hidden_play_the_rules_relocate_inside_its_own_fold_is_revealed_at_once() {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: zone_table(),
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
        Box::new(agni_sim::engine::NativeEngine::new()),
        Some(Box::new(Relocator {
            to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST + 1),
            on_move: true,
        })),
    )
    .unwrap();
    host.deal_to(0, faces("secret", 2), WireZone::Plugin(ZONE_HAND))
        .unwrap();
    let hand = in_zone(host.state(), 0, Zone::Plugin(ZONE_HAND));
    let entries = host
        .intent(
            0,
            WireIntent::MoveHidden {
                card: hand[0],
                to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST),
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert!(matches!(entries[0].action, LogAction::Move { .. }));
    assert!(entries[1..]
        .iter()
        .any(|entry| matches!(entry.action, LogAction::Reveal { card, .. } if card == hand[0])));
    assert!(host.hidden_plays().is_empty());
    assert!(host.state().revealed.contains(&hand[0]));
    let landed = host.state().table.get(CardId(hand[0])).unwrap();
    assert_eq!(landed.zone, Zone::Plugin(ZONE_BATTLEFIELD_FIRST + 1));
    assert_eq!(landed.face.name, "secret 0");
    assert_eq!(fold(host.log()).revealed, host.state().revealed);
}
