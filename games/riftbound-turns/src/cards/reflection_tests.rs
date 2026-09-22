use super::keeper_of_masks::{become_copy_of, spawn_reflection};
use super::trove_golem::tests::golds_of;
use super::{honest_broker, leblanc_deceiver, Keyword};
use crate::engine::ctx::{Cause, Ctx, Location, MoveCause};
use crate::engine::fixtures::{self, Fixture};
use crate::engine::{cleanup, phases, priority, settle, showdown};
use crate::state::{GameBlob, PromptWhy};

const ORIGINAL: u32 = 90;

fn step(fixture: &mut Fixture, act: impl FnOnce(&mut Ctx)) {
    let mut ctx = fixture.ctx();
    act(&mut ctx);
    settle(&mut ctx).unwrap();
    assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    let table = ctx.table.clone();
    drop(ctx);
    fixture.blob = GameBlob::decode(&fixture.blob.encode()).unwrap();
    fixture.commit(table);
}

fn choose(fixture: &mut Fixture, seat: u8, label: &str) {
    step(fixture, |ctx| fixtures::choose(ctx, seat, label).unwrap());
}

fn resolve(fixture: &mut Fixture) {
    for _ in 0..64 {
        let ctx = fixture.ctx();
        if let Some(prompt) = &ctx.blob.prompt {
            let seat = prompt.seat;
            let label = match ctx.blob.why {
                Some(
                    PromptWhy::OrderTriggers { .. } | PromptWhy::Assign | PromptWhy::Discard { .. },
                ) => fixtures::labels(&ctx)[0].clone(),
                Some(PromptWhy::OptionalCost { .. }) => "no".into(),
                other => panic!("unexpected prompt: {other:?}"),
            };
            drop(ctx);
            choose(fixture, seat, &label);
        } else if let Some(seat) = priority::holder(&ctx) {
            drop(ctx);
            step(fixture, |ctx| priority::pass(ctx, seat).unwrap());
        } else if let Some(combat) = &ctx.blob.showdown {
            let seat = combat.window.focus;
            drop(ctx);
            step(fixture, |ctx| showdown::pass(ctx, seat).unwrap());
        } else {
            assert!(ctx.blob.chain.is_empty());
            assert!(ctx.blob.queue.is_empty());
            return;
        }
    }
    panic!("resolution did not finish");
}

fn leblanc_copy(name: &str) -> (Fixture, u32) {
    let mut fixture = Fixture::enforced();
    fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().name =
        leblanc_deceiver::CARD.name.into();
    fixture
        .table
        .cards
        .push(fixtures::unit(ORIGINAL, fixtures::BF1, 0, name, 2));
    fixture.blob.set_holder(fixtures::BF1, Some(0));
    fixture.resolve();
    step(&mut fixture, |ctx| {
        cleanup::score_holds(ctx, 0);
    });
    choose(&mut fixture, 0, "yes");
    step(&mut fixture, |ctx| priority::pass(ctx, 0).unwrap());
    step(&mut fixture, |ctx| priority::pass(ctx, 1).unwrap());
    let token = fixture.table.next_id;
    choose(
        &mut fixture,
        0,
        &format!("{{card {}}}", fixtures::HAND_GEAR),
    );
    if fixture.blob.prompt.is_some() {
        choose(&mut fixture, 0, &format!("{{card {ORIGINAL}}}"));
    }
    let ctx = fixture.ctx();
    assert_eq!(
        ctx.card(token).unwrap().face(),
        ctx.card(ORIGINAL).unwrap().face()
    );
    assert!(ctx.is_token(token));
    assert!(ctx.is_temporary(token));
    assert!(!ctx.is_temporary(ORIGINAL));
    assert!(!ctx.card(token).unwrap().exhausted);
    assert!(std::ptr::eq(
        ctx.script(token).unwrap(),
        ctx.script(ORIGINAL).unwrap()
    ));
    drop(ctx);
    (fixture, token)
}

#[test]
fn leblanc_reflection_and_honest_broker_both_pay_gold_when_killed_in_one_combat() {
    let (mut fixture, token) = leblanc_copy(honest_broker::CARD.name);
    let enemy = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
    enemy.zone = Some(fixtures::BF1);
    enemy.might = Some(4);
    fixture.blob.set_contested(fixtures::BF1, Some(1));
    fixture.resolve();
    step(&mut fixture, |ctx| cleanup::run(ctx, None));
    assert!(fixture
        .blob
        .showdown
        .as_ref()
        .is_some_and(|held| held.combat));
    resolve(&mut fixture);
    let ctx = fixture.ctx();
    assert_eq!(ctx.card(ORIGINAL).unwrap().zone, Some(fixtures::TRASH));
    assert!(ctx.card(token).is_none());
    assert_eq!(golds_of(&ctx, 0).len(), 2);
    assert!(golds_of(&ctx, 1).is_empty());
    for gold in golds_of(&ctx, 0) {
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert!(ctx.card(gold).unwrap().exhausted);
    }
}

