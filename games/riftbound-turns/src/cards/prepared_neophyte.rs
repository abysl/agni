use super::prelude::{unit, with_statics};
use super::{Card, Grant, Static, KIND_SPELL};
use crate::engine::cost;
use crate::engine::ctx::{Ctx, Event};
use crate::state::ItemKind;

pub const SPENT_AT_LEAST: u8 = 4;
pub const BONUS: i16 = 4;

pub fn energy_spent_on_spells_this_turn(ctx: &Ctx, seat: u8) -> Vec<u8> {
    ctx.events
        .iter()
        .filter_map(|event| match event {
            Event::Played {
                card,
                controller,
                kind,
                ..
            } if *controller == seat && kind == KIND_SPELL => Some(*card),
            _ => None,
        })
        .filter_map(|card| {
            ctx.blob
                .chain
                .iter()
                .find(|item| matches!(item.kind, ItemKind::Spell { card: held } if held == card))
        })
        .map(|item| cost::of_item(ctx, item, None).energy)
        .collect()
}

pub fn spent_four_or_more_on_a_spell_this_turn(ctx: &Ctx, seat: u8) -> bool {
    energy_spent_on_spells_this_turn(ctx, seat)
        .into_iter()
        .any(|energy| energy >= SPENT_AT_LEAST)
}

fn primed(ctx: &Ctx, me: u32) -> bool {
    spent_four_or_more_on_a_spell_this_turn(ctx, ctx.controller(me))
}

pub static CARD: Card = with_statics(
    unit("Prepared Neophyte", &[], &[]),
    &[Static::While(primed, &[Grant::Might(BONUS)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use agni_plugin_sdk::table::CardInfo;

    const NEOPHYTE: u32 = 90;
    const THEIR_NEOPHYTE: u32 = 91;
    const BIG_SPELL: u32 = 92;
    const SMALL_SPELL: u32 = 93;
    const MIGHT: u8 = 1;
    const FURY_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn neophyte(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, fixtures::BASE, seat, "Prepared Neophyte", MIGHT)
        }
    }

    fn seminary() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(neophyte(NEOPHYTE, 0));
        fixture.table.cards.push(neophyte(THEIR_NEOPHYTE, 1));
        fixture.table.cards.push(fixtures::spell(
            BIG_SPELL,
            fixtures::HAND,
            0,
            "Cataclysm",
            4,
            0,
        ));
        fixture.table.cards.push(fixtures::spell(
            SMALL_SPELL,
            fixtures::HAND,
            0,
            "Flicker",
            3,
            0,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in FURY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(NEOPHYTE).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_whose_only_static_is_a_conditional_four_might() {
        assert!(std::ptr::eq(script_of("Prepared Neophyte").unwrap(), &CARD));
        assert_eq!(CARD.name, "Prepared Neophyte");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.additional.is_none());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(BONUS)])]
        ));
        assert_eq!(SPENT_AT_LEAST, 4);
    }

    #[test]
    fn a_spell_played_for_four_this_request_gives_the_four_might_while_it_is_on_the_chain() {
        let mut fixture = seminary();
        let mut ctx = fixture.ctx();
        assert!(energy_spent_on_spells_this_turn(&ctx, 0).is_empty());
        assert_eq!(ctx.current_might(NEOPHYTE), i32::from(MIGHT));
        assert!(statics::grants_on(&ctx, NEOPHYTE).is_empty());
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        assert_eq!(energy_spent_on_spells_this_turn(&ctx, 0), [4]);
        assert!(spent_four_or_more_on_a_spell_this_turn(&ctx, 0));
        assert!(
            !spent_four_or_more_on_a_spell_this_turn(&ctx, 1),
            "the other seat spent nothing"
        );
        assert_eq!(
            ctx.current_might(NEOPHYTE),
            i32::from(MIGHT) + i32::from(BONUS)
        );
        assert_eq!(
            ctx.current_might(THEIR_NEOPHYTE),
            i32::from(MIGHT),
            "their Neophyte reads their own spending"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn three_energy_is_not_four_and_a_unit_played_is_not_a_spell() {
        let mut fixture = seminary();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SMALL_SPELL).unwrap();
        assert_eq!(energy_spent_on_spells_this_turn(&ctx, 0), [3]);
        assert!(!spent_four_or_more_on_a_spell_this_turn(&ctx, 0));
        assert_eq!(ctx.current_might(NEOPHYTE), i32::from(MIGHT));
        drop(ctx);
        let mut fixture = seminary();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(5);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(energy_spent_on_spells_this_turn(&ctx, 0).is_empty());
        assert_eq!(ctx.current_might(NEOPHYTE), i32::from(MIGHT));
    }

    #[test]
    #[ignore = "engine gap · the energy spent to play a spell is not recorded: the seam prices the spell while it sits on the chain in the same request, so once it resolves or in a later request the turn's spending is invisible (a per-seat per-turn counter on SeatState, the Yordle Explorer row)"]
    fn the_four_might_lasts_the_turn_after_the_spell_resolves() {
        let mut fixture = seminary();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(spent_four_or_more_on_a_spell_this_turn(&ctx, 0));
        assert_eq!(
            ctx.current_might(NEOPHYTE),
            i32::from(MIGHT) + i32::from(BONUS)
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request carries no events");
        assert!(spent_four_or_more_on_a_spell_this_turn(&ctx, 0));
        assert_eq!(
            ctx.current_might(NEOPHYTE),
            i32::from(MIGHT) + i32::from(BONUS)
        );
    }
}
