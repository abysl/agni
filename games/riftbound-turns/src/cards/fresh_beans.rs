use super::prelude::{done, draw, exhausting_self, gear, on_you_play_card, optional, when};
use super::{Card, Flow, Item, Source, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub const DRAWS: usize = 1;

pub fn a_unit_during_a_showdown(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Played { kind, .. } if kind == KIND_UNIT) && ctx.blob.showdown.is_some()
}

fn sip(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = gear(
    "Fresh Beans",
    &[],
    &[when(
        optional(exhausting_self(on_you_play_card(&[], sip))),
        a_unit_during_a_showdown,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const BEANS: u32 = 90;
    const RECRUIT: u32 = 91;
    const JINX: u32 = 92;

    fn beans(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into()],
            exhausted,
            ..fixtures::gear(BEANS, fixtures::BASE, 0, CARD.name, 2)
        }
    }

    fn brewing(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(beans(exhausted));
        fixture
            .table
            .cards
            .push(fixtures::unit(RECRUIT, fixtures::BF1, 0, "Recruit", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BF1, 1, "Jinx", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        ctx.blob.set_contested(fixtures::BF1, Some(1));
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.is_some());
    }

    fn played(card: u32, kind: &str) -> Event {
        Event::Played {
            card,
            controller: 0,
            kind: kind.into(),
            origin: Origin::Hand,
            paid_additional: false,
        }
    }

    fn source() -> Source {
        Source {
            card: BEANS,
            ability: 0,
        }
    }

    #[test]
    fn the_script_is_an_optional_exhaust_trigger_on_your_unit_plays_gated_to_showdowns() {
        assert!(std::ptr::eq(script_of("Fresh Beans").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.optional, "you may exhaust this");
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(ability.cost.is_none());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_condition_reads_a_unit_play_while_a_showdown_is_open_and_nothing_else() {
        let mut fixture = brewing(false);
        let mut ctx = fixture.ctx();
        assert!(
            !a_unit_during_a_showdown(&ctx, &played(fixtures::HAND_UNIT, KIND_UNIT), source()),
            "no showdown is open"
        );
        open_showdown(&mut ctx);
        assert!(a_unit_during_a_showdown(
            &ctx,
            &played(fixtures::HAND_UNIT, KIND_UNIT),
            source()
        ));
        assert!(
            !a_unit_during_a_showdown(&ctx, &played(fixtures::HAND_SPELL, "Spell"), source()),
            "a spell is not a unit"
        );
        assert!(
            !a_unit_during_a_showdown(&ctx, &played(fixtures::HAND_GEAR, "Gear"), source()),
            "gear is not a unit"
        );
        assert!(!a_unit_during_a_showdown(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source()
        ));
    }

    #[test]
    fn a_unit_played_during_a_showdown_asks_to_exhaust_the_beans_and_yes_draws_one() {
        let mut fixture = brewing(false);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        let hand = ctx.hand_of(0).len();
        ctx.raise(played(fixtures::HAND_UNIT, KIND_UNIT));
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(BEANS).unwrap().exhausted,
            "the exhaust is paid as the trigger finalizes"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BEANS
        ));
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_token_unit_played_during_a_showdown_asks_too() {
        let mut fixture = brewing(false);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        let hand = ctx.hand_of(0).len();
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).is_some());
        settle(&mut ctx).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "350.2 · a Sprite played is a unit played"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(BEANS).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_keeps_the_beans_ready_and_draws_nothing() {
        let mut fixture = brewing(false);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        let hand = ctx.hand_of(0).len();
        ctx.raise(played(fixtures::HAND_UNIT, KIND_UNIT));
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(!ctx.card(BEANS).unwrap().exhausted);
        assert!(!ctx
            .blob
            .chain
            .iter()
            .any(|item| item.kind.source() == BEANS));
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BEANS}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn outside_a_showdown_a_spent_beans_or_an_opponents_unit_wakes_nothing() {
        let mut fixture = brewing(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PlayLocation { item: 1 }));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(ctx.blob.chain.is_empty(), "no showdown is open");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        drop(ctx);
        let mut spent = brewing(true);
        let mut ctx = spent.ctx();
        open_showdown(&mut ctx);
        ctx.raise(played(fixtures::HAND_UNIT, KIND_UNIT));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "an exhausted Beans can't pay");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BEANS}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut theirs = brewing(false);
        let mut ctx = theirs.ctx();
        open_showdown(&mut ctx);
        ctx.raise(Event::Played {
            card: fixtures::THEIR_HAND_CARD,
            controller: 1,
            kind: KIND_UNIT.into(),
            origin: Origin::Hand,
            paid_additional: false,
        });
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "not your unit");
        assert!(!ctx
            .blob
            .chain
            .iter()
            .any(|item| item.kind.source() == BEANS));
        assert!(!ctx.card(BEANS).unwrap().exhausted);
    }
}
