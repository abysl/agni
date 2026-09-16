use agni_core::{CardFace, CardId, Intent, PlayerId, Table, Zone};
use agni_net::session::{
    decode_client, decode_host, default_seat_color, encode_client, encode_host, engine_blob_ref,
    genesis_plugin_pin, module_matches_pin, pin_hash, roster_color, verify_plugin_pin, ClientMsg,
    ClientSession, DealGroup, DealTarget, HostMsg, HostSession, SeatInfo, SessionError, WireIntent,
    SEAT_PICKABLE_COLORS, WIRE_VERSION,
};
use agni_sim::engine::{EngineFault, NativeEngine, PluginModule};
use agni_sim::log::{encode_log, fold, FoldError, LogAction, TableConfig, Verdict};
use agni_sim::wire::{ZoneDecl, ZoneKind, ZoneLayout, ZoneOwner, ZonePlace, ZoneVisibility};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

fn faces(prefix: &str, n: usize) -> Vec<CardFace> {
    (0..n)
        .map(|i| CardFace::named(format!("{prefix} {i}")))
        .collect()
}

fn area(table: &Table, seat: u8, zone: Zone) -> Vec<u32> {
    table
        .in_area(PlayerId(seat), zone)
        .map(|card| card.id.0)
        .collect()
}

fn areas_agree(host: &Table, replica: &Table, seats: u8) {
    for seat in 0..seats {
        for zone in [Zone::Hand, Zone::Board] {
            assert_eq!(area(host, seat, zone), area(replica, seat, zone));
        }
    }
}

#[test]
fn a_returning_node_reclaims_its_seat_instead_of_taking_a_new_one() {
    let mut host = HostSession::new("rae");
    let (seat, entry) = host.join_as("node-ada", "ada").unwrap();
    assert_eq!(seat, 1);
    assert!(entry.is_some());
    host.disconnect(seat);
    assert!(!host.roster()[1].connected);
    let log_before = host.log().len();
    let (again, entry) = host.join_as("node-ada", "ada on her phone").unwrap();
    assert_eq!(again, seat);
    assert!(entry.is_none());
    assert_eq!(host.log().len(), log_before);
    assert_eq!(host.roster().len(), 2);
    assert!(host.roster()[1].connected);
    assert_eq!(host.roster()[1].name, "ada on her phone");
    assert_eq!(host.seat_of_node("node-ada"), Some(seat));
    let (fresh, entry) = host.join_as("node-bo", "bo").unwrap();
    assert_eq!(fresh, 2);
    assert!(entry.is_some());
}

#[test]
fn a_reclaimed_seat_is_handed_back_its_own_hidden_faces_and_nobody_elses() {
    let mut host = HostSession::new("rae");
    let (seat, _) = host.join_as("node-ada", "ada").unwrap();
    host.deal(0, faces("host", 3)).unwrap();
    host.deal(seat, faces("ada", 4)).unwrap();
    let owed = host.faces_owed_to(seat);
    assert_eq!(owed.len(), 4);
    assert!(owed.iter().all(|(_, face)| face.name.starts_with("ada")));
    let hers: Vec<u32> = host
        .state()
        .table
        .in_area(PlayerId(seat), Zone::Hand)
        .map(|card| card.id.0)
        .collect();
    assert_eq!(
        owed.iter().map(|(id, _)| *id).collect::<Vec<u32>>(),
        hers,
        "the reclaim owes exactly the seat's own hand"
    );
    let mut returning = ClientSession::from_welcome(seat, host.roster(), host.log().to_vec());
    returning.add_faces(owed);
    let table = returning.table();
    for id in &hers {
        assert!(table.get(CardId(*id)).unwrap().face.name.starts_with("ada"));
    }
}

fn library_decl() -> ZoneDecl {
    ZoneDecl {
        id: 1,
        name: "library".into(),
        kind: ZoneKind::Deck,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::None,
        place: ZonePlace::Offstage,
        layout: ZoneLayout::Pile,
        span: 1,
        label: "library".into(),
    }
}

#[test]
fn a_reclaimed_seat_never_carries_the_faces_of_a_face_down_zone() {
    let mut host = HostSession::with_config(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: vec![library_decl()],
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
    );
    let (seat, _) = host.join_as("node-ada", "ada").unwrap();
    host.deal_to(seat, faces("secret", 5), Zone::Plugin(1))
        .unwrap();
    assert!(host.faces_owed_to(seat).is_empty());
}

struct Fixture {
    host: HostSession,
    dealt: BTreeMap<u8, Vec<(u32, CardFace)>>,
}

fn hosted(joiners: &[&str], hand: usize) -> Fixture {
    let mut host = HostSession::new("rae");
    let mut dealt = BTreeMap::new();
    let (_, wire) = host.deal(0, faces("host", hand)).unwrap();
    dealt.insert(0, wire);
    for name in joiners {
        let (seat, _) = host.join(name).unwrap();
        let (_, wire) = host.deal(seat, faces(name, hand)).unwrap();
        dealt.insert(seat, wire);
    }
    Fixture { host, dealt }
}

fn client_for(fixture: &Fixture, seat: u8) -> ClientSession {
    let mut client =
        ClientSession::from_welcome(seat, fixture.host.roster(), fixture.host.log().to_vec());
    if let Some(wire) = fixture.dealt.get(&seat) {
        client.add_faces(wire.clone());
    }
    client
}

fn host_move(fixture: &mut Fixture, from: u8, card: u32, seat: u8) -> Vec<agni_sim::log::LogEntry> {
    fixture
        .host
        .intent(
            from,
            WireIntent::Move {
                card,
                to: Zone::Board,
                seat,
                index: 0,
            },
        )
        .unwrap()
}

#[test]
fn seats_assign_in_join_order_with_the_host_first() {
    let mut host = HostSession::new("rae");
    assert_eq!(host.join("ada").unwrap().0, 1);
    assert_eq!(host.join("lin").unwrap().0, 2);
    let roster = host.roster();
    assert_eq!(roster.len(), 3);
    assert!(roster[0].host);
    assert_eq!(roster[1].name, "ada");
    assert_eq!(roster[2].seat, 2);
}

#[test]
fn a_folded_replica_hides_other_hands_but_keeps_their_size() {
    let fixture = hosted(&["ada", "lin"], 3);
    let client = client_for(&fixture, 2);
    let table = client.table();
    let mine: Vec<u32> = area(&table, 2, Zone::Hand);
    assert_eq!(mine.len(), 3);
    for id in &mine {
        assert!(table.get(CardId(*id)).unwrap().face.name.starts_with("lin"));
    }
    for seat in [0u8, 1] {
        let theirs = area(&table, seat, Zone::Hand);
        assert_eq!(theirs.len(), 3);
        for id in &theirs {
            assert!(table.get(CardId(*id)).unwrap().face.is_hidden());
        }
    }
}

#[test]
fn board_cards_stay_public_in_every_replica() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let hand = area(&fixture.host.table(), 0, Zone::Hand);
    let entries = host_move(&mut fixture, 0, hand[0], 0);
    for entry in entries {
        assert!(client.apply(entry).unwrap());
    }
    let table = client.table();
    let board = area(&table, 0, Zone::Board);
    assert_eq!(board.len(), 1);
    assert_eq!(table.get(CardId(board[0])).unwrap().face.name, "host 0");
}

