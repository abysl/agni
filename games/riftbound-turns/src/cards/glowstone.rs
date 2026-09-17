use super::prelude::{
    activated, deal, disempowering_self, done, empower, friendly_units, gear, kill, named, recall,
    seat_target, target, triggered,
};
use super::{
    Card, Cost, Filter, Flow, Item, Keyword, Power, Stage, TargetKind, TargetSpec, Timing, Trigger,
};
use crate::engine::control;
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow, Power::Rainbow],
};
pub const DAMAGE: u8 = 5;
pub const HAND_OVER: u8 = 1;
pub const DETONATE: u8 = 2;
pub const A_PLAYER: TargetSpec = target(
    Filter::Any,
    1,
    1,
    TargetKind::Seat,
    "a player who takes this and recalls it",
);

pub fn hand_control_to(ctx: &mut Ctx, stone: u32, seat: u8) -> bool {
    if ctx.controller(stone) == seat {
        recall(ctx, stone, false);
        ctx.narrate(format!(
            "{{seat {seat}}} keeps {{card {stone}}} and recalls it"
        ));
        return true;
    }
    if ctx.owner(stone) == seat {
        return control::revert(ctx, stone);
    }
    ctx.set_controller(stone, seat, stone)
}

fn hand_over(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(seat) = seat_target(item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        return done();
    }
    hand_control_to(ctx, me, seat);
    done()
}

fn detonate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    kill(ctx, item, me);
    ctx.narrate(format!("{{card {me}}} is killed"));
    for unit in friendly_units(ctx, seat) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
        }
    }
    done()
}