#[test]
fn leblanc_reflection_temporary_kill_resolves_deathknell_before_hold_scoring() {
    let (mut fixture, token) = leblanc_copy(honest_broker::CARD.name);
    let points = fixture.ctx().points(0);
    fixture.blob.clear_scored();
    step(&mut fixture, phases::start_turn);
    step(&mut fixture, |ctx| priority::pass(ctx, 0).unwrap());
    step(&mut fixture, |ctx| priority::pass(ctx, 1).unwrap());
    let ctx = fixture.ctx();
    assert!(ctx.card(token).is_none());
    assert!(ctx.on_board(ORIGINAL));
    assert_eq!(ctx.points(0), points);
    assert_eq!(ctx.blob.chain.len(), 1);
    assert!(golds_of(&ctx, 0).is_empty());
    drop(ctx);
    resolve(&mut fixture);
    let ctx = fixture.ctx();
    assert_eq!(golds_of(&ctx, 0).len(), 1);
    assert_eq!(ctx.points(0), points + 1);
}

#[test]
fn copied_deathknell_with_multiple_prompt_stages_survives_the_token_disappearing() {
    let (mut fixture, token) = leblanc_copy("Undercover Agent");
    let hand = fixture.ctx().hand_of(0).len();
    step(&mut fixture, |ctx| {
        ctx.kill(token, Cause::Rule);
    });
    resolve(&mut fixture);
    let ctx = fixture.ctx();
    assert!(ctx.card(token).is_none());
    assert_eq!(ctx.hand_of(0).len(), hand);
    assert!(ctx.blob.log.iter().any(|line| line == "{seat 0} draws 2"));
}

#[test]
fn copied_move_trigger_works_after_serializing_and_does_not_replay_on_copy() {
    let (mut fixture, token) = leblanc_copy("Traveling Merchant");
    let discards = fixture
        .blob
        .log
        .iter()
        .filter(|line| line.contains("discards"))
        .count();
    step(&mut fixture, |ctx| {
        ctx.move_unit(token, Location::Base(0), MoveCause::Effect);
    });
    resolve(&mut fixture);
    let ctx = fixture.ctx();
    assert_eq!(
        ctx.blob
            .log
            .iter()
            .filter(|line| line.contains("discards"))
            .count(),
        discards + 1
    );
    assert!(ctx.is_temporary(token));
}

#[test]
fn a_reflection_of_a_reflection_keeps_printed_rules_but_not_granted_temporary() {
    let (mut fixture, first) = leblanc_copy(honest_broker::CARD.name);
    step(&mut fixture, |ctx| {
        let second = spawn_reflection(ctx, 0, Location::Base(0), true).unwrap();
        assert!(become_copy_of(ctx, second, first));
        assert_eq!(
            ctx.card(second).unwrap().face(),
            ctx.card(ORIGINAL).unwrap().face()
        );
        assert!(ctx.has_keyword(second, Keyword::Deathknell));
        assert!(!ctx.is_temporary(second));
        ctx.kill(second, Cause::Rule);
    });
    resolve(&mut fixture);
    assert_eq!(golds_of(&fixture.ctx(), 0).len(), 1);
}

#[test]
fn bouncing_a_copied_broker_despawns_it_without_deathknell() {
    let (mut fixture, token) = leblanc_copy(honest_broker::CARD.name);
    step(&mut fixture, |ctx| {
        ctx.bounce(token);
    });
    resolve(&mut fixture);
    let ctx = fixture.ctx();
    assert!(ctx.card(token).is_none());
    assert!(golds_of(&ctx, 0).is_empty());
}

#[test]
fn queued_copy_trigger_keeps_its_text_if_the_reflection_changes_again() {
    let (mut fixture, token) = leblanc_copy("Traveling Merchant");
    fixture
        .table
        .cards
        .push(fixtures::unit(91, fixtures::BASE, 0, "Honest Broker", 2));
    fixture.resolve();
    step(&mut fixture, |ctx| {
        ctx.move_unit(token, Location::Base(0), MoveCause::Effect);
    });
    step(&mut fixture, |ctx| {
        assert!(become_copy_of(ctx, token, 91));
    });
    resolve(&mut fixture);
    assert_eq!(fixture.ctx().blob.seat(0).draws, 1);
    assert!(golds_of(&fixture.ctx(), 0).is_empty());
}

#[test]
fn an_existing_reflection_gains_and_loses_copied_auras_immediately() {
    let (mut fixture, token) = leblanc_copy("Honest Broker");
    fixture
        .table
        .cards
        .push(fixtures::unit(91, fixtures::BASE, 0, "Baron Nashor", 10));
    fixture.resolve();
    step(&mut fixture, |ctx| {
        let before = ctx.current_might(ORIGINAL);
        assert!(become_copy_of(ctx, token, 91));
        assert_eq!(ctx.current_might(ORIGINAL), before + 2);
        assert!(become_copy_of(ctx, token, ORIGINAL));
        assert_eq!(ctx.current_might(ORIGINAL), before);
    });
}

#[test]
fn copied_activated_ability_and_keywords_remain_usable() {
    let (mut fixture, token) = leblanc_copy("Legion Marauder");
    step(&mut fixture, |ctx| {
        assert!(ctx.has_keyword(token, Keyword::Empower(super::legion_marauder::EMPOWER)));
        crate::engine::activate::activate(ctx, 0, token, 0).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
    });
    resolve(&mut fixture);
    let ctx = fixture.ctx();
    assert!(ctx.is_empowered(token));
    assert_eq!(ctx.current_might(token), 3);
    assert!(!ctx.is_empowered(ORIGINAL));
    assert!(ctx.is_temporary(token));
}