#[test]
fn replicas_agree_with_the_host_after_a_run_of_entries() {
    let mut fixture = hosted(&["ada", "lin"], 3);
    let mut early = client_for(&fixture, 1);
    let moves = [
        (0u8, area(&fixture.host.table(), 0, Zone::Hand)[0], 0u8),
        (1u8, area(&fixture.host.table(), 1, Zone::Hand)[1], 2u8),
        (2u8, area(&fixture.host.table(), 2, Zone::Hand)[0], 1u8),
    ];
    let mut entries = Vec::new();
    for (from, card, seat) in moves.iter().take(2) {
        entries.extend(host_move(&mut fixture, *from, *card, *seat));
    }
    let mut late = client_for(&fixture, 2);
    let (from, card, seat) = moves[2];
    entries.extend(host_move(&mut fixture, from, card, seat));
    for entry in entries {
        early.apply(entry.clone()).unwrap();
        late.apply(entry).unwrap();
    }
    areas_agree(&fixture.host.table(), &early.table(), 3);
    areas_agree(&fixture.host.table(), &late.table(), 3);
}

#[test]
fn a_hand_card_is_revealed_when_it_hits_the_board() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let host_hand = area(&fixture.host.table(), 0, Zone::Hand);
    assert!(client
        .table()
        .get(CardId(host_hand[0]))
        .unwrap()
        .face
        .is_hidden());
    let entries = host_move(&mut fixture, 0, host_hand[0], 0);
    assert!(matches!(entries[0].action, LogAction::Reveal { .. }));
    assert!(matches!(entries[1].action, LogAction::Move { .. }));
    for entry in entries {
        client.apply(entry).unwrap();
    }
    assert_eq!(
        client.table().get(CardId(host_hand[0])).unwrap().face.name,
        "host 0"
    );
}

#[test]
fn a_hidden_card_played_onto_another_seats_public_zone_is_revealed_too() {
    let field = ZoneDecl {
        id: 2,
        name: "field".into(),
        kind: ZoneKind::Battlefield,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        place: ZonePlace::Inner,
        layout: ZoneLayout::Row,
        span: 4,
        label: "Field".into(),
    };
    let mut host = HostSession::with_config(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: vec![library_decl(), field],
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
    );
    let (ada, _) = host.join("ada").unwrap();
    host.deal_to(ada, faces("ada lib", 3), Zone::Plugin(1))
        .unwrap();
    let card = area(&host.table(), ada, Zone::Plugin(1))[0];
    let entries = host
        .intent(
            ada,
            WireIntent::Move {
                card,
                to: Zone::Plugin(2),
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[1].action, LogAction::Reveal { .. }));
    let landed = host.state().table.get(CardId(card)).unwrap();
    assert_eq!(landed.zone, Zone::Plugin(2));
    assert_eq!(landed.seat, PlayerId(0));
    assert!(host.state().revealed.contains(&card));
    let mut client = ClientSession::from_welcome(0, host.roster(), host.log().to_vec());
    assert_eq!(
        client.table().get(CardId(card)).unwrap().face.name,
        "ada lib 0"
    );
    let (lin, join) = host.join("lin").unwrap();
    assert!(client.apply(join).unwrap());
    let _ = lin;
}

#[test]
fn a_hand_card_the_other_seat_has_seen_is_secret_again_once_it_is_hidden() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let card = area(&fixture.host.table(), 0, Zone::Hand)[0];
    let shown = fixture.host.intent(0, WireIntent::Reveal { card }).unwrap();
    for entry in shown {
        client.apply(entry).unwrap();
    }
    assert_eq!(
        client.table().get(CardId(card)).unwrap().face.name,
        "host 0",
        "a reveal in place shows the joiner the face"
    );
    assert!(client.state().shown_in_place(card));
    let hidden = fixture
        .host
        .intent(
            0,
            WireIntent::MoveHidden {
                card,
                to: Zone::Board,
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert_eq!(hidden.len(), 1, "no reveal accompanies a hide");
    assert!(
        matches!(hidden[0].action, LogAction::Move { hidden: true, .. }),
        "the log records the move as a hide"
    );
    for entry in hidden {
        client.apply(entry).unwrap();
    }
    for state in [fixture.host.state(), client.state()] {
        assert!(!state.revealed.contains(&card));
        assert!(!state.shown_in_place(card));
        assert!(state.table.get(CardId(card)).unwrap().face.is_hidden());
    }
    assert!(
        client.table().get(CardId(card)).unwrap().face.is_hidden(),
        "the joiner's replica forgets the face it was shown"
    );
    assert!(
        fixture
            .host
            .table_for(1)
            .get(CardId(card))
            .unwrap()
            .face
            .is_hidden(),
        "and the host renders it face down for that seat"
    );
    assert_eq!(
        fixture
            .host
            .table_for(0)
            .get(CardId(card))
            .unwrap()
            .face
            .name,
        "host 0",
        "while the hider still sees their own card"
    );
    assert_eq!(fixture.host.hidden_plays(), [card]);
    let other = area(&fixture.host.table(), 0, Zone::Hand)[0];
    for entry in host_move(&mut fixture, 0, other, 0) {
        client.apply(entry).unwrap();
    }
    let played = fixture
        .host
        .intent(
            0,
            WireIntent::Move {
                card,
                to: Zone::Board,
                seat: 0,
                index: 0,
            },
        )
        .unwrap();
    assert!(
        played.iter().any(
            |entry| matches!(entry.action, LogAction::Reveal { card: shown, .. } if shown == card)
        ),
        "playing it later reveals it again"
    );
    for entry in played {
        client.apply(entry).unwrap();
    }
    assert_eq!(
        client.table().get(CardId(card)).unwrap().face.name,
        "host 0"
    );
}

#[test]
fn stale_and_duplicate_entries_are_ignored() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let hand = area(&client.table(), 1, Zone::Hand);
    let entries = host_move(&mut fixture, 1, hand[0], 1);
    for entry in &entries {
        assert!(client.apply(entry.clone()).unwrap());
    }
    for entry in &entries {
        assert!(!client.apply(entry.clone()).unwrap());
    }
    assert_eq!(area(&client.table(), 1, Zone::Board).len(), 1);
}

#[test]
fn a_player_cannot_move_a_card_out_of_anothers_hand() {
    let mut fixture = hosted(&["ada"], 2);
    let host_hand = area(&fixture.host.table(), 0, Zone::Hand);
    let refused = fixture.host.intent(
        1,
        WireIntent::Move {
            card: host_hand[0],
            to: Zone::Board,
            seat: 1,
            index: 0,
        },
    );
    assert_eq!(refused, Err(SessionError::Refused(FoldError::ForeignHand)));
    assert_eq!(area(&fixture.host.table(), 0, Zone::Hand), host_hand);
}

