use agni_plugin_sdk::blob::MapWriter;
use agni_plugin_sdk::cbor::Writer;
use agni_riftbound_turns::state::{
    GameBlob, ItemStatus, Limited, Needs, Price, TargetRef, FLAG_DEFENDER, FLAG_ENTERED_THIS_TURN,
    FLAG_FROM_FACEDOWN, FLAG_REVEALING, FLAG_SHROUDED, FLAG_STUNNED, SLOT_PROMISED_REPEAT,
};

fn seat_v7_old_lock(writer: &mut Writer) {
    writer.array(8);
    writer.unsigned(2);
    writer.unsigned(3);
    writer.bool(true);
    writer.bool(true);
    writer.unsigned(4);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.array(2);
    writer.unsigned(7);
    writer.unsigned(8);
}

fn seat_v7_champion(writer: &mut Writer) {
    writer.array(9);
    writer.unsigned(2);
    writer.unsigned(3);
    writer.bool(true);
    writer.unsigned(7);
    writer.unsigned(4);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.array(2);
    writer.unsigned(7);
    writer.unsigned(8);
    writer.text("Riven");
}

fn seat_v9(writer: &mut Writer) {
    writer.array(10);
    writer.unsigned(1);
    writer.unsigned(2);
    writer.bool(false);
    writer.unsigned(4);
    writer.unsigned(3);
    writer.unsigned(4);
    writer.unsigned(5);
    writer.array(2);
    writer.unsigned(6);
    writer.unsigned(7);
    writer.bool(true);
    writer.bool(false);
}

fn seat_v10(writer: &mut Writer) {
    writer.array(11);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.bool(true);
    writer.unsigned(7);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.unsigned(7);
    writer.array(2);
    writer.unsigned(8);
    writer.unsigned(9);
    writer.bool(false);
    writer.bool(true);
    writer.text("Aurora");
}

fn seat_v10_zero_discount(writer: &mut Writer) {
    writer.array(11);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.bool(true);
    writer.unsigned(7);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.unsigned(7);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.bool(false);
    writer.bool(true);
    writer.text("Aurora");
}

fn seat_v10_economy(writer: &mut Writer) {
    writer.array(11);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.bool(true);
    writer.bool(true);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.unsigned(7);
    writer.unsigned(8);
    writer.unsigned(9);
    writer.array(1);
    writer.array(3);
    writer.unsigned(1);
    writer.unsigned(1);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(2);
    writer.array(1);
    writer.unsigned(0);
}

fn expiry_end_of_turn(writer: &mut Writer, turn: u16) {
    writer.array(2);
    writer.unsigned(1);
    writer.unsigned(u64::from(turn));
}

fn expiry_while_attached(writer: &mut Writer, card: u32) {
    writer.array(2);
    writer.unsigned(3);
    writer.unsigned(u64::from(card));
}

fn might(writer: &mut Writer, delta: i64, expiry: impl FnOnce(&mut Writer), source: u16) {
    writer.array(3);
    writer.signed(delta);
    expiry(writer);
    writer.unsigned(u64::from(source));
}

fn ordinary(writer: &mut Writer, code: u8, arg: u8, expiry: impl FnOnce(&mut Writer)) {
    writer.array(3);
    writer.unsigned(u64::from(code));
    writer.unsigned(u64::from(arg));
    expiry(writer);
}

fn costed(
    writer: &mut Writer,
    code: u8,
    energy: u8,
    powers: &[u8],
    expiry: impl FnOnce(&mut Writer),
) {
    writer.array(4);
    writer.unsigned(u64::from(code));
    writer.unsigned(u64::from(energy));
    writer.array(powers.len());
    for power in powers {
        writer.unsigned(u64::from(*power));
    }
    expiry(writer);
}

