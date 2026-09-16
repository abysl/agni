use super::poro_snax::ready_to_exhaust;
use super::prelude::{
    a_unit, activated, card_target, done, gear, might_this_turn, named, paying_with, usable_if,
};
use super::{Card, Cost, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;
pub const CAST: u8 = 0;

fn cast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, BONUS, None);
        ctx.narrate(format!("{{card {unit}}} gets +{BONUS} Might this turn"));
    }
    done()
}

pub static CARD: Card = gear(
    "Divining Shells",
    &[Keyword::Vision],
    &[usable_if(
        named(
            paying_with(
                activated(
                    Timing::Action,
                    Cost::FREE,
                    &[a_unit("a unit to give +2 Might this turn")],
                    cast,
                ),
                SelfCost::KillSelf,
            ),
            "exhaust and kill this: give a unit +2 Might this turn",
        ),
        ready_to_exhaust,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, showdown};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const SHELLS: u32 = 90;
    const JINX: u32 = 91;

    fn shells(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            exhausted,
            ..fixtures::gear(SHELLS, zone, 0, CARD.name, 2)
        }
    }

    fn shore(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shells(zone, exhausted));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BF1, 1, "Jinx", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_vision_and_has_one_free_kill_this_action_gated_on_being_ready() {
        assert!(std::ptr::eq(script_of("Divining Shells").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Vision]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let cast = &CARD.abilities[usize::from(CAST)];
        assert_eq!(cast.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(cast.cost, Some(Cost::FREE));
        assert_eq!(cast.self_cost, SelfCost::KillSelf);
        assert!(
            cast.usable.is_some(),
            "the exhaust is part of the cost: spent Shells can't be cast"
        );
        assert_eq!(cast.targets[0].filter, crate::cards::prelude::UNIT);
        assert_eq!(
            cast.label,
            Some("exhaust and kill this: give a unit +2 Might this turn")
        );
        assert_eq!(BONUS, 2);
    }

    #[test]
    fn the_exhaust_and_the_kill_buy_two_might_for_the_turn_on_any_unit() {
        let mut fixture = shore(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == SHELLS)
            .unwrap();
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {SHELLS}}}: exhaust and kill this: give a unit +2 Might this turn")
        );
        activate::activate(&mut ctx, 0, SHELLS, CAST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {JINX}}}"),
                "cancel".to_string()
            ],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(!ctx.on_board(SHELLS), "the kill is paid as the cost");
        assert!(ctx.in_trash(SHELLS));
        assert!(
            ctx.effects.contains(&Effect::exhaust(SHELLS)),
            "exhausted on the way out"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: false, .. } if *card == SHELLS
        )));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index } if source == SHELLS && index == CAST
        ));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3 + i32::from(BONUS));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +2 Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_an_action_it_is_cast_during_a_showdown_with_the_focus() {
        let mut fixture = shore(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        ctx.blob.set_contested(fixtures::BF1, Some(1));
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.is_some());
        assert_eq!(
            activate::activate(&mut ctx, 0, SHELLS, CAST),
            Err(Refusal::NotYourFocus)
        );
        showdown::pass(&mut ctx, 1).unwrap();
        activate::activate(&mut ctx, 0, SHELLS, CAST).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.in_trash(SHELLS));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 3 + i32::from(BONUS));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn spent_shells_the_other_seat_and_shells_still_in_hand_are_refused() {
        let mut spent = shore(fixtures::BASE, true);
        let mut ctx = spent.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == SHELLS));
        assert_eq!(
            activate::activate(&mut ctx, 0, SHELLS, CAST),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses Shells that can't be exhausted"
        );
        assert!(ctx.on_board(SHELLS));
        drop(ctx);
        let mut fixture = shore(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SHELLS, CAST),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.on_board(SHELLS));
        drop(ctx);
        let mut held = shore(fixtures::HAND, false);
        let mut ctx = held.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SHELLS, CAST),
            Err(Refusal::Illegal(Reason::NotInPlay))
        );
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
    }

    #[test]
    fn vision_looks_at_the_top_card_as_the_shells_are_played_and_may_recycle_it() {
        let mut fixture = shore(fixtures::HAND, false);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHELLS).unwrap();
        assert!(ctx.on_board(SHELLS));
        assert_eq!(ctx.blob.chain.len(), 1, "817.1.c · the Vision trigger");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Peek { card: 23, seat: 0 })));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}".to_string(), "skip".to_string()]
        );
    }
}