#[test]
fn a_player_cannot_stuff_a_card_into_anothers_hand() {
    let mut fixture = hosted(&["ada"], 2);
    let hand = area(&fixture.host.table(), 1, Zone::Hand);
    let refused = fixture.host.intent(
        1,
        WireIntent::Move {
            card: hand[0],
            to: Zone::Hand,
            seat: 0,
            index: 0,
        },
    );
    assert_eq!(
        refused,
        Err(SessionError::Refused(FoldError::ForeignHandTarget))
    );
}

#[test]
fn a_refusal_explains_itself() {
    let mut fixture = hosted(&["ada"], 2);
    let refused = fixture
        .host
        .intent(
            9,
            WireIntent::Game {
                data: ByteBuf::from(vec![1]),
            },
        )
        .unwrap_err();
    assert!(refused.to_string().contains("seat"));
    assert!(!refused.is_engine_fault());
    assert!(refused.applied().is_empty());
    assert_eq!(
        fixture.host.deal_groups(9, Vec::new()),
        Err(SessionError::UnknownSeat(9))
    );
}

#[test]
fn a_reset_entry_replaces_every_replica() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let hands = vec![(0u8, faces("fresh-host", 4)), (1u8, faces("fresh-ada", 4))];
    let (entries, dealt) = fixture.host.reset(hands).unwrap();
    for entry in entries {
        client.apply(entry).unwrap();
    }
    for (seat, wire) in dealt {
        if seat == client.seat() {
            client.add_faces(wire);
        }
    }
    let table = client.table();
    areas_agree(&fixture.host.table(), &table, 2);
    assert_eq!(area(&table, 1, Zone::Hand).len(), 4);
    assert!(table
        .in_area(PlayerId(0), Zone::Hand)
        .all(|card| card.face.is_hidden()));
    assert!(table
        .in_area(PlayerId(1), Zone::Hand)
        .all(|card| card.face.name.starts_with("fresh-ada")));
}

#[test]
fn wire_messages_survive_a_cbor_round_trip() {
    let fixture = hosted(&["ada"], 2);
    let host_msg = HostMsg::Welcome {
        version: WIRE_VERSION,
        seat: 1,
        roster: fixture.host.roster(),
        log: fixture.host.log().to_vec(),
    };
    assert_eq!(decode_host(&encode_host(&host_msg)).unwrap(), host_msg);
    let faces_msg = HostMsg::Faces {
        faces: fixture.dealt[&1].clone(),
    };
    assert_eq!(decode_host(&encode_host(&faces_msg)).unwrap(), faces_msg);
    let client_msg = ClientMsg::Intent {
        intent: WireIntent::Move {
            card: 2,
            to: Zone::Plugin(3),
            seat: 1,
            index: 4,
        },
    };
    assert_eq!(
        decode_client(&encode_client(&client_msg)).unwrap(),
        client_msg
    );
    let annotate_msg = ClientMsg::Intent {
        intent: WireIntent::Annotate {
            card: 2,
            key: "exhausted".into(),
            value: Some(ByteBuf::from(vec![0xf5])),
        },
    };
    assert_eq!(
        decode_client(&encode_client(&annotate_msg)).unwrap(),
        annotate_msg
    );
    let game_msg = ClientMsg::Intent {
        intent: WireIntent::Game {
            data: ByteBuf::from(vec![9, 9]),
        },
    };
    assert_eq!(decode_client(&encode_client(&game_msg)).unwrap(), game_msg);
    let join = ClientMsg::Join {
        name: "ada".into(),
        version: WIRE_VERSION,
    };
    assert_eq!(decode_client(&encode_client(&join)).unwrap(), join);
}

#[test]
fn intents_round_trip_through_log_actions() {
    let intent = WireIntent::Annotate {
        card: 4,
        key: "exhausted".into(),
        value: None,
    };
    let action = LogAction::from(intent.clone());
    assert_eq!(WireIntent::try_from(action), Ok(intent));
    assert!(WireIntent::try_from(LogAction::Reset).is_err());
    let hide = WireIntent::MoveHidden {
        card: 4,
        to: Zone::Board,
        seat: 0,
        index: 0,
    };
    let action = LogAction::from(hide.clone());
    assert!(matches!(action, LogAction::Move { hidden: true, .. }));
    assert_eq!(WireIntent::try_from(action), Ok(hide));
}

#[test]
fn an_annotate_intent_replicates_to_every_client() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let hand = area(&fixture.host.table(), 0, Zone::Hand);
    let played = host_move(&mut fixture, 0, hand[0], 0);
    for entry in played {
        client.apply(entry).unwrap();
    }
    let board = area(&fixture.host.table(), 0, Zone::Board);
    let entries = fixture
        .host
        .intent(
            1,
            WireIntent::Annotate {
                card: board[0],
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![0xf5])),
            },
        )
        .unwrap();
    for entry in entries {
        assert!(client.apply(entry).unwrap());
    }
    assert_eq!(
        client.state().annotation(board[0], "exhausted"),
        Some(&[0xf5][..])
    );
    assert_eq!(client.state().annotations, fixture.host.state().annotations);
    let refused = fixture.host.intent(
        1,
        WireIntent::Annotate {
            card: hand[1],
            key: "exhausted".into(),
            value: Some(ByteBuf::from(vec![0xf5])),
        },
    );
    assert!(refused.is_err());
}

#[test]
fn a_game_intent_replicates_without_moving_cards() {
    let mut fixture = hosted(&["ada"], 2);
    let mut client = client_for(&fixture, 1);
    let before = client.table();
    let entries = fixture
        .host
        .intent(
            1,
            WireIntent::Game {
                data: ByteBuf::from(vec![7]),
            },
        )
        .unwrap();
    for entry in entries {
        assert!(client.apply(entry).unwrap());
    }
    assert_eq!(client.table(), before);
    assert_eq!(encode_log(fixture.host.log()), encode_log(client.log()));
}

#[test]
fn no_op_moves_produce_no_entry() {
    let mut fixture = hosted(&[], 2);
    let hand = area(&fixture.host.table(), 0, Zone::Hand);
    let before = fixture.host.log().len();
    let refused = fixture.host.intent(
        0,
        WireIntent::Move {
            card: hand[0],
            to: Zone::Hand,
            seat: 0,
            index: 0,
        },
    );
    assert_eq!(refused, Err(SessionError::Refused(FoldError::NoOp)));
    assert_eq!(fixture.host.log().len(), before);
}

#[test]
fn an_echoed_entry_replaces_the_optimistic_move() {
    let mut fixture = hosted(&["ada"], 3);
    let mut client = client_for(&fixture, 1);
    let hand = area(&client.table(), 1, Zone::Hand);
    let intent = WireIntent::Move {
        card: hand[0],
        to: Zone::Board,
        seat: 1,
        index: 0,
    };
    client.optimistic(intent.clone());
    assert_eq!(client.pending_len(), 1);
    assert_eq!(area(&client.table(), 1, Zone::Board), vec![hand[0]]);
    let entries = fixture.host.intent(1, intent).unwrap();
    for entry in entries {
        client.apply(entry).unwrap();
    }
    assert_eq!(client.pending_len(), 0);
    areas_agree(&fixture.host.table(), &client.table(), 2);
}

