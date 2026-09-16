use super::prelude::{done, draw, play, spell, with_statics};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const DRAWS: usize = 2;

pub fn enemy_unit_died_this_turn(ctx: &Ctx, seat: u8) -> bool {
    ctx.deaths_this_turn()
        .iter()
        .any(|death| death.unit && death.controller != seat)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if enemy_unit_died_this_turn(ctx, seat) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

fn spoils(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = with_statics(
    spell("Spoils of War", &[Keyword::Reaction], &[play(&[], spoils)]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal, priority};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SPOILS: u32 = 90;
    const BODY_RUNE: u32 = 46;

    fn spoils_card(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(SPOILS, fixtures::HAND, seat, "Spoils of War", 4, 1);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(spoils_card(0));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPOILS).unwrap(),
            &CARD
        ));
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

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_reaction_with_a_self_discount_and_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Spoils of War").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!(DISCOUNT.energy, 2);
        assert_eq!(DRAWS, 2);
    }

    #[test]
    fn at_full_price_it_draws_two_on_resolution() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cost::total(&ctx, SPOILS, false).energy, 4);
        fixtures::play_from_hand(&mut ctx, 0, SPOILS).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy off the four ready runes"
        );
        assert_eq!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Body rune recycles for the power"
        );
        assert_eq!(drew(&ctx, 0), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "one played, two drawn");
        assert_eq!(ctx.card(SPOILS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn three_ready_runes_are_refused_at_full_price() {
        let mut fixture = armed();
        fixture.table.card_mut(43).unwrap().exhausted = true;
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SPOILS)),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            })
        );
    }

    #[test]
    fn the_discount_reads_an_enemy_unit_death_raised_in_the_same_request() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(!enemy_unit_died_this_turn(&ctx, 0));
        ctx.kill(fixtures::VI, Cause::Rule);
        assert!(
            !enemy_unit_died_this_turn(&ctx, 0),
            "a friendly death is not an enemy one"
        );
        assert_eq!(cost::total(&ctx, SPOILS, false).energy, 4);
        ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
        assert!(enemy_unit_died_this_turn(&ctx, 0));
        assert_eq!(cost::total(&ctx, SPOILS, false).energy, 2);
        assert!(
            enemy_unit_died_this_turn(&ctx, 1),
            "from the other seat Vi was the enemy unit"
        );
    }

    #[test]
    fn an_enemy_unit_killed_earlier_this_turn_still_discounts_the_spell_by_two() {
        let mut fixture = armed();
        fixture.table.card_mut(43).unwrap().exhausted = true;
        let table = {
            let mut ctx = fixture.ctx();
            ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
            ctx.table.clone()
        };
        fixture.commit(table);
        let mut ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request carries no events");
        assert_eq!(
            cost::total(&ctx, SPOILS, false).energy,
            2,
            "the death this turn is remembered across requests"
        );
        fixtures::play_from_hand(&mut ctx, 0, SPOILS).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy off three runes");
    }
}
