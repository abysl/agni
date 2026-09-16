use super::prelude::{unit, with_statics};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const MOVES_FOR_IMMUNITY: u8 = 2;

pub fn moves_this_turn(_ctx: &Ctx, _unit: u32) -> Option<u8> {
    None
}

pub fn takes_no_damage(ctx: &Ctx, me: u32) -> bool {
    moves_this_turn(ctx, me).is_some_and(|moves| moves >= MOVES_FOR_IMMUNITY)
}

pub static CARD: Card = with_statics(
    unit("Kayn - Unleashed", &[Keyword::Ganking], &[]),
    &[Static::NoDamage(|ctx, me, _| takes_no_damage(ctx, me))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, settle};
    use agni_plugin_sdk::table::CardInfo;

    const KAYN: u32 = 90;

    fn kayn(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(KAYN, zone, seat, "Kayn - Unleashed", 6)
        }
    }

    fn shadow(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kayn(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn march(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(KAYN, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    fn march_again(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let mut ctx = fixture.ctx();
        ctx.ready(KAYN);
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        march(fixture, to)
    }

    #[test]
    fn the_script_prints_ganking_and_a_no_damage_static_over_the_move_count() {
        assert!(std::ptr::eq(script_of("Kayn - Unleashed").unwrap(), &CARD));
        assert_eq!(CARD.name, "Kayn - Unleashed");
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::NoDamage(_)]));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(MOVES_FOR_IMMUNITY, 2);
    }

    #[test]
    fn before_his_second_move_he_takes_damage_like_anyone() {
        let mut fixture = shadow(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(
            ctx.location(KAYN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(!takes_no_damage(&ctx, KAYN));
        assert!(ctx.damage(KAYN, 2, Cause::Item(1)));
        assert_eq!(ctx.damage_on(KAYN), 2);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 2, .. } if *card == KAYN
        )));
    }

    #[test]
    fn the_seam_reports_no_count_until_the_engine_keeps_one() {
        let mut fixture = shadow(fixtures::BASE);
        let ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(moves_this_turn(&ctx, KAYN), None);
        assert!(!takes_no_damage(&ctx, KAYN));
    }

    #[test]
    #[ignore = "engine gap · per-turn counters (moves per card this turn in CardState, reset at Expiration, read by moves_this_turn); the Static::NoDamage over takes_no_damage is wired and Ctx::damage honours it once the count is real"]
    fn after_two_moves_in_a_turn_he_takes_no_damage() {
        let mut fixture = shadow(fixtures::BASE);
        let ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(moves_this_turn(&ctx, KAYN), Some(1));
        drop(ctx);
        let mut ctx = march_again(&mut fixture, fixtures::BF2);
        assert_eq!(moves_this_turn(&ctx, KAYN), Some(2));
        assert!(takes_no_damage(&ctx, KAYN));
        assert!(!ctx.damage(KAYN, 6, Cause::Item(1)), "no damage is dealt");
        assert_eq!(ctx.damage_on(KAYN), 0);
        assert!(!ctx.damage(KAYN, 6, Cause::Combat));
        assert_eq!(ctx.damage_on(KAYN), 0);
        assert!(ctx.on_board(KAYN));
    }
}