#[test]
fn the_log_is_byte_identical_at_every_seat_and_carries_no_hidden_faces() {
    let mut fixture = hosted(&["ada", "lin"], 3);
    let mut ada = client_for(&fixture, 1);
    let mut lin = client_for(&fixture, 2);
    let hand = area(&fixture.host.table(), 1, Zone::Hand);
    for entry in fixture
        .host
        .intent(
            1,
            WireIntent::Move {
                card: hand[0],
                to: Zone::Board,
                seat: 1,
                index: 0,
            },
        )
        .unwrap()
    {
        ada.apply(entry.clone()).unwrap();
        lin.apply(entry).unwrap();
    }
    assert_eq!(encode_log(fixture.host.log()), encode_log(ada.log()));
    assert_eq!(encode_log(ada.log()), encode_log(lin.log()));
    for entry in fixture.host.log() {
        match &entry.action {
            LogAction::Reveal { face, .. } => assert!(!face.is_hidden()),
            LogAction::Deal { cards, .. } => assert!(!cards.is_empty()),
            _ => {}
        }
    }
}

struct FakePlugin {
    hash: Option<[u8; 32]>,
    reject: fn(&LogAction) -> bool,
}

impl PluginModule for FakePlugin {
    fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault> {
        let request: agni_sim::abi::DecideRequest =
            agni_sim::abi::decode(request).ok_or_else(|| EngineFault("bad request".into()))?;
        Ok(if (self.reject)(&request.entry.action) {
            Verdict::reject()
        } else {
            Verdict::accept()
        })
    }

    fn module_hash(&self) -> Option<[u8; 32]> {
        self.hash
    }
}

fn plugin(hash: Option<[u8; 32]>) -> Box<FakePlugin> {
    Box::new(FakePlugin {
        hash,
        reject: |_| false,
    })
}

#[test]
fn a_hosted_plugin_pins_its_hash_into_the_genesis() {
    let hash = *blake3::hash(b"hardened plugin").as_bytes();
    let host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(NativeEngine::new()),
        Some(plugin(Some(hash))),
    )
    .unwrap();
    let pin = genesis_plugin_pin(host.log()).unwrap();
    assert_eq!(pin, engine_blob_ref(hash));
    assert!(module_matches_pin(&pin, b"hardened plugin"));
    assert!(verify_plugin_pin(host.log(), Some(&pin)).is_ok());
}

#[test]
fn a_hashless_plugin_leaves_the_genesis_unpinned() {
    let host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(NativeEngine::new()),
        Some(plugin(None)),
    )
    .unwrap();
    assert_eq!(genesis_plugin_pin(host.log()), None);
    assert!(verify_plugin_pin(host.log(), None).is_ok());
}

#[test]
fn plugin_pin_verification_refuses_every_mismatch() {
    let hash = *blake3::hash(b"pinned plugin").as_bytes();
    let pinned = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(NativeEngine::new()),
        Some(plugin(Some(hash))),
    )
    .unwrap();
    let other = engine_blob_ref(*blake3::hash(b"other plugin").as_bytes());
    assert!(verify_plugin_pin(pinned.log(), None).is_err());
    assert!(verify_plugin_pin(pinned.log(), Some(&other)).is_err());
    let unpinned = HostSession::new("rae");
    assert!(verify_plugin_pin(unpinned.log(), Some(&other)).is_err());
    assert!(verify_plugin_pin(unpinned.log(), None).is_ok());
}

#[test]
fn a_plugin_that_refuses_the_move_leaks_no_face_into_the_log() {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(NativeEngine::new()),
        Some(Box::new(FakePlugin {
            hash: None,
            reject: |action| matches!(action, LogAction::Move { .. }),
        })),
    )
    .unwrap();
    host.deal(0, faces("secret", 2)).unwrap();
    let hand = area(&host.table(), 0, Zone::Hand);
    let before = host.log().len();
    let refused = host.intent(
        0,
        WireIntent::Move {
            card: hand[0],
            to: Zone::Board,
            seat: 0,
            index: 0,
        },
    );
    assert_eq!(refused, Err(SessionError::Refused(FoldError::rejected())));
    assert_eq!(host.log().len(), before);
    let log_bytes = encode_log(host.log());
    assert!(!log_bytes.windows(6).any(|w| w == b"secret"));
}

#[test]
fn a_plugin_that_refuses_the_reveal_leaves_the_card_in_hand() {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(NativeEngine::new()),
        Some(Box::new(FakePlugin {
            hash: None,
            reject: |action| matches!(action, LogAction::Reveal { .. }),
        })),
    )
    .unwrap();
    host.deal(0, faces("card", 2)).unwrap();
    let hand = area(&host.table(), 0, Zone::Hand);
    let outcome = host.intent(
        0,
        WireIntent::Move {
            card: hand[0],
            to: Zone::Board,
            seat: 0,
            index: 0,
        },
    );
    let Err(error) = outcome else {
        panic!("the refused reveal must surface");
    };
    assert!(error.applied().is_empty());
    assert!(!error.is_engine_fault());
    assert!(area(&host.table(), 0, Zone::Board).is_empty());
    assert_eq!(area(&host.table(), 0, Zone::Hand).len(), 2);
}

#[test]
fn wrong_bytes_do_not_match_a_module_pin() {
    let pin = engine_blob_ref(*blake3::hash(b"the real module").as_bytes());
    assert!(module_matches_pin(&pin, b"the real module"));
    assert!(!module_matches_pin(&pin, b"a tampered module"));
    assert!(!module_matches_pin("blob:zz", b"the real module"));
    assert!(!module_matches_pin("nonsense", b"the real module"));
    assert_eq!(
        pin_hash(&pin),
        Some(*blake3::hash(b"the real module").as_bytes())
    );
}

#[test]
fn a_draw_from_a_hidden_deck_owes_the_face_to_the_drawer() {
    let zones = vec![ZoneDecl {
        id: 0,
        name: "main-deck".into(),
        kind: ZoneKind::Deck,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::None,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 1,
        label: "Main Deck".into(),
    }];
    let mut host = HostSession::with_config(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones,
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
    );
    let (seat, _) = host.join("ada").unwrap();
    let (_, wire) = host
        .deal_to(seat, faces("deck", 2), Zone::Plugin(0))
        .unwrap();
    assert!(wire.is_empty());
    let mut client = ClientSession::from_welcome(seat, host.roster(), host.log().to_vec());
    let decked = area(&host.table(), seat, Zone::Plugin(0));
    let entries = host
        .intent(
            seat,
            WireIntent::Move {
                card: decked[0],
                to: Zone::Hand,
                seat,
                index: 0,
            },
        )
        .unwrap();
    let owed = host.private_faces(&entries);
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].0, seat);
    assert_eq!(owed[0].1 .0, decked[0]);
    assert_eq!(owed[0].1 .1.name, "deck 0");
    for entry in entries {
        assert!(client.apply(entry).unwrap());
    }
    client.add_faces(vec![owed[0].1.clone()]);
    assert_eq!(
        client.table().get(CardId(decked[0])).unwrap().face.name,
        "deck 0"
    );
    assert_eq!(host.view().cards.len(), client.view().cards.len());
    let hand = area(&host.table(), seat, Zone::Hand);
    let public = host
        .intent(
            seat,
            WireIntent::Move {
                card: hand[0],
                to: Zone::Board,
                seat,
                index: 0,
            },
        )
        .unwrap();
    assert!(host.private_faces(&public).is_empty());
}

