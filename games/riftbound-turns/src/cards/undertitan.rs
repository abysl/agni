use super::prelude::{done, friendly_units, might_this_turn, play, unit};
use super::{Card, Cost, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::FLAG_REVEALING;

pub const MIGHT: i16 = 2;
pub const ADDS: Cost = Cost {
    energy: 2,
    power: &[],
};

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let others: Vec<u32> = friendly_units(ctx, item.controller)
        .into_iter()
        .filter(|unit| *unit != me)
        .collect();
    for unit in others {
        might_this_turn(ctx, item, unit, MIGHT, None);
    }
    done()
}

fn in_main_deck(ctx: &Ctx, card: u32) -> bool {
    ctx.zones.main_deck.is_some()
        && ctx
            .card(card)
            .is_some_and(|held| held.zone == ctx.zones.main_deck)
}

pub fn adds_as_revealed_from_deck(ctx: &Ctx, seat: u8, card: u32) -> Option<Cost> {
    let titan = ctx
        .script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD));
    let from_the_deck = in_main_deck(ctx, card) || ctx.has_flag(card, FLAG_REVEALING);
    (titan && ctx.owner(card) == seat && from_the_deck).then_some(ADDS)
}

pub static CARD: Card = unit("Undertitan", &[], &[play(&[], rally)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{pay, priority};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const TITAN: u32 = 90;
    const ALLY: u32 = 91;
    const ORDER_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn titan(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(TITAN, zone, seat, "Undertitan", 5)
        }
    }

    fn rift(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(titan(zone, 0));
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Scout", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn land(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, TITAN).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(ctx, 0, "your base").unwrap();
        }
    }

    #[test]
    fn the_script_is_a_plain_unit_with_an_untargeted_play_trigger_and_a_two_energy_add() {
        assert!(std::ptr::eq(script_of("Undertitan").unwrap(), &CARD));
        assert_eq!(CARD.name, "Undertitan");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert_eq!(MIGHT, 2);
        assert_eq!(ADDS.energy, 2);
        assert!(ADDS.power.is_empty());
    }

    #[test]
    fn playing_it_gives_every_other_friendly_unit_two_might_for_the_turn() {
        let mut fixture = rift(fixtures::HAND);
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        assert_eq!(ctx.location(TITAN), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == TITAN
        ));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the buff waits for the trigger"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            ctx.current_might(ALLY),
            4,
            "at a battlefield or in base alike"
        );
        assert_eq!(ctx.current_might(TITAN), 5, "your other units · not itself");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "not the enemy's"
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(ALLY), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_add_reads_only_our_own_undertitan_in_or_leaving_our_deck() {
        let mut fixture = rift(fixtures::MAIN_DECK);
        let mut ctx = fixture.ctx();
        assert_eq!(adds_as_revealed_from_deck(&ctx, 0, TITAN), Some(ADDS));
        assert_eq!(
            adds_as_revealed_from_deck(&ctx, 1, TITAN),
            None,
            "the other seat's reveal adds nothing for us"
        );
        assert_eq!(
            adds_as_revealed_from_deck(&ctx, 0, fixtures::VI),
            None,
            "a unit that is not an Undertitan"
        );
        assert_eq!(ctx.reveal_top(0), Some(TITAN));
        assert!(ctx.has_flag(TITAN, FLAG_REVEALING));
        assert_eq!(
            adds_as_revealed_from_deck(&ctx, 0, TITAN),
            Some(ADDS),
            "mid-reveal it still reads as revealed from the deck"
        );
        let mut fixture = rift(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(
            adds_as_revealed_from_deck(&ctx, 0, TITAN),
            None,
            "a hand card is not revealed from the deck"
        );
        let mut fixture = rift(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(adds_as_revealed_from_deck(&ctx, 0, TITAN), None);
    }

    #[test]
    #[ignore = "engine gap · [Add] as a replacement on a reveal from the deck (370.1.b.1, 429): Ctx::reveal_top raises no Revealed hook and pay::plan knows only runes and the Gold token, so the two energy adds_as_revealed_from_deck returns are never put in the rune pool"]
    fn revealing_it_from_the_deck_adds_two_energy_to_pay_with() {
        let mut fixture = rift(fixtures::MAIN_DECK);
        for rune in [41, 43].into_iter().chain(ORDER_RUNES) {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let three = cost::Cost {
            energy: 3,
            ..cost::Cost::default()
        };
        assert!(
            !pay::affordable(&ctx, 0, &three),
            "one ready rune cannot pay three"
        );
        assert_eq!(ctx.reveal_top(0), Some(TITAN));
        assert!(
            pay::affordable(&ctx, 0, &three),
            "the reveal added two energy beside the ready rune"
        );
    }
}
