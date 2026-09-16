use super::prelude::{done, might_this_turn, on_friendly_unit_dies, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;

pub fn haunt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!("{{card {me}}} is gone · nothing to give"));
        return done();
    }
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Spectral Centaur",
    &[],
    &[on_friendly_unit_dies(&[], haunt)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Once, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const CENTAUR: u32 = 90;
    const ALLY: u32 = 91;
    const MIGHT: u8 = 5;

    fn centaur(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Mind".into()],
            ..fixtures::unit(CENTAUR, zone, seat, "Spectral Centaur", MIGHT)
        }
    }

    fn graveyard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(centaur(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_item(id: u16) -> Item {
        Item::new(
            id,
            ItemKind::Trigger {
                source: CENTAUR,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_the_pool_name_with_a_friendly_death_trigger() {
        assert!(std::ptr::eq(script_of("Spectral Centaur").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Friendly));
        assert_eq!(ability.once, Once::Never);
        assert!(ability.condition.is_none());
        assert_eq!(BONUS, 2);
        let fixture = graveyard();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CENTAUR).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_effect_gives_the_centaur_two_might_until_the_turn_ends_and_stacks_per_death() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(CENTAUR), i32::from(MIGHT));
        assert_eq!(haunt(&mut ctx, &trigger_item(7), Stage(0)), Flow::Done);
        assert_eq!(
            ctx.current_might(CENTAUR),
            i32::from(MIGHT) + i32::from(BONUS)
        );
        assert_eq!(haunt(&mut ctx, &trigger_item(8), Stage(0)), Flow::Done);
        assert_eq!(
            ctx.current_might(CENTAUR),
            i32::from(MIGHT) + 2 * i32::from(BONUS),
            "every friendly death is its own trigger"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3, "the bonus is its own");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CENTAUR}}} gets +2 Might this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(CENTAUR), i32::from(MIGHT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_centaur_that_left_the_board_before_the_trigger_resolves_gets_nothing() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        assert!(ctx.bounce(CENTAUR));
        assert_eq!(haunt(&mut ctx, &trigger_item(7), Stage(0)), Flow::Done);
        assert!(ctx
            .state_of(CENTAUR)
            .is_none_or(|state| state.might.is_empty()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CENTAUR}}} is gone · nothing to give")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn its_own_death_is_not_another_units() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(CENTAUR, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn every_other_friendly_death_gives_two_might_this_turn_and_an_enemy_death_gives_nothing() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(CENTAUR),
            i32::from(MIGHT) + i32::from(BONUS)
        );
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(CENTAUR),
            i32::from(MIGHT) + 2 * i32::from(BONUS)
        );
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "an enemy death is not friendly");
    }
}