fn card_v7(writer: &mut Writer) {
    writer.array(12);
    writer.unsigned(101);
    writer.unsigned(u64::from(FLAG_STUNNED | FLAG_DEFENDER));
    writer.array(1);
    might(writer, 3, |writer| expiry_end_of_turn(writer, 22), 77);
    writer.array(1);
    ordinary(writer, 9, 0, |writer| writer.unsigned(0));
    writer.unsigned(7);
    writer.unsigned(4);
    writer.unsigned(12);
    writer.unsigned(13);
    writer.unsigned(14);
    writer.unsigned(1);
    writer.unsigned(8);
    writer.text("Legacy name");
}

fn card_v9(writer: &mut Writer) {
    writer.array(12);
    writer.unsigned(202);
    writer.unsigned(u64::from(FLAG_FROM_FACEDOWN | FLAG_SHROUDED));
    writer.array(0);
    writer.array(1);
    ordinary(writer, 1, 0, |writer| writer.unsigned(0));
    writer.array(1);
    costed(writer, 14, 3, &[7, 2, 6], |writer| {
        expiry_end_of_turn(writer, 22)
    });
    writer.null();
    writer.unsigned(6);
    writer.unsigned(20);
    writer.unsigned(21);
    writer.unsigned(22);
    writer.unsigned(2);
    writer.unsigned(9);
}

fn card_v10(writer: &mut Writer) {
    writer.array(13);
    writer.unsigned(303);
    writer.unsigned(u64::from(FLAG_ENTERED_THIS_TURN | FLAG_REVEALING));
    writer.array(1);
    might(writer, -2, |writer| expiry_while_attached(writer, 404), 55);
    writer.array(1);
    ordinary(writer, 4, 2, |writer| writer.unsigned(2));
    writer.array(1);
    costed(writer, 20, 1, &[6, 7], |writer| writer.unsigned(0));
    writer.unsigned(202);
    writer.null();
    writer.unsigned(31);
    writer.unsigned(32);
    writer.unsigned(33);
    writer.unsigned(0);
    writer.unsigned(303);
    writer.text("Chosen title");
}

fn legacy(version: u64, seat: impl FnOnce(&mut Writer), card: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut map = MapWriter::new();
    map.field("v").unsigned(version);
    let seats = map.field("s");
    seats.array(1);
    seat(seats);
    let cards = map.field("c");
    cards.array(1);
    card(cards);
    map.field("wn").unsigned(1);
    let deaths = map.field("dt");
    deaths.array(1);
    deaths.array(4);
    deaths.unsigned(303);
    deaths.unsigned(1);
    deaths.bool(true);
    deaths.unsigned(5);
    map.finish()
}

fn canonical_v12(seat: impl FnOnce(&mut Writer), card: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut map = MapWriter::new();
    map.field("v").unsigned(14);
    let seats = map.field("s");
    seats.array(1);
    seat(seats);
    let cards = map.field("c");
    cards.array(1);
    card(cards);
    map.field("wn").unsigned(1);
    let deaths = map.field("dt");
    deaths.array(1);
    deaths.array(4);
    deaths.unsigned(303);
    deaths.unsigned(1);
    deaths.bool(true);
    deaths.unsigned(5);
    map.finish()
}

fn promise_discount(writer: &mut Writer, energy: u8, rainbow: usize) {
    writer.array(3);
    writer.unsigned(3);
    writer.array(2);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(u64::from(energy));
    writer.array(rainbow);
    for _ in 0..rainbow {
        writer.unsigned(0);
    }
    writer.unsigned(0);
}

