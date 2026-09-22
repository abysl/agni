use super::keeper_of_masks::{become_copy_of, spawn_reflection};
use super::trove_golem::tests::golds_of;
use super::{honest_broker, leblanc_deceiver, Keyword};
use crate::engine::ctx::{Cause, Ctx, Location, MoveCause};
use crate::engine::fixtures::{self, Fixture};
use crate::engine::{cleanup, phases, priority};
use crate::state::PromptWhy;

const ORIGINAL: u32 = 90;

fn first_required_or_decline_optional(ctx: &Ctx) -> String {
    match ctx.blob.why {
        Some(PromptWhy::OrderTriggers { .. } | PromptWhy::Assign | PromptWhy::Discard { .. }) => {
            fixtures::labels(ctx)[0].clone()
        }
        Some(PromptWhy::OptionalCost { .. }) => "no".into(),
        other => panic!("unexpected prompt: {other:?}"),
    }
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
    fixture.act_and_reload(|ctx| {
        cleanup::score_holds(ctx, 0);
    });
    fixture.choose_and_reload(0, "yes");
    fixture.act_and_reload(|ctx| priority::pass(ctx, 0).unwrap());
    fixture.act_and_reload(|ctx| priority::pass(ctx, 1).unwrap());
    let token = fixture.table.next_id;
    fixture.choose_and_reload(0, &format!("{{card {}}}", fixtures::HAND_GEAR));
    if fixture.blob.prompt.is_some() {
        fixture.choose_and_reload(0, &format!("{{card {ORIGINAL}}}"));
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
    fixture.act_and_reload(|ctx| cleanup::run(ctx, None));
    assert!(fixture
        .blob
        .showdown
        .as_ref()
        .is_some_and(|held| held.combat));
    fixture.resolve_and_reload(first_required_or_decline_optional);
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
    fixture.act_and_reload(phases::start_turn);
    fixture.act_and_reload(|ctx| priority::pass(ctx, 0).unwrap());
    fixture.act_and_reload(|ctx| priority::pass(ctx, 1).unwrap());
    let ctx = fixture.ctx();
    assert!(ctx.card(token).is_none());
    assert!(ctx.on_board(ORIGINAL));
    assert_eq!(ctx.points(0), points);
    assert_eq!(ctx.blob.chain.len(), 1);
    assert!(golds_of(&ctx, 0).is_empty());
    drop(ctx);
    fixture.resolve_and_reload(first_required_or_decline_optional);
    let ctx = fixture.ctx();
    assert_eq!(golds_of(&ctx, 0).len(), 1);
    assert_eq!(ctx.points(0), points + 1);
}

#[test]
fn copied_deathknell_with_multiple_prompt_stages_survives_the_token_disappearing() {
    let (mut fixture, token) = leblanc_copy("Undercover Agent");
    let hand = fixture.ctx().hand_of(0).len();
    fixture.act_and_reload(|ctx| {
        ctx.kill(token, Cause::Rule);
    });
    fixture.resolve_and_reload(first_required_or_decline_optional);
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
    fixture.act_and_reload(|ctx| {
        ctx.move_unit(token, Location::Base(0), MoveCause::Effect);
    });
    fixture.resolve_and_reload(first_required_or_decline_optional);
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
    fixture.act_and_reload(|ctx| {
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
    fixture.resolve_and_reload(first_required_or_decline_optional);
    assert_eq!(golds_of(&fixture.ctx(), 0).len(), 1);
}

#[test]
fn bouncing_a_copied_broker_despawns_it_without_deathknell() {
    let (mut fixture, token) = leblanc_copy(honest_broker::CARD.name);
    fixture.act_and_reload(|ctx| {
        ctx.bounce(token);
    });
    fixture.resolve_and_reload(first_required_or_decline_optional);
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
    fixture.act_and_reload(|ctx| {
        ctx.move_unit(token, Location::Base(0), MoveCause::Effect);
    });
    fixture.act_and_reload(|ctx| {
        assert!(become_copy_of(ctx, token, 91));
    });
    fixture.resolve_and_reload(first_required_or_decline_optional);
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
    fixture.act_and_reload(|ctx| {
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
    fixture.act_and_reload(|ctx| {
        assert!(ctx.has_keyword(token, Keyword::Empower(super::legion_marauder::EMPOWER)));
        crate::engine::activate::activate(ctx, 0, token, 0).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
    });
    fixture.resolve_and_reload(first_required_or_decline_optional);
    let ctx = fixture.ctx();
    assert!(ctx.is_empowered(token));
    assert_eq!(ctx.current_might(token), 3);
    assert!(!ctx.is_empowered(ORIGINAL));
    assert!(ctx.is_temporary(token));
}

#[test]
fn arena_queued_copy_trigger_survives_its_reflection_leaving_play() {
    let (mut fixture, token) = leblanc_copy("Kai'Sa - Survivor");
    fixture.table.card_mut(ORIGINAL).unwrap().zone = Some(fixtures::BASE);
    fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Reckoner's Arena".into();
    fixture.blob.clear_scored();
    fixture.resolve();
    fixture.act_and_reload(|ctx| {
        cleanup::score_holds(ctx, 0);
    });
    while let Some(PromptWhy::OrderTriggers { seat }) = fixture.blob.why {
        let label = fixtures::labels(&fixture.ctx())[0].clone();
        fixture.choose_and_reload(seat, &label);
    }
    fixture.act_and_reload(|ctx| priority::pass(ctx, 0).unwrap());
    fixture.act_and_reload(|ctx| priority::pass(ctx, 1).unwrap());
    assert_eq!(fixture.blob.chain.len(), 1);
    assert_eq!(fixture.blob.chain[0].kind.source(), token);
    fixture.act_and_reload(|ctx| {
        ctx.kill(token, Cause::Rule);
    });
    fixture.resolve_and_reload(first_required_or_decline_optional);
    assert_eq!(fixture.ctx().blob.seat(0).draws, 1);
}