fn synthetic_deck(prefix: &str) -> agni_riftbound::DeckFaces {
    let face = |name: String| CardFace::named(name);
    agni_riftbound::DeckFaces {
        legend: Some(face(format!("{prefix} legend"))),
        chosen_champion: Some(face(format!("{prefix} champion"))),
        main_deck: (0..8).map(|i| face(format!("{prefix} main {i}"))).collect(),
        runes: (0..4).map(|i| face(format!("{prefix} rune {i}"))).collect(),
        battlefields: (0..3)
            .map(|i| face(format!("{prefix} field {i}")))
            .collect(),
        sideboard: vec![face(format!("{prefix} spare"))],
    }
}

fn riftbound_host() -> HostSession {
    HostSession::with_config(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: agni_riftbound::zone_table(),
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
    )
}

fn zone_ids(table: &Table, seat: u8, zone: u16) -> Vec<u32> {
    area(table, seat, Zone::Plugin(zone))
}

#[test]
fn a_seated_deck_deal_replicates_with_faces_only_where_visibility_allows() {
    let mut host = riftbound_host();
    let (seat, _) = host.join("ada").unwrap();
    let mut client = ClientSession::from_welcome(seat, host.roster(), host.log().to_vec());
    let (entries, owner_faces) = host
        .deal_groups(seat, agni_riftbound::deal_plan(&synthetic_deck("ada")))
        .unwrap();
    for entry in &entries {
        assert!(client.apply(entry.clone()).unwrap());
    }
    client.add_faces(owner_faces.clone());
    let table = client.table();
    let legend = zone_ids(&table, seat, agni_riftbound::ZONE_LEGEND);
    assert_eq!(legend.len(), 1);
    assert_eq!(
        table.get(CardId(legend[0])).unwrap().face.name,
        "ada legend"
    );
    let champion = zone_ids(&table, seat, agni_riftbound::ZONE_CHAMPION);
    assert_eq!(champion.len(), 1);
    assert_eq!(
        table.get(CardId(champion[0])).unwrap().face.name,
        "ada champion"
    );
    for (zone, count) in [
        (agni_riftbound::ZONE_RUNE_DECK, 4),
        (agni_riftbound::ZONE_MAIN_DECK, 4),
    ] {
        let decked = zone_ids(&table, seat, zone);
        assert_eq!(decked.len(), count);
        for id in decked {
            assert!(table.get(CardId(id)).unwrap().face.is_hidden());
        }
    }
    let hand = zone_ids(&table, seat, agni_riftbound::ZONE_HAND);
    assert_eq!(hand.len(), agni_riftbound::OPENING_HAND_SIZE as usize);
    for id in &hand {
        assert!(table
            .get(CardId(*id))
            .unwrap()
            .face
            .name
            .starts_with("ada main"));
    }
    let host_table = host.table();
    for id in &hand {
        assert!(host_table.get(CardId(*id)).unwrap().face.is_hidden());
    }
    let drawn = owner_faces
        .iter()
        .filter(|(_, face)| face.name.starts_with("ada main"))
        .count();
    assert_eq!(drawn, agni_riftbound::OPENING_HAND_SIZE as usize);
    let spare = zone_ids(&table, seat, agni_riftbound::ZONE_SIDEBOARD);
    assert_eq!(spare.len(), 1);
    assert_eq!(owner_faces.len(), 5);
    assert!(owner_faces.iter().any(|(_, face)| face.name == "ada spare"));
    assert_eq!(table.get(CardId(spare[0])).unwrap().face.name, "ada spare");
    for slot in 0..3u16 {
        let field = zone_ids(&table, 0, agni_riftbound::ZONE_BATTLEFIELD_FIRST + slot);
        assert_eq!(field.len(), 1);
        assert!(table
            .get(CardId(field[0]))
            .unwrap()
            .face
            .name
            .starts_with("ada field"));
    }
    assert_eq!(encode_log(host.log()), encode_log(client.log()));
    let log_bytes = encode_log(host.log());
    let contains = |needle: &[u8]| log_bytes.windows(needle.len()).any(|w| w == needle);
    assert!(contains(b"ada legend"));
    assert!(contains(b"ada field"));
    assert!(!contains(b"ada main"));
    assert!(!contains(b"ada rune"));
    assert!(!contains(b"ada spare"));
}

#[test]
fn the_dealers_shuffle_is_deterministic_and_seeded() {
    let run = || {
        let mut host = riftbound_host();
        let (seat, _) = host.join("ada").unwrap();
        host.deal_groups(seat, agni_riftbound::deal_plan(&synthetic_deck("ada")))
            .unwrap();
        encode_log(host.log())
    };
    assert_eq!(run(), run());
    let mut host = riftbound_host();
    let (seat, _) = host.join("ada").unwrap();
    let ordered: Vec<String> = synthetic_deck("ada")
        .main_deck
        .iter()
        .map(|face| face.name.clone())
        .collect();
    host.deal_groups(seat, agni_riftbound::deal_plan(&synthetic_deck("ada")))
        .unwrap();
    let decked: Vec<String> = host
        .state()
        .table
        .in_area(PlayerId(seat), Zone::Plugin(agni_riftbound::ZONE_MAIN_DECK))
        .map(|card| host.face_of(card.id.0).unwrap().1.name)
        .collect();
    let drawn: Vec<String> = host
        .state()
        .table
        .in_area(PlayerId(seat), Zone::Plugin(agni_riftbound::ZONE_HAND))
        .map(|card| host.face_of(card.id.0).unwrap().1.name)
        .collect();
    assert_eq!(decked.len(), 4);
    assert_eq!(drawn.len(), agni_riftbound::OPENING_HAND_SIZE as usize);
    let dealt: Vec<String> = decked
        .iter()
        .cloned()
        .chain(drawn.iter().rev().cloned())
        .collect();
    assert_eq!(
        dealt
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        ordered
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
    );
    assert_ne!(dealt, ordered);
}

#[test]
fn a_second_deck_balances_the_shared_battlefields() {
    let mut host = riftbound_host();
    let (seat, _) = host.join("ada").unwrap();
    host.deal_groups(0, agni_riftbound::deal_plan(&synthetic_deck("rae")))
        .unwrap();
    host.deal_groups(seat, agni_riftbound::deal_plan(&synthetic_deck("ada")))
        .unwrap();
    let table = host.table();
    for slot in 0..3u16 {
        let field = zone_ids(&table, 0, agni_riftbound::ZONE_BATTLEFIELD_FIRST + slot);
        assert_eq!(field.len(), 2);
    }
}

