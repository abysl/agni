use super::prelude::unit;
use super::{Card, Keyword};
use crate::engine::ctx::Ctx;

pub fn grants_quick_draw_in_hand(ctx: &Ctx, me: u32, card: u32) -> bool {
    ctx.on_board(me)
        && !ctx.is_facedown(me)
        && ctx.in_hand(card)
        && ctx.controller(card) == ctx.controller(me)
        && ctx.is_gear(card)
        && ctx.script(card).is_some_and(|script| script.is_equipment())
}

pub fn equipment_in_hand_with_quick_draw(ctx: &Ctx, me: u32) -> Vec<u32> {
    ctx.hand_of(ctx.controller(me))
        .into_iter()
        .filter(|card| grants_quick_draw_in_hand(ctx, me, *card))
        .collect()
}

pub static CARD: Card = unit("Jax - Unmatched", &[Keyword::Deflect(1)], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, equip, gear, while_attached, with_statics, RAINBOW};
    use crate::cards::{script_of, Grant};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{hide, legal};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const JAX: u32 = 90;
    const SWORD: u32 = 91;
    const THEIR_SWORD: u32 = 92;
    const TRINKET: u32 = 93;

    static SWORD_CARD: Card = with_statics(
        gear("Sword", &[Keyword::Equip(RAINBOW)], &[equip(RAINBOW)]),
        &[while_attached(&[Grant::Might(2)])],
    );

    static TRINKET_CARD: Card = gear("Trinket", &[], &[]);

    fn jax(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(JAX, zone, 0, "Jax - Unmatched", 5)
        }
    }

    fn armory(jax_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jax(jax_zone));
        let mut sword = fixtures::gear(SWORD, fixtures::HAND, 0, "Sword", 0);
        sword.power = Some(0);
        fixture.table.cards.push(sword);
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_SWORD, fixtures::HAND, 1, "Sword", 0));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::HAND, 0, "Trinket", 0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(SWORD, &SWORD_CARD)
            .with_script(THEIR_SWORD, &SWORD_CARD)
            .with_script(TRINKET, &TRINKET_CARD);
        fixture
    }

    #[test]
    fn the_stub_carries_deflect_and_the_hand_grant_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Jax - Unmatched").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        let mut fixture = armory(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(JAX).unwrap(), &CARD));
        assert_eq!(ctx.deflect_of(JAX), 1);
    }

    #[test]
    fn the_grant_reads_your_equipment_in_hand_while_he_is_on_the_board() {
        let mut fixture = armory(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(grants_quick_draw_in_hand(&ctx, JAX, SWORD));
        assert!(
            !grants_quick_draw_in_hand(&ctx, JAX, TRINKET),
            "a gear without Equip is not an Equipment"
        );
        assert!(
            !grants_quick_draw_in_hand(&ctx, JAX, THEIR_SWORD),
            "the opponent's hand is not yours"
        );
        assert!(
            !grants_quick_draw_in_hand(&ctx, JAX, fixtures::HAND_UNIT),
            "a unit is not gear"
        );
        assert_eq!(
            equipment_in_hand_with_quick_draw(&ctx, JAX),
            [fixtures::HAND_GEAR, SWORD],
            "the Boots in hand are an Equipment too"
        );
        drop(ctx);

        let mut fixture = armory(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(
            !grants_quick_draw_in_hand(&ctx, JAX, SWORD),
            "384.1 · in hand he grants nothing"
        );
        assert!(equipment_in_hand_with_quick_draw(&ctx, JAX).is_empty());
    }

    #[test]
    fn today_the_sword_in_hand_has_no_reaction_timing_and_is_refused_on_a_closed_chain() {
        let mut fixture = armory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(!hide::reacts(&ctx, SWORD));
        assert!(!ctx.has_keyword(SWORD, Keyword::QuickDraw));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        let entry = crate::engine::ctx::EntryMove {
            card: SWORD,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert!(legal::classify(&ctx, 0, &entry).is_err());
        assert!(ctx.in_hand(SWORD));
    }

    #[test]
    #[ignore = "engine gap · Quick-Draw and a hand-zone static: statics::grants_on reads in-play cards only and nothing reads Keyword::QuickDraw; with a hand grant consulted by hide::reacts / legal::timing and play::finalize attaching the gear as it resolves, the Sword plays at Reaction timing over a spell and lands on a unit"]
    fn with_him_on_the_board_an_equipment_in_hand_plays_as_a_reaction_and_attaches() {
        let mut fixture = armory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(hide::reacts(&ctx, SWORD));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, SWORD).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|item| item.kind),
            Some(ItemKind::Permanent { card }) if card == SWORD
        ));
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {JAX}}}")).unwrap();
        assert_eq!(attached_to(&ctx, SWORD), Some(JAX));
    }
}
