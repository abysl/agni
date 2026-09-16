use super::prelude::{friendly_gear, gear, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub fn symbols_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_gear(ctx, seat)
        .into_iter()
        .filter(|held| {
            ctx.script(*held)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| !ctx.is_facedown(*held))
        .collect()
}

pub fn tie_recalls_all(ctx: &Ctx, attacker: u8) -> bool {
    !symbols_of(ctx, attacker).is_empty()
}

pub fn your_tie_recalls_all(ctx: &Ctx, symbol: u32, attacker: u8) -> bool {
    statics::in_play(ctx, symbol) && ctx.controller(symbol) == attacker
}

pub fn recall_all(ctx: &mut Ctx, zone: u16) -> Vec<u32> {
    ctx.recall_all(zone)
}

pub static CARD: Card = with_statics(
    gear("Symbol of the Solari", &[], &[]),
    &[Static::TieRecallsAll(your_tie_recalls_all)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle};

    const SYMBOL: u32 = 90;
    const DEFENDER: u32 = 91;

    fn shrine(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut symbol = fixtures::gear(SYMBOL, zone, 0, "Symbol of the Solari", 1);
        symbol.domain = vec!["Order".into()];
        fixture.table.cards.push(symbol);
        fixture
            .table
            .cards
            .push(fixtures::unit(DEFENDER, fixtures::BF1, 1, "Defender", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_gear_whose_whole_text_is_the_tie_static_and_the_registry_resolves_it() {
        let fixture = shrine(fixtures::BASE);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SYMBOL).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Symbol of the Solari");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::TieRecallsAll(_)]));
    }

    #[test]
    fn the_static_applies_to_the_attacker_who_controls_a_symbol_in_play_and_to_nobody_else() {
        let mut fixture = shrine(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(symbols_of(&ctx, 0), [SYMBOL]);
        assert!(symbols_of(&ctx, 1).is_empty());
        assert!(tie_recalls_all(&ctx, 0));
        assert_eq!(ctx.tie_recalls_all(0), Some(SYMBOL));
        assert!(
            !tie_recalls_all(&ctx, 1),
            "the defender's tie is the attacker's"
        );
        assert_eq!(ctx.tie_recalls_all(1), None);
        drop(ctx);
        let mut in_hand = shrine(fixtures::HAND);
        let ctx = in_hand.ctx();
        assert!(!tie_recalls_all(&ctx, 0), "a symbol in hand is not in play");
        assert_eq!(ctx.tie_recalls_all(0), None);
    }

    #[test]
    fn recall_all_sends_every_unit_at_the_battlefield_home_as_a_recall_not_a_move() {
        let mut fixture = shrine(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            recall_all(&mut ctx, fixtures::BF1),
            [fixtures::VI, DEFENDER]
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.location(DEFENDER), Some(Location::Base(1)));
        assert!(
            !ctx.events.iter().any(|event| matches!(
                event,
                crate::engine::ctx::Event::Moved {
                    cause: MoveCause::Effect,
                    ..
                }
            )),
            "a recall is not a move"
        );
        assert!(ctx.blob.log.contains(&format!(
            "the tie at {{zone {}}} recalls every unit there · {{card {}}}, {{card {DEFENDER}}}",
            fixtures::BF1,
            fixtures::VI
        )));
        assert!(recall_all(&mut ctx, fixtures::BF3).is_empty());
    }

    #[test]
    fn a_tied_combat_where_the_symbols_controller_attacks_recalls_both_sides() {
        let mut fixture = shrine(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Standard,
        );
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.showdown.is_some());
        cleanup::after_combat(&mut ctx, fixtures::BF1, 0);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(DEFENDER),
            Some(Location::Base(1)),
            "the defender is recalled too, so the battlefield is left empty"
        );
        assert!(ctx.blob.holder(fixtures::BF1).is_none());
    }
}