pub static CARD: Card = gear(
    "Glowstone",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        named(
            disempowering_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[A_PLAYER],
                hand_over,
            )),
            "disempower this: a player takes it",
        ),
        triggered(Trigger::EndOfTurn, &[], detonate),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, priority, settle};
    use crate::state::{GameBlob, ItemKind, Mode, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const STONE: u32 = 90;

    fn stone(seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            exhausted,
            ..fixtures::gear(STONE, fixtures::BASE, seat, "Glowstone", 2)
        }
    }

    fn cave(seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(stone(seat, exhausted));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(STONE).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn empower_it(ctx: &mut Ctx) {
        let runes = ctx.runes_of(0).len();
        activate::activate(ctx, 0, STONE, 0).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 2,
            "two runes recycled for the rainbows"
        );
        resolve_chain(ctx);
        assert!(ctx.is_empowered(STONE));
    }

    #[test]
    fn the_script_empowers_for_two_rainbow_hands_itself_over_by_disempowering_and_detonates_at_end_of_turn(
    ) {
        assert!(std::ptr::eq(script_of("Glowstone").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 3);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        let hand_over = &CARD.abilities[usize::from(HAND_OVER)];
        assert_eq!(hand_over.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(hand_over.cost, Some(Cost::FREE));
        assert_eq!(hand_over.self_cost, SelfCost::Disempower);
        assert!(hand_over.usable.is_none());
        assert_eq!(hand_over.targets, [A_PLAYER]);
        assert_eq!(A_PLAYER.kind, TargetKind::Seat);
        let detonate = &CARD.abilities[usize::from(DETONATE)];
        assert_eq!(detonate.trigger, Trigger::EndOfTurn);
        assert!(detonate.targets.is_empty());
        assert!(detonate.condition.is_none());
        assert_eq!(DAMAGE, 5);
    }

    #[test]
    fn once_empowered_it_offers_every_player_and_the_pick_takes_it_to_their_base_disempowered() {
        let mut fixture = cave(0, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, STONE, HAND_OVER),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "the usable gate · nothing to disempower"
        );
        empower_it(&mut ctx);
        activate::activate(&mut ctx, 0, STONE, HAND_OVER).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{seat 0}", "{seat 1}", "cancel"],
            "any player, yourself included"
        );
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        assert!(ctx.card(STONE).unwrap().exhausted, "exhausted as the cost");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.controller(STONE), 0, "not until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_empowered(STONE));
        assert!(ctx.events.contains(&Event::Disempowered { card: STONE }));
        assert_eq!(ctx.controller(STONE), 1);
        assert_eq!(ctx.owner(STONE), 0, "ownership never changes");
        assert_eq!(ctx.location(STONE), Some(Location::Base(1)));
        assert!(ctx.effects.contains(&Effect::Move {
            card: STONE,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert_eq!(
            ctx.state_of(STONE).unwrap().control_source,
            Some(STONE),
            "the stone is its own control source, so the theft is for good"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} takes control of {card 90}".to_string()));
        assert_eq!(
            activate::activate(&mut ctx, 0, STONE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard)),
            "it is theirs now"
        );
        assert!(ctx.ready(STONE));
        let their_runes = ctx.runes_of(1).len();
        assert!(
            activate::activate(&mut ctx, 1, STONE, 0).is_err(),
            "their turn, not yours"
        );
        assert_eq!(ctx.runes_of(1).len(), their_runes);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn choosing_yourself_keeps_it_and_choosing_the_owner_gives_it_back() {
        let mut fixture = cave(0, false);
        let mut ctx = fixture.ctx();
        empower_it(&mut ctx);
        activate::activate(&mut ctx, 0, STONE, HAND_OVER).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        fixtures::choose(&mut ctx, 0, "{seat 0}").unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.controller(STONE), 0);
        assert_eq!(ctx.location(STONE), Some(Location::Base(0)));
        assert!(!ctx.is_empowered(STONE));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps {card 90} and recalls it".to_string()));
        assert!(ctx.set_controller(STONE, 1, STONE));
        assert_eq!(ctx.location(STONE), Some(Location::Base(1)));
        assert!(hand_control_to(&mut ctx, STONE, 0));
        assert_eq!(ctx.controller(STONE), 0);
        assert_eq!(ctx.location(STONE), Some(Location::Base(0)));
        assert!(ctx
            .state_of(STONE)
            .is_none_or(|row| row.control_source.is_none()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_the_end_of_its_controllers_turn_it_dies_and_deals_five_to_all_their_units() {
        let mut fixture = cave(0, false);
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the end-of-turn trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger {
                source: STONE,
                index: DETONATE
            }
        ));
        assert!(ctx.on_board(STONE), "not until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(STONE));
        assert_eq!(ctx.card(STONE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 90} is killed".to_string()));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {STONE}}} deals 5 to {{card {}}}",
            fixtures::VI
        )));
        settle(&mut ctx).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(!ctx.on_board(fixtures::VI), "3 Might under 5 damage");
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "the other player's units are untouched"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_stone_they_hold_detonates_at_the_end_of_their_turn_not_yours() {
        let mut fixture = cave(1, false);
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            ctx.blob
                .chain
                .iter()
                .all(|held| held.kind.source() != STONE),
            "not the holder's turn"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(STONE));
        drop(ctx);
        let mut fixture = cave(1, false);
        fixture.blob = GameBlob::start(2, 1, Mode::Enforced);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        phases::end_turn(&mut ctx).unwrap();
        assert!(ctx
            .blob
            .chain
            .iter()
            .any(|held| held.kind.source() == STONE));
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(STONE));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {STONE}}} deals 5 to {{card {}}}",
            fixtures::THEIR_UNIT
        )));
        assert!(!ctx.blob.log.contains(&format!(
            "{{card {STONE}}} deals 5 to {{card {}}}",
            fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_stone_cannot_be_handed_over_and_an_unempowered_one_is_refused() {
        let mut fixture = cave(0, true);
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(STONE));
        assert_eq!(
            activate::activate(&mut ctx, 0, STONE, HAND_OVER),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.ready(STONE));
        assert!(ctx.disempower(STONE));
        assert_eq!(
            activate::activate(&mut ctx, 0, STONE, HAND_OVER),
            Err(Refusal::Illegal(Reason::NotEmpowered))
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != STONE || offer.index == 0));
    }

    #[test]
    fn the_disempower_is_paid_at_activation_before_the_ability_is_on_the_chain() {
        let mut fixture = cave(0, false);
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(STONE));
        activate::activate(&mut ctx, 0, STONE, HAND_OVER).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        assert!(!ctx.is_empowered(STONE));
        assert_eq!(ctx.blob.chain.len(), 1);
    }
}