#[test]
fn a_deal_against_missing_zones_refuses_without_mutating() {
    let mut host = HostSession::new("rae");
    let before = host.log().len();
    assert_eq!(
        host.deal_groups(0, agni_riftbound::deal_plan(&synthetic_deck("rae"))),
        Err(SessionError::NoDealTarget)
    );
    let groups = vec![DealGroup {
        target: DealTarget::Zone("nowhere".into()),
        faces: vec![faces("x", 1)[0].clone()],
        shuffle: false,
        draw: 0,
    }];
    let mut zoned = riftbound_host();
    let zoned_before = zoned.log().len();
    assert_eq!(
        zoned.deal_groups(0, groups),
        Err(SessionError::NoDealTarget)
    );
    assert_eq!(
        zoned.deal_groups(9, Vec::new()),
        Err(SessionError::UnknownSeat(9))
    );
    assert_eq!(host.log().len(), before);
    assert_eq!(zoned.log().len(), zoned_before);
}

#[test]
fn hosting_from_a_solo_table_reproduces_it_through_the_log() {
    let mut solo = Table::new();
    for face in faces("solo", 4) {
        solo.add_face(PlayerId(0), Zone::Hand, face);
    }
    let played = solo.cards()[1].id;
    solo.apply(Intent::MoveCard {
        card: played,
        to: Zone::Board,
        seat: PlayerId(1),
        index: 0,
    });
    let host = HostSession::host_from("rae", &solo);
    assert_eq!(host.table(), solo);
    let replica = fold(host.log());
    assert_eq!(area(&replica.table, 1, Zone::Board), vec![played.0]);
    assert_eq!(replica.table.get(played).unwrap().face.name, "solo 1");
    assert!(replica
        .table
        .in_area(PlayerId(0), Zone::Hand)
        .all(|card| card.face.is_hidden()));
}