fn canonical_seat_v7_old_lock(writer: &mut Writer) {
    writer.array(16);
    writer.unsigned(2);
    writer.unsigned(3);
    writer.bool(true);
    writer.unsigned(1);
    writer.unsigned(4);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.array(1);
    promise_discount(writer, 7, 8);
    writer.bool(false);
    writer.bool(false);
    writer.null();
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.array(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
}

fn canonical_seat_v7_champion(writer: &mut Writer) {
    writer.array(16);
    writer.unsigned(2);
    writer.unsigned(3);
    writer.bool(true);
    writer.unsigned(7);
    writer.unsigned(4);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.array(1);
    promise_discount(writer, 7, 8);
    writer.bool(false);
    writer.bool(false);
    writer.text("Riven");
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.array(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
}

fn canonical_seat_v9(writer: &mut Writer) {
    writer.array(16);
    writer.unsigned(1);
    writer.unsigned(2);
    writer.bool(false);
    writer.unsigned(4);
    writer.unsigned(3);
    writer.unsigned(4);
    writer.unsigned(5);
    writer.array(1);
    promise_discount(writer, 6, 7);
    writer.bool(true);
    writer.bool(false);
    writer.null();
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.array(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
}

fn canonical_seat_v10(writer: &mut Writer) {
    writer.array(16);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.bool(true);
    writer.unsigned(7);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.unsigned(7);
    writer.array(1);
    promise_discount(writer, 8, 9);
    writer.bool(false);
    writer.bool(true);
    writer.text("Aurora");
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.array(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
}

fn canonical_seat_v10_zero_discount(writer: &mut Writer) {
    writer.array(16);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.bool(true);
    writer.unsigned(7);
    writer.unsigned(5);
    writer.unsigned(6);
    writer.unsigned(7);
    writer.array(0);
    writer.bool(false);
    writer.bool(true);
    writer.text("Aurora");
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.array(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
}

fn canonical_chain_item_v12(writer: &mut Writer, picks: &[u8]) {
    writer.array(14);
    writer.unsigned(41);
    writer.array(3);
    writer.unsigned(0);
    writer.unsigned(77);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(0);
    writer.array(0);
    writer.array(picks.len());
    for pick in picks {
        writer.unsigned(u64::from(*pick));
    }
    writer.unsigned(0);
    writer.null();
    writer.null();
    writer.unsigned(0);
    writer.array(0);
    writer.null();
}

fn old_chain_item(writer: &mut Writer, picks: &[u8]) {
    writer.array(13);
    writer.unsigned(41);
    writer.array(3);
    writer.unsigned(0);
    writer.unsigned(77);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(0);
    writer.array(0);
    writer.array(picks.len());
    for pick in picks {
        writer.unsigned(u64::from(*pick));
    }
    writer.unsigned(0);
    writer.null();
    writer.null();
    writer.unsigned(0);
    writer.array(0);
}

fn old_pending_item(writer: &mut Writer, picks: &[u8]) {
    writer.array(2);
    old_chain_item(writer, picks);
    writer.unsigned(1);
}

fn writer_pending_v12(writer: &mut Writer, picks: &[u8]) {
    writer.array(2);
    canonical_chain_item_v12(writer, picks);
    writer.unsigned(1);
}

fn legacy_modes() -> Vec<u8> {
    let mut map = MapWriter::new();
    map.field("v").unsigned(10);
    let chain = map.field("ch");
    chain.array(1);
    old_chain_item(chain, &[255, 255, 255, 255, 255, 255, 3, 4]);
    let queue = map.field("q");
    queue.array(1);
    old_pending_item(queue, &[255, 255, 255, 255, 255, 255, 3, 4]);
    map.finish()
}

fn canonical_modes() -> Vec<u8> {
    let mut map = MapWriter::new();
    map.field("v").unsigned(14);
    let chain = map.field("ch");
    chain.array(1);
    canonical_chain_item_v12(chain, &[255, 255, 255, 255, 255, 255, 255, 3, 4]);
    let queue = map.field("q");
    queue.array(1);
    writer_pending_v12(queue, &[255, 255, 255, 255, 255, 255, 255, 3, 4]);
    map.finish()
}

fn v11_here_to_help_item(writer: &mut Writer) {
    writer.array(13);
    writer.unsigned(501);
    writer.array(3);
    writer.unsigned(0);
    writer.unsigned(900);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.unsigned(2);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(801);
    writer.array(2);
    writer.unsigned(2);
    writer.unsigned(12);
    writer.array(2);
    writer.unsigned(1);
    writer.unsigned(1);
    writer.array(8);
    writer.unsigned(1);
    writer.unsigned(0);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.unsigned(3);
    writer.null();
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(802);
    writer.unsigned(1);
    writer.array(2);
    writer.unsigned(701);
    writer.unsigned(702);
}

fn v11_swarm_queen_item(writer: &mut Writer) {
    writer.array(13);
    writer.unsigned(502);
    writer.array(3);
    writer.unsigned(3);
    writer.unsigned(901);
    writer.unsigned(0);
    writer.unsigned(1);
    writer.unsigned(2);
    writer.array(2);
    writer.unsigned(3);
    writer.unsigned(0);
    writer.array(2);
    writer.array(2);
    writer.unsigned(2);
    writer.unsigned(12);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(803);
    writer.array(1);
    writer.unsigned(1);
    writer.array(8);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(1);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(6);
    writer.unsigned(4);
    writer.null();
    writer.array(2);
    writer.unsigned(2);
    writer.unsigned(12);
    writer.unsigned(2);
    writer.array(1);
    writer.unsigned(703);
}

fn v12_here_to_help_item(writer: &mut Writer) {
    write_here_to_help_item(writer, 14, false);
}

fn v12_limited_item(writer: &mut Writer) {
    write_here_to_help_item(writer, 14, true);
}

fn write_here_to_help_item(writer: &mut Writer, length: usize, limited: bool) {
    writer.array(length);
    writer.unsigned(501);
    writer.array(3);
    writer.unsigned(0);
    writer.unsigned(900);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.unsigned(2);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(0);
    writer.array(2);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(801);
    writer.array(2);
    writer.unsigned(2);
    writer.unsigned(12);
    writer.array(2);
    writer.unsigned(1);
    writer.unsigned(1);
    writer.array(8);
    writer.unsigned(1);
    writer.unsigned(0);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(2);
    writer.unsigned(4);
    writer.unsigned(3);
    writer.null();
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(802);
    writer.unsigned(1);
    writer.array(2);
    writer.unsigned(701);
    writer.unsigned(702);
    if length == 14 {
        if limited {
            writer.array(2);
            writer.array(2);
            writer.unsigned(12);
            writer.unsigned(13);
            writer.array(2);
            writer.unsigned(2);
            writer.unsigned(3);
        } else {
            writer.null();
        }
    }
}

fn v12_swarm_queen_item(writer: &mut Writer) {
    writer.array(14);
    writer.unsigned(502);
    writer.array(3);
    writer.unsigned(3);
    writer.unsigned(901);
    writer.unsigned(0);
    writer.unsigned(1);
    writer.unsigned(2);
    writer.array(2);
    writer.unsigned(3);
    writer.unsigned(0);
    writer.array(2);
    writer.array(2);
    writer.unsigned(2);
    writer.unsigned(12);
    writer.array(2);
    writer.unsigned(0);
    writer.unsigned(803);
    writer.array(1);
    writer.unsigned(1);
    writer.array(8);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(1);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(255);
    writer.unsigned(6);
    writer.unsigned(4);
    writer.null();
    writer.array(2);
    writer.unsigned(2);
    writer.unsigned(12);
    writer.unsigned(2);
    writer.array(1);
    writer.unsigned(703);
    writer.null();
}

fn v11_staged_callbacks() -> Vec<u8> {
    let mut map = MapWriter::new();
    map.field("v").unsigned(11);
    let chain = map.field("ch");
    chain.array(2);
    v11_here_to_help_item(chain);
    v11_swarm_queen_item(chain);
    let queue = map.field("q");
    queue.array(2);
    queue.array(2);
    v11_here_to_help_item(queue);
    queue.unsigned(1);
    queue.array(2);
    v11_swarm_queen_item(queue);
    queue.unsigned(2);
    map.finish()
}

fn canonical_staged_callbacks() -> Vec<u8> {
    let mut map = MapWriter::new();
    map.field("v").unsigned(14);
    let chain = map.field("ch");
    chain.array(2);
    v12_here_to_help_item(chain);
    v12_swarm_queen_item(chain);
    let queue = map.field("q");
    queue.array(2);
    queue.array(2);
    v12_here_to_help_item(queue);
    queue.unsigned(1);
    queue.array(2);
    v12_swarm_queen_item(queue);
    queue.unsigned(2);
    map.finish()
}

fn canonical_card_v7(writer: &mut Writer) {
    writer.array(15);
    writer.unsigned(101);
    writer.unsigned(u64::from(FLAG_STUNNED | FLAG_DEFENDER));
    writer.array(1);
    might(writer, 3, |writer| expiry_end_of_turn(writer, 22), 77);
    writer.array(1);
    ordinary(writer, 9, 0, |writer| writer.unsigned(0));
    writer.array(0);
    writer.unsigned(7);
    writer.unsigned(4);
    writer.unsigned(12);
    writer.unsigned(13);
    writer.unsigned(14);
    writer.unsigned(1);
    writer.unsigned(8);
    writer.text("Legacy name");
    writer.unsigned(0);
    writer.array(0);
}

fn canonical_card_v9(writer: &mut Writer) {
    writer.array(15);
    writer.unsigned(202);
    writer.unsigned(u64::from(FLAG_FROM_FACEDOWN | FLAG_SHROUDED));
    writer.array(0);
    writer.array(1);
    ordinary(writer, 1, 0, |writer| writer.unsigned(0));
    writer.array(1);
    costed(writer, 14, 3, &[7, 2, 6], |writer| {
        expiry_end_of_turn(writer, 22)
    });
    writer.null();
    writer.unsigned(6);
    writer.unsigned(20);
    writer.unsigned(21);
    writer.unsigned(22);
    writer.unsigned(2);
    writer.unsigned(9);
    writer.null();
    writer.unsigned(0);
    writer.array(0);
}

fn canonical_card_v10(writer: &mut Writer) {
    writer.array(15);
    writer.unsigned(303);
    writer.unsigned(u64::from(FLAG_ENTERED_THIS_TURN | FLAG_REVEALING));
    writer.array(1);
    might(writer, -2, |writer| expiry_while_attached(writer, 404), 55);
    writer.array(1);
    ordinary(writer, 4, 2, |writer| writer.unsigned(2));
    writer.array(1);
    costed(writer, 20, 1, &[6, 7], |writer| writer.unsigned(0));
    writer.unsigned(202);
    writer.null();
    writer.unsigned(31);
    writer.unsigned(32);
    writer.unsigned(33);
    writer.unsigned(0);
    writer.unsigned(303);
    writer.text("Chosen title");
    writer.unsigned(0);
    writer.array(0);
}

fn assert_canonical_roundtrip(decoded: &GameBlob, expected: Vec<u8>) {
    assert_eq!(decoded.encode(), expected);
    assert_eq!(GameBlob::decode(&decoded.encode()), Some(decoded.clone()));
}

#[test]
fn v7_old_bool_lock_and_named_card_upgrade_to_v10() {
    let bytes = legacy(7, seat_v7_old_lock, card_v7);
    let decoded = GameBlob::decode(&bytes).expect("v7 bool-lock blob");
    assert!(decoded.seats[0]
        .play_lock
        .contains(agni_riftbound_turns::state::PlayLock::SPELLS));
    assert_eq!(decoded.cards[0].named.as_deref(), Some("Legacy name"));
    assert_eq!(decoded.cards[0].attached_to, Some(7));
    assert_eq!(decoded.cards[0].hidden_at, Some(4));
    assert_eq!(decoded.won, Some(1));
    assert_eq!(decoded.deaths_this_turn.len(), 1);
    assert_canonical_roundtrip(
        &decoded,
        canonical_v12(canonical_seat_v7_old_lock, canonical_card_v7),
    );
}

#[test]
fn v7_numeric_lock_and_champion_upgrade_to_v10() {
    let bytes = legacy(7, seat_v7_champion, card_v7);
    let decoded = GameBlob::decode(&bytes).expect("v7 champion blob");
    assert_eq!(decoded.seats[0].chosen_champion.as_deref(), Some("Riven"));
    assert!(decoded.seats[0]
        .play_lock
        .contains(agni_riftbound_turns::state::PlayLock::CARDS));
    assert_canonical_roundtrip(
        &decoded,
        canonical_v12(canonical_seat_v7_champion, canonical_card_v7),
    );
}

#[test]
fn v9_costed_grants_keep_their_own_chaos_and_rainbow_power() {
    let bytes = legacy(9, seat_v9, card_v9);
    let decoded = GameBlob::decode(&bytes).expect("v9 costed blob");
    assert!(decoded.seats[0].units_enter_ready_this_turn);
    assert_eq!(decoded.cards[0].granted_costed.len(), 1);
    assert_eq!(decoded.cards[0].named, None);
    assert_eq!(decoded.cards[0].controlled_by, Some(2));
    assert_eq!(decoded.cards[0].control_source, Some(9));
    assert_canonical_roundtrip(
        &decoded,
        canonical_v12(canonical_seat_v9, canonical_card_v9),
    );
}

#[test]
fn v10_combines_readiness_named_and_costed_state() {
    let bytes = legacy(10, seat_v10, card_v10);
    let decoded = GameBlob::decode(&bytes).expect("v10 blob");
    assert!(decoded.seats[0].next_unit_enters_ready);
    assert_eq!(decoded.seats[0].chosen_champion.as_deref(), Some("Aurora"));
    assert_eq!(decoded.cards[0].granted.len(), 1);
    assert_eq!(decoded.cards[0].granted_costed.len(), 1);
    assert_eq!(decoded.cards[0].named.as_deref(), Some("Chosen title"));
    assert_eq!(decoded.cards[0].attached_to, Some(202));
    assert_eq!(decoded.cards[0].hidden_at, None);
    assert_canonical_roundtrip(
        &decoded,
        canonical_v12(canonical_seat_v10, canonical_card_v10),
    );
}

#[test]
fn v10_discount_and_economy_rows_migrate_to_v11() {
    let zero = GameBlob::decode(&legacy(10, seat_v10_zero_discount, card_v10)).unwrap();
    assert!(zero.seats[0].promises.is_empty());
    assert_canonical_roundtrip(
        &zero,
        canonical_v12(canonical_seat_v10_zero_discount, canonical_card_v10),
    );

    assert!(
        GameBlob::decode(&legacy(10, seat_v10_economy, card_v10)).is_none(),
        "the unpublished economy v10 row is ambiguous with the integrated statics history"
    );
}

#[test]
fn old_chain_and_pending_modes_migrate_after_promised_repeat_for_all_executions() {
    let decoded = GameBlob::decode(&legacy_modes()).expect("v10 mode rows");
    assert_eq!(decoded.chain[0].mode_at(0), Some(3));
    assert_eq!(decoded.chain[0].mode_at(1), Some(4));
    assert_eq!(decoded.chain[0].slot(6), None);
    assert_eq!(decoded.pending(41).unwrap().item.mode_at(0), Some(3));
    assert_eq!(decoded.pending(41).unwrap().item.mode_at(1), Some(4));
    assert_eq!(decoded.pending(41).unwrap().item.slot(6), None);
    assert_canonical_roundtrip(&decoded, canonical_modes());
}

#[test]
fn v11_staged_callbacks_migrate_chain_and_pending_rows_to_v12() {
    let decoded = GameBlob::decode(&v11_staged_callbacks()).expect("v11 staged callbacks");
    assert_eq!(decoded.chain.len(), 2);
    assert_eq!(decoded.queue.len(), 2);
    assert_eq!(decoded.chain[0].stage, 3);
    assert_eq!(decoded.chain[0].mode_at(0), Some(4));
    assert_eq!(
        decoded.chain[0].targets,
        [TargetRef::Card(801), TargetRef::Zone(12)]
    );
    assert_eq!(decoded.chain[0].subject_card(), Some(802));
    assert_eq!(decoded.chain[0].awaiting, [701, 702]);
    assert!(decoded.chain[0].limited.is_none());
    assert_eq!(decoded.chain[1].stage, 4);
    assert_eq!(decoded.chain[1].mode_at(0), Some(6));
    assert_eq!(
        decoded.chain[1].targets,
        [TargetRef::Zone(12), TargetRef::Card(803)]
    );
    assert_eq!(decoded.chain[1].awaiting, [703]);
    assert!(decoded.chain[1].limited.is_none());
    assert_eq!(decoded.queue[0].item.stage, 3);
    assert_eq!(decoded.queue[0].item.awaiting, [701, 702]);
    assert!(decoded.queue[0].item.limited.is_none());
    assert_eq!(decoded.queue[1].item.stage, 4);
    assert_eq!(decoded.queue[1].item.awaiting, [703]);
    assert!(decoded.queue[1].item.limited.is_none());
    assert_canonical_roundtrip(&decoded, canonical_staged_callbacks());
}

#[test]
fn v12_limited_chain_item_roundtrips_its_zones_and_price() {
    let mut map = MapWriter::new();
    map.field("v").unsigned(12);
    let chain = map.field("ch");
    chain.array(1);
    v12_limited_item(chain);
    let decoded = GameBlob::decode(&map.finish()).expect("v12 limited item");
    assert_eq!(
        decoded.chain[0].limited,
        Some(Limited {
            zones: vec![12, 13],
            price: Price::LessEnergy(3),
        })
    );
    assert_eq!(GameBlob::decode(&decoded.encode()), Some(decoded));
}

#[test]
fn v12_and_v13_saved_chain_and_pending_rows_keep_trigger_resume_fields() {
    fn rich_item(writer: &mut Writer, id: u16, kind: [u64; 3], limited: bool) {
        writer.array(14);
        writer.unsigned(u64::from(id));
        writer.array(3);
        for value in kind {
            writer.unsigned(value);
        }
        writer.unsigned(1);
        writer.unsigned(2);
        writer.array(2);
        writer.unsigned(2);
        writer.unsigned(9);
        writer.array(2);
        writer.array(2);
        writer.unsigned(0);
        writer.unsigned(801);
        writer.array(2);
        writer.unsigned(2);
        writer.unsigned(12);
        writer.array(2);
        writer.unsigned(1);
        writer.unsigned(1);
        writer.array(8);
        writer.unsigned(1);
        writer.unsigned(0);
        writer.unsigned(255);
        writer.unsigned(255);
        writer.unsigned(255);
        writer.unsigned(255);
        writer.unsigned(2);
        writer.unsigned(4);
        writer.unsigned(3);
        writer.array(4);
        writer.unsigned(12);
        writer.signed(2);
        writer.unsigned(1);
        writer.bool(true);
        writer.array(2);
        writer.unsigned(0);
        writer.unsigned(802);
        writer.unsigned(1);
        writer.array(2);
        writer.unsigned(701);
        writer.unsigned(702);
        if limited {
            writer.array(2);
            writer.array(2);
            writer.unsigned(12);
            writer.unsigned(13);
            writer.array(2);
            writer.unsigned(2);
            writer.unsigned(3);
        } else {
            writer.null();
        }
    }

    let mut v12 = MapWriter::new();
    v12.field("v").unsigned(12);
    let chain = v12.field("ch");
    chain.array(1);
    rich_item(chain, 501, [0, 900, 0], true);
    let queue = v12.field("q");
    queue.array(1);
    queue.array(2);
    rich_item(queue, 502, [3, 901, 0], false);
    queue.unsigned(2);
    let decoded_v12 = GameBlob::decode(&v12.finish()).expect("independent v12 rows");
    assert_eq!(decoded_v12.chain[0].stage, 3);
    assert_eq!(decoded_v12.chain[0].targets.len(), 2);
    assert_eq!(decoded_v12.chain[0].awaiting, [701, 702]);
    assert_eq!(decoded_v12.chain[0].mode_at(0), Some(4));
    assert_eq!(decoded_v12.chain[0].slot(SLOT_PROMISED_REPEAT), Some(2));
    assert_eq!(decoded_v12.chain[0].spec_counts, [1, 1]);
    assert_eq!(decoded_v12.chain[0].execution, 1);
    assert_eq!(decoded_v12.chain[0].controller, 1);
    assert_eq!(decoded_v12.chain[0].noted.unwrap().might, 2);
    assert!(!decoded_v12.chain[0].noted.unwrap().buffed);
    assert_eq!(
        decoded_v12.chain[0].limited.as_ref().unwrap().zones,
        [12, 13]
    );
    assert_eq!(decoded_v12.pending(502).unwrap().item.awaiting, [701, 702]);
    assert_eq!(decoded_v12.pending(502).unwrap().needs, Needs::OptionalCost);
    assert_eq!(
        decoded_v12.pending(502).unwrap().item.status,
        ItemStatus::Resolving
    );
    assert!(decoded_v12.pending(502).unwrap().item.limited.is_none());
    assert_eq!(GameBlob::decode(&decoded_v12.encode()), Some(decoded_v12));

    let mut v13 = MapWriter::new();
    v13.field("v").unsigned(13);
    let chain = v13.field("ch");
    chain.array(1);
    rich_item(chain, 601, [2, 900, 1], false);
    let queue = v13.field("q");
    queue.array(1);
    queue.array(2);
    rich_item(queue, 602, [3, 901, 0], false);
    queue.unsigned(1);
    let decoded_v13 = GameBlob::decode(&v13.finish()).expect("independent v13 rows");
    assert_eq!(decoded_v13.chain[0].stage, 3);
    assert_eq!(decoded_v13.chain[0].noted.unwrap().might, 2);
    assert!(!decoded_v13.chain[0].noted.unwrap().buffed);
    assert_eq!(decoded_v13.chain[0].awaiting, [701, 702]);
    assert_eq!(
        decoded_v13.pending(602).unwrap().item.subject_card(),
        Some(802)
    );
    assert_eq!(GameBlob::decode(&decoded_v13.encode()), Some(decoded_v13));
}

#[test]
fn chain_rows_require_the_versioned_limited_field() {
    let mut old_shape = MapWriter::new();
    old_shape.field("v").unsigned(12);
    let chain = old_shape.field("ch");
    chain.array(1);
    old_chain_item(chain, &[255, 255, 255, 255, 255, 255, 255, 3, 4]);
    assert!(GameBlob::decode(&old_shape.finish()).is_none());

    let mut new_shape = MapWriter::new();
    new_shape.field("v").unsigned(11);
    let chain = new_shape.field("ch");
    chain.array(1);
    v12_here_to_help_item(chain);
    assert!(GameBlob::decode(&new_shape.finish()).is_none());
}

#[test]
fn incompatible_versions_and_layouts_are_refused() {
    assert!(GameBlob::decode(&legacy(8, seat_v10, card_v10)).is_none());
    assert!(GameBlob::decode(&legacy(7, seat_v9, card_v7)).is_none());
    assert!(GameBlob::decode(&legacy(9, seat_v9, card_v10)).is_none());
    assert!(GameBlob::decode(&legacy(10, seat_v10, card_v9)).is_none());
    let mut truncated = legacy(10, seat_v10, card_v10);
    truncated.pop();
    assert!(GameBlob::decode(&truncated).is_none());
}
