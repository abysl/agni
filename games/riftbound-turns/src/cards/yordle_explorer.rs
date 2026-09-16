use super::prelude::{done, draw, on_you_play_card, unit, when};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const POWER_AT_LEAST: u8 = 2;
pub const DRAWS: usize = 1;

pub fn spell_with_two_or_more_power_played(_: &Ctx, _: &Event) -> bool {
    false
}

pub fn a_card_costing_two_or_more_power(ctx: &Ctx, event: &Event, _: Source) -> bool {
    match event {
        Event::Played { card, .. } => ctx
            .card(*card)
            .and_then(|face| face.power)
            .is_some_and(|power| power >= POWER_AT_LEAST),
        Event::PlayedSpell { .. } => spell_with_two_or_more_power_played(ctx, event),
        _ => false,
    }
}

fn explore(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = unit(
    "Yordle Explorer",
    &[],
    &[when(
        on_you_play_card(&[], explore),
        a_card_costing_two_or_more_power,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const EXPLORER: u32 = 90;
    const HEAVY_UNIT: u32 = 91;
    const HEAVY_GEAR: u32 = 92;
    const HEAVY_SPELL: u32 = 93;
    const FURY_RUNES: [u32; 3] = [46, 47, 48];

    fn explorer(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: None,
            domain: vec!["Body".into()],
            ..fixtures::unit(EXPLORER, zone, 0, "Yordle Explorer", 4)
        }
    }

    fn heavy_unit() -> CardInfo {
        CardInfo {
            energy: Some(1),
            power: Some(2),
            ..fixtures::unit(HEAVY_UNIT, fixtures::HAND, 0, "Titan", 5)
        }
    }

    fn heavy_gear() -> CardInfo {
        CardInfo {
            power: Some(2),
            ..fixtures::gear(HEAVY_GEAR, fixtures::HAND, 0, "Anvil", 1)
        }
    }

    fn camp(explorer_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(explorer(explorer_zone));
        fixture.table.cards.push(heavy_unit());
        fixture.table.cards.push(heavy_gear());
        fixture.table.cards.push(fixtures::spell(
            HEAVY_SPELL,
            fixtures::HAND,
            0,
            "Spark",
            1,
            2,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in FURY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn her_trigger_is_queued(ctx: &Ctx) -> bool {
        ctx.blob.chain.iter().any(|item| {
            matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == EXPLORER)
        })
    }

    #[test]
    fn the_script_watches_the_cards_its_controller_plays_for_their_power_cost() {
        assert!(std::ptr::eq(script_of("Yordle Explorer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some());
        assert_eq!(POWER_AT_LEAST, 2);
        assert_eq!(DRAWS, 1);
        let fixture = camp(fixtures::BASE);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(EXPLORER).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_condition_reads_the_printed_power_of_a_played_unit_or_gear() {
        let mut fixture = camp(fixtures::BASE);
        let ctx = fixture.ctx();
        let source = Source {
            card: EXPLORER,
            ability: 0,
        };
        let played = |card: u32, kind: &str| Event::Played {
            card,
            controller: 0,
            kind: kind.into(),
            origin: crate::state::Origin::Hand,
            paid_additional: false,
        };
        assert!(a_card_costing_two_or_more_power(
            &ctx,
            &played(HEAVY_UNIT, "Unit"),
            source
        ));
        assert!(a_card_costing_two_or_more_power(
            &ctx,
            &played(HEAVY_GEAR, "Gear"),
            source
        ));
        assert!(
            !a_card_costing_two_or_more_power(&ctx, &played(fixtures::HAND_UNIT, "Unit"), source),
            "no power cost at all"
        );
        assert!(
            !a_card_costing_two_or_more_power(&ctx, &played(EXPLORER, "Unit"), source),
            "her own play has no power cost"
        );
        assert!(
            !a_card_costing_two_or_more_power(
                &ctx,
                &Event::PlayedSpell {
                    item: 1,
                    controller: 0,
                    nth: 1
                },
                source
            ),
            "the spell's card is gone when the event is read · the seam answers no"
        );
        assert!(!a_card_costing_two_or_more_power(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source
        ));
    }

    #[test]
    fn playing_a_two_power_unit_draws_one_when_her_trigger_resolves() {
        let mut fixture = camp(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, HEAVY_UNIT).unwrap();
        assert!(ctx.on_board(HEAVY_UNIT));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(her_trigger_is_queued(&ctx));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the draw waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {EXPLORER}}} · {{seat 0}} draws 1")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_two_power_gear_counts_and_a_card_with_less_power_does_not() {
        let mut fixture = camp(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, HEAVY_GEAR).unwrap();
        assert!(ctx.on_board(HEAVY_GEAR));
        assert!(her_trigger_is_queued(&ctx));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        drop(ctx);

        let mut fixture = camp(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            ctx.blob.chain.is_empty(),
            "a unit without a power cost is not watched"
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn in_hand_she_watches_nothing_and_the_opponents_plays_are_not_yours() {
        let mut fixture = camp(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEAVY_UNIT).unwrap();
        assert!(ctx.blob.chain.is_empty(), "384.1 · only on the board");
        drop(ctx);

        let mut fixture = camp(fixtures::BASE);
        let their = fixture.table.card_mut(HEAVY_UNIT).unwrap();
        their.seat = 1;
        their.owner = 1;
        fixture
            .table
            .cards
            .push(fixtures::rune(49, 1, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(55, 1, "Fury", false));
        fixture.table.card_mut(HEAVY_UNIT).unwrap().energy = Some(0);
        fixture.blob.core_mut().unwrap().advance();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        fixtures::play_from_hand(&mut ctx, 1, HEAVY_UNIT).unwrap();
        assert!(ctx.on_board(HEAVY_UNIT));
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's unit is not a card you play"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · PlayedSpell carries no card: the spell's chain item and card state are gone when the trigger is collected, so a two-power spell is not seen; spell_with_two_or_more_power_played is the seam"]
    fn a_two_power_spell_draws_one_once_it_resolves() {
        let mut fixture = camp(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, HEAVY_SPELL).unwrap();
        resolve_chain(&mut ctx);
        assert!(her_trigger_is_queued(&ctx));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
    }
}
