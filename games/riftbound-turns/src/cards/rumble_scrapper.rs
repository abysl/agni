use super::ferrous_forerunner::play_mechs;
use super::prelude::{done, on_hold_me, unit, with_statics, Location};
use super::rumble_mechanized_menace::your_mech;
use super::{Card, Flow, Grant, Item, Scope, Stage, Static};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const MECHS: usize = 1;

fn scrap_together(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_mechs(ctx, seat, Location::Base(seat), MECHS);
    done()
}

pub static CARD: Card = with_statics(
    unit("Rumble - Scrapper", &[], &[on_hold_me(&[], scrap_together)]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: your_mech,
        grants: &[Grant::Might(MIGHT)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::ferrous_forerunner::tests::mechs_of;
    use crate::cards::ferrous_forerunner::MECH_MIGHT;
    use crate::cards::rumble_mechanized_menace::is_mech;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle, statics};
    use crate::state::ItemKind;

    const RUMBLE: u32 = 90;
    const BOT: u32 = 91;
    const THEIR_MECH: u32 = 92;

    fn scrapyard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            RUMBLE,
            fixtures::BF1,
            0,
            "Rumble - Scrapper",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BOT, fixtures::BASE, 0, "Bubble Bot", 3));
        fixture.table.cards.push(fixtures::unit(
            THEIR_MECH,
            fixtures::BASE,
            1,
            "Adaptatron",
            3,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RUMBLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_might_aura_over_your_mechs_and_one_hold_trigger() {
        assert!(std::ptr::eq(script_of("Rumble - Scrapper").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Might(1)],
                ..
            }]
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
        assert_eq!((MIGHT, MECHS), (1, 1));
    }

    #[test]
    fn your_mechs_including_rumble_get_one_might_and_the_spawned_mech_reads_four() {
        let mut fixture = scrapyard();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(RUMBLE), 5, "including me");
        assert_eq!(ctx.current_might(BOT), 4);
        assert!(matches!(
            statics::grants_on(&ctx, BOT).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi is no Mech");
        assert_eq!(ctx.current_might(THEIR_MECH), 3, "not yours");
        let mech = play_mechs(&mut ctx, 0, Location::Base(0), 1)[0];
        assert!(is_mech(&ctx, mech));
        assert_eq!(
            ctx.current_might(mech),
            i32::from(MECH_MIGHT) + i32::from(MIGHT),
            "a spawned Mech token is one of your Mechs"
        );
        ctx.kill(RUMBLE, crate::engine::ctx::Cause::Rule);
        assert_eq!(ctx.current_might(BOT), 3, "365.1 · the aura dies with him");
        assert_eq!(ctx.current_might(mech), i32::from(MECH_MIGHT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn holding_with_him_plays_an_exhausted_three_might_mech_into_your_base() {
        let mut fixture = scrapyard();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == RUMBLE
        ));
        assert!(ctx.blob.prompt.is_none(), "the hold asks nothing");
        assert!(mechs_of(&ctx, 0).is_empty(), "the Mech waits for the chain");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(mechs_of(&ctx, 0), [next]);
        assert_eq!(
            ctx.location(next),
            Some(Location::Base(0)),
            "to your base, not here"
        );
        assert!(ctx.card(next).unwrap().exhausted);
        assert_eq!(
            ctx.current_might(next),
            i32::from(MECH_MIGHT) + i32::from(MIGHT)
        );
        assert_eq!(ctx.points(0), 1, "the hold point stays");
        assert!(mechs_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_with_him_and_a_hold_without_him_play_nothing() {
        let mut fixture = scrapyard();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(mechs_of(&ctx, 0).is_empty(), "a conquer is not a hold");
        drop(ctx);
        let mut fixture = scrapyard();
        fixture.table.card_mut(RUMBLE).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi held, not Rumble");
        assert!(mechs_of(&ctx, 0).is_empty());
    }
}
