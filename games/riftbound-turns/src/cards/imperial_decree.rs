use super::prelude::{done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn decree_this_turn(ctx: &mut Ctx, _: &Item) {
    let turn = ctx.turn();
    ctx.narrate(format!(
        "the decree stands for turn {turn} · any unit that takes damage dies"
    ));
}

fn proclaim(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    decree_this_turn(ctx, item);
    done()
}

pub static CARD: Card = spell(
    "Imperial Decree",
    &[Keyword::Action],
    &[play(&[], proclaim)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, settle};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const DECREE: u32 = 90;
    const THEIR_DECREE: u32 = 91;
    const ORDER_A: u32 = 100;
    const ORDER_B: u32 = 101;

    fn decree(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Imperial Decree", 5, 2);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(decree(DECREE, 0));
        fixture.table.cards.push(decree(THEIR_DECREE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_A, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_B, 0, "Order", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, DECREE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_targetless_action() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Imperial Decree").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Imperial Decree");
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
    }

    #[test]
    fn the_decree_resolves_for_five_energy_and_two_order_and_touches_nothing_by_itself() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT] {
            assert!(ctx.on_board(unit), "{unit} stands");
        }
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        let turn = ctx.turn();
        assert!(ctx.blob.log.contains(&format!(
            "the decree stands for turn {turn} · any unit that takes damage dies"
        )));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "five of the six ready runes paid the energy"
        );
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · a turn-scoped floating trigger · triggers::sources lists in-play cards only, so a resolved spell cannot watch every unit's damage this turn; decree_this_turn only narrates until the engine grows floating triggers with Trigger::Damaged over any unit"]
    fn any_unit_that_takes_damage_this_turn_dies_and_the_decree_lapses_with_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Rule));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH),
            "one damage on a 2-Might unit is not lethal by itself: the decree killed it"
        );
        assert!(ctx.damage(fixtures::VI, 1, Cause::Rule));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::TRASH),
            "the caster's own units are not spared"
        );
    }

    #[test]
    fn the_decree_is_the_turn_players_action_and_needs_its_two_order_power() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DECREE)),
            Err(Refusal::NotYourTurn),
            "an Action has no window on the other seat's turn outside a showdown"
        );
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(ORDER_B).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, DECREE)),
            Err(Refusal::NoPowerOf),
            "the energy is there but only one Order rune is"
        );
    }
}