#[test]
fn hosting_a_zone_table_ignores_the_solo_hand_and_starts_empty() {
    let mut solo = Table::new();
    for face in faces("Lightning Bolt", 7) {
        solo.add_face(PlayerId(0), Zone::Hand, face);
    }
    solo.apply(Intent::MoveCard {
        card: solo.cards()[0].id,
        to: Zone::Board,
        seat: PlayerId(0),
        index: 0,
    });
    let host = HostSession::host_from_with(
        "rae",
        &solo,
        TableConfig {
            engine: None,
            plugin: None,
            zones: agni_riftbound::zone_table(),
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
        Box::new(NativeEngine::new()),
        None,
    )
    .unwrap();
    assert!(host.table().is_empty());
    assert_eq!(host.log().len(), 1);
    assert!(matches!(host.log()[0].action, LogAction::Genesis { .. }));
    let log_bytes = encode_log(host.log());
    let leaked = |needle: &[u8]| log_bytes.windows(needle.len()).any(|w| w == needle);
    assert!(!leaked(b"Lightning Bolt"));
    let freeform = HostSession::host_from_with(
        "rae",
        &solo,
        TableConfig::default(),
        Box::new(NativeEngine::new()),
        None,
    )
    .unwrap();
    assert_eq!(freeform.table(), solo);
}

#[test]
fn a_reload_sweeps_that_seat_alone_and_deals_the_edited_list() {
    let mut host = riftbound_host();
    let (ada, _) = host.join("ada").unwrap();
    host.deal_groups(0, agni_riftbound::deal_plan(&synthetic_deck("rae")))
        .unwrap();
    host.deal_groups(ada, agni_riftbound::deal_plan(&synthetic_deck("ada")))
        .unwrap();
    let rae_before = zone_ids(&host.table(), 0, agni_riftbound::ZONE_MAIN_DECK);
    let mut edited = synthetic_deck("ada");
    edited.main_deck.truncate(5);
    let (entries, owner_faces) = host
        .reload_groups(ada, agni_riftbound::deal_plan(&edited))
        .unwrap();
    assert!(matches!(
        entries.first().map(|entry| &entry.action),
        Some(LogAction::Clear { seat }) if *seat == ada
    ));
    let table = host.table();
    assert_eq!(
        zone_ids(&table, 0, agni_riftbound::ZONE_MAIN_DECK),
        rae_before
    );
    assert_eq!(
        zone_ids(&table, ada, agni_riftbound::ZONE_MAIN_DECK).len(),
        5 - agni_riftbound::OPENING_HAND_SIZE as usize
    );
    assert_eq!(zone_ids(&table, ada, agni_riftbound::ZONE_LEGEND).len(), 1);
    for slot in 0..3u16 {
        let field = zone_ids(&table, 0, agni_riftbound::ZONE_BATTLEFIELD_FIRST + slot);
        assert_eq!(field.len(), 2);
    }
    assert!(owner_faces
        .iter()
        .any(|(_, face)| face.name.starts_with("ada main")));
    let replayed = fold(host.log());
    assert_eq!(replayed.table.len(), host.state().table.len());
    assert_eq!(
        host.reload_groups(ada, Vec::new()),
        Err(SessionError::NoDealTarget)
    );
    assert_eq!(
        host.reload_groups(9, agni_riftbound::deal_plan(&edited)),
        Err(SessionError::UnknownSeat(9))
    );
}

#[test]
fn a_reload_works_before_anything_was_dealt() {
    let mut host = riftbound_host();
    let (ada, _) = host.join("ada").unwrap();
    assert_eq!(host.clear_seat(ada), Ok(None));
    let (entries, _) = host
        .reload_groups(ada, agni_riftbound::deal_plan(&synthetic_deck("ada")))
        .unwrap();
    assert!(entries
        .iter()
        .all(|entry| !matches!(entry.action, LogAction::Clear { .. })));
    assert_eq!(
        zone_ids(&host.table(), ada, agni_riftbound::ZONE_LEGEND).len(),
        1
    );
}

#[test]
fn seat_colours_are_unique_and_claimed_first_come_first_served() {
    let mut host = riftbound_host();
    assert_eq!(host.roster()[0].color, default_seat_color(0));
    let (ada, _) = host.join("ada").unwrap();
    let (kai, _) = host.join("kai").unwrap();
    let claimed: Vec<u8> = host.roster().iter().map(|info| info.color).collect();
    assert_eq!(claimed, vec![0, 1, 2]);
    assert!(!host.pick_color(ada, 0));
    assert!(!host.pick_color(ada, SEAT_PICKABLE_COLORS));
    assert!(!host.pick_color(9, 3));
    assert!(host.pick_color(ada, 3));
    assert_eq!(roster_color(&host.roster(), ada), 3);
    assert!(host.pick_color(ada, 3));
    assert!(host.pick_color(kai, 1));
    assert_eq!(roster_color(&host.roster(), kai), 1);
    assert_eq!(roster_color(&host.roster(), 9), default_seat_color(9));
    let info: Vec<SeatInfo> = host.roster();
    assert!(info.iter().all(|seat| seat.color < SEAT_PICKABLE_COLORS));
}

struct Counting {
    inner: NativeEngine,
    snapshots: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl agni_sim::engine::Engine for Counting {
    fn fold_entry(
        &mut self,
        entry: &agni_sim::log::LogEntry,
        verdict: Option<Verdict>,
        mode: agni_sim::engine::FoldMode,
        viewer: u8,
    ) -> Result<agni_sim::engine::FoldOutcome, EngineFault> {
        self.inner.fold_entry(entry, verdict, mode, viewer)
    }

    fn fold_log(
        &mut self,
        entries: &[agni_sim::log::LogEntry],
        viewer: u8,
    ) -> Result<agni_sim::engine::FoldLogOutcome, EngineFault> {
        self.inner.fold_log(entries, viewer)
    }

    fn decide_request(
        &mut self,
        entry: &agni_sim::log::LogEntry,
    ) -> Result<Option<Vec<u8>>, EngineFault> {
        self.inner.decide_request(entry)
    }

    fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault> {
        self.snapshots
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.snapshot()
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault> {
        self.inner.restore(bytes)
    }

    fn view(&mut self, viewer: u8) -> Result<agni_sim::view::TableView, EngineFault> {
        self.inner.view(viewer)
    }

    fn engine_hash(&self) -> Option<[u8; 32]> {
        None
    }
}

#[test]
fn sessions_fold_a_native_shadow_instead_of_snapshotting_the_engine_per_action() {
    let snapshots = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let engine = Box::new(Counting {
        inner: NativeEngine::new(),
        snapshots: snapshots.clone(),
    });
    let mut host = HostSession::with_engine("rae", TableConfig::default(), engine, None).unwrap();
    host.deal(0, faces("host", 3)).unwrap();
    let (ada, _) = host.join("ada").unwrap();
    host.deal(ada, faces("ada", 2)).unwrap();
    let card = host.state().table.cards()[0].id.0;
    host.intent(
        0,
        WireIntent::Move {
            card,
            to: Zone::Board,
            seat: 0,
            index: 0,
        },
    )
    .unwrap();
    assert_eq!(host.state().next_seq, host.log().len() as u64);
    assert_eq!(host.state(), &fold(host.log()));
    let replica = ClientSession::from_welcome_with(
        ada,
        host.roster(),
        host.log().to_vec(),
        Box::new(Counting {
            inner: NativeEngine::new(),
            snapshots: snapshots.clone(),
        }),
        None,
    )
    .unwrap();
    assert_eq!(replica.state(), host.state());
    assert_eq!(snapshots.load(std::sync::atomic::Ordering::SeqCst), 0);
}

struct Disagreeing(NativeEngine);

impl agni_sim::engine::Engine for Disagreeing {
    fn fold_entry(
        &mut self,
        entry: &agni_sim::log::LogEntry,
        verdict: Option<Verdict>,
        mode: agni_sim::engine::FoldMode,
        viewer: u8,
    ) -> Result<agni_sim::engine::FoldOutcome, EngineFault> {
        let mut outcome = self.0.fold_entry(entry, verdict, mode, viewer)?;
        if matches!(entry.action, LogAction::Move { .. }) {
            outcome.result = Err(FoldError::rejected());
        }
        Ok(outcome)
    }

    fn fold_log(
        &mut self,
        entries: &[agni_sim::log::LogEntry],
        viewer: u8,
    ) -> Result<agni_sim::engine::FoldLogOutcome, EngineFault> {
        self.0.fold_log(entries, viewer)
    }

    fn decide_request(
        &mut self,
        entry: &agni_sim::log::LogEntry,
    ) -> Result<Option<Vec<u8>>, EngineFault> {
        self.0.decide_request(entry)
    }

    fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault> {
        self.0.snapshot()
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault> {
        self.0.restore(bytes)
    }

    fn view(&mut self, viewer: u8) -> Result<agni_sim::view::TableView, EngineFault> {
        self.0.view(viewer)
    }

    fn engine_hash(&self) -> Option<[u8; 32]> {
        None
    }
}

#[test]
fn an_engine_that_disagrees_with_this_builds_fold_is_reported_as_a_fault_not_absorbed() {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(Disagreeing(NativeEngine::new())),
        None,
    )
    .unwrap();
    host.deal(0, faces("host", 1)).unwrap();
    let card = host.state().table.cards()[0].id.0;
    let error = host
        .intent(
            0,
            WireIntent::Move {
                card,
                to: Zone::Board,
                seat: 0,
                index: 0,
            },
        )
        .unwrap_err();
    assert!(error.is_engine_fault(), "{error:?}");
}

struct InformationPlugin(Vec<agni_sim::log::Effect>);

impl PluginModule for InformationPlugin {
    fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault> {
        let request: agni_sim::abi::DecideRequest =
            agni_sim::abi::decode(request).ok_or_else(|| EngineFault("bad request".into()))?;
        Ok(match request.entry.action {
            LogAction::Game { .. } => Verdict::accept().with_effects(self.0.clone()),
            _ => Verdict::accept(),
        })
    }
}

fn informed_table(effects: Vec<agni_sim::log::Effect>) -> (HostSession, u8) {
    let hand = ZoneDecl {
        id: 0,
        name: "hand".into(),
        kind: ZoneKind::Hand,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::Owner,
        place: ZonePlace::Fan,
        layout: ZoneLayout::Fan,
        span: 1,
        label: "hand".into(),
    };
    let field = ZoneDecl {
        id: 2,
        name: "field".into(),
        kind: ZoneKind::Battlefield,
        owner: ZoneOwner::Shared,
        visibility: ZoneVisibility::All,
        place: ZonePlace::Center,
        layout: ZoneLayout::Row,
        span: 1,
        label: "field".into(),
    };
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: vec![hand, library_decl(), field],
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
        Box::new(NativeEngine::new()),
        Some(Box::new(InformationPlugin(effects))),
    )
    .unwrap();
    let (ada, _) = host.join("ada").unwrap();
    (host, ada)
}

fn joined_client(
    host: &HostSession,
    seat: u8,
    effects: Vec<agni_sim::log::Effect>,
) -> ClientSession {
    let mut client = ClientSession::from_welcome_with(
        seat,
        host.roster(),
        host.log().to_vec(),
        Box::new(NativeEngine::new()),
        Some(Box::new(InformationPlugin(effects))),
    )
    .unwrap();
    client.add_faces(host.faces_owed_to(seat));
    client
}

fn nudge(host: &mut HostSession, from: u8) -> Vec<agni_sim::log::LogEntry> {
    host.intent(
        from,
        WireIntent::Game {
            data: ByteBuf::from(vec![1]),
        },
    )
    .unwrap()
}

#[test]
fn a_reveal_effect_publishes_a_hand_card_to_every_seat() {
    let (mut host, ada) = informed_table(Vec::new());
    host.deal_to(ada, faces("ada", 2), Zone::Plugin(0)).unwrap();
    let hers = area(&host.table(), ada, Zone::Plugin(0));
    let effects = vec![agni_sim::log::Effect::Reveal { card: hers[0] }];
    let (mut host, ada) = {
        let (mut fresh, seat) = informed_table(effects.clone());
        fresh
            .deal_to(seat, faces("ada", 2), Zone::Plugin(0))
            .unwrap();
        (fresh, seat)
    };
    let mut client = joined_client(&host, ada, effects.clone());
    let mut watcher = joined_client(&host, 0, effects);
    assert!(host.table().get(CardId(hers[0])).unwrap().face.is_hidden());
    assert_eq!(
        client.table().get(CardId(hers[0])).unwrap().face.name,
        "ada 0"
    );
    let entries = nudge(&mut host, 0);
    assert_eq!(entries.len(), 2, "the game entry and the reveal it owed");
    let LogAction::Reveal { card, face } = &entries[1].action else {
        panic!("the host pays the reveal debt at once: {:?}", entries[1]);
    };
    assert_eq!((*card, face.name.as_str()), (hers[0], "ada 0"));
    assert_eq!(entries[1].seat, ada, "a hand card is revealed by its owner");
    assert!(host.state().owed_reveals.is_empty());
    assert!(host.state().revealed.contains(&hers[0]));
    assert!(!host.state().revealed.contains(&hers[1]));
    assert_eq!(
        host.table().get(CardId(hers[0])).unwrap().face.name,
        "ada 0"
    );
    assert_eq!(
        host.table_for(ada).get(CardId(hers[0])).unwrap().face.name,
        "ada 0"
    );
    for replica in [&mut client, &mut watcher] {
        for entry in &entries {
            assert!(replica.apply(entry.clone()).unwrap());
        }
        assert_eq!(
            replica.table().get(CardId(hers[0])).unwrap().face.name,
            "ada 0"
        );
        assert!(replica.state().owed_reveals.is_empty());
    }
    assert!(watcher
        .table()
        .get(CardId(hers[1]))
        .unwrap()
        .face
        .is_hidden());
    assert!(
        host.owed_faces()
            .iter()
            .all(|(seat, (card, _))| *seat == ada && *card == hers[1]),
        "a public face is owed to nobody in private"
    );
}

#[test]
fn a_peek_effect_hands_a_deck_face_to_one_seat_only() {
    let (mut host, ada) = informed_table(Vec::new());
    host.deal_to(ada, faces("deck", 2), Zone::Plugin(1))
        .unwrap();
    let decked = area(&host.table(), ada, Zone::Plugin(1));
    let effects = vec![agni_sim::log::Effect::Peek {
        card: decked[1],
        seat: 0,
    }];
    let (mut host, ada) = {
        let (mut fresh, seat) = informed_table(effects.clone());
        fresh
            .deal_to(seat, faces("deck", 2), Zone::Plugin(1))
            .unwrap();
        (fresh, seat)
    };
    let mut client = joined_client(&host, ada, effects.clone());
    let mut watcher = joined_client(&host, 0, effects.clone());
    assert!(host.faces_owed_to(0).is_empty());
    assert!(host.faces_owed_to(ada).is_empty());
    let entries = nudge(&mut host, ada);
    assert_eq!(entries.len(), 1, "a peek appends nothing to the public log");
    assert_eq!(
        host.state().peeks,
        std::collections::BTreeSet::from([(decked[1], 0)])
    );
    assert!(!host.state().revealed.contains(&decked[1]));
    let owed = host.owed_faces();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].0, 0);
    assert_eq!(owed[0].1 .0, decked[1]);
    assert_eq!(owed[0].1 .1.name, "deck 1");
    assert!(host.owed_faces().is_empty(), "a peeked face is sent once");
    assert_eq!(host.faces_owed_to(0), vec![owed[0].1.clone()]);
    assert!(host.faces_owed_to(ada).is_empty());
    assert_eq!(
        host.table_for(0).get(CardId(decked[1])).unwrap().face.name,
        "deck 1"
    );
    assert!(host
        .table_for(ada)
        .get(CardId(decked[1]))
        .unwrap()
        .face
        .is_hidden());
    assert!(host
        .table_for(0)
        .get(CardId(decked[0]))
        .unwrap()
        .face
        .is_hidden());
    for replica in [&mut client, &mut watcher] {
        for entry in &entries {
            assert!(replica.apply(entry.clone()).unwrap());
        }
        assert_eq!(replica.state().peeks, host.state().peeks);
    }
    assert_eq!(watcher.view().peeked, vec![decked[1]]);
    assert!(client.view().peeked.is_empty());
    assert_eq!(host.view().peeked, vec![decked[1]]);
    watcher.add_faces(vec![owed[0].1.clone()]);
    assert_eq!(
        watcher.table().get(CardId(decked[1])).unwrap().face.name,
        "deck 1"
    );
    assert!(watcher
        .table()
        .get(CardId(decked[0]))
        .unwrap()
        .face
        .is_hidden());
    assert!(
        client
            .table()
            .get(CardId(decked[1]))
            .unwrap()
            .face
            .is_hidden(),
        "the deck's owner was not the peeking seat and learns nothing"
    );
    let private = host.private_faces(&[]);
    assert_eq!(private, vec![(0, owed[0].1.clone())]);
    let returning = joined_client(&host, 0, effects);
    assert_eq!(
        returning.table().get(CardId(decked[1])).unwrap().face.name,
        "deck 1"
    );
}

#[test]
fn a_reveal_effect_publishes_a_facedown_card_the_host_was_keeping_secret() {
    let (mut host, ada) = informed_table(Vec::new());
    host.deal_to(ada, faces("ada", 1), Zone::Plugin(0)).unwrap();
    let hers = area(&host.table(), ada, Zone::Plugin(0));
    let effects = vec![agni_sim::log::Effect::Reveal { card: hers[0] }];
    let (mut host, ada) = {
        let (mut fresh, seat) = informed_table(effects.clone());
        fresh
            .deal_to(seat, faces("ada", 1), Zone::Plugin(0))
            .unwrap();
        (fresh, seat)
    };
    let mut watcher = joined_client(&host, 0, effects);
    let hidden = host
        .intent(
            ada,
            WireIntent::MoveHidden {
                card: hers[0],
                to: Zone::Plugin(2),
                seat: ada,
                index: 0,
            },
        )
        .unwrap();
    assert_eq!(hidden.len(), 1, "a hide publishes no face");
    assert_eq!(host.hidden_plays(), vec![hers[0]]);
    for entry in &hidden {
        assert!(watcher.apply(entry.clone()).unwrap());
    }
    assert!(watcher
        .table()
        .get(CardId(hers[0]))
        .unwrap()
        .face
        .is_hidden());
    assert!(host.table().get(CardId(hers[0])).unwrap().face.is_hidden());
    let entries = nudge(&mut host, 0);
    assert_eq!(entries.len(), 2);
    let LogAction::Reveal { card, face } = &entries[1].action else {
        panic!("the facedown card is revealed at once: {:?}", entries[1]);
    };
    assert_eq!((*card, face.name.as_str()), (hers[0], "ada 0"));
    assert_eq!(entries[1].seat, ada, "revealed by the seat that hid it");
    assert!(host.hidden_plays().is_empty());
    assert!(host.state().owed_reveals.is_empty());
    assert_eq!(
        host.table().get(CardId(hers[0])).unwrap().face.name,
        "ada 0"
    );
    for entry in &entries {
        assert!(watcher.apply(entry.clone()).unwrap());
    }
    assert_eq!(
        watcher.table().get(CardId(hers[0])).unwrap().face.name,
        "ada 0"
    );
}
