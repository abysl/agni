use super::prelude::{adding, legend, legion, ONE_ENERGY};
use super::{Adds, Card, Cost, Paying};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = ONE_ENERGY;

pub static CARD: Card = adding(legend("Darius - Hand of Noxus", &[], &[]), adds);

pub fn adds_while_paying(ctx: &Ctx, seat: u8, legend: u32) -> Option<Cost> {
    let ready = ctx.card(legend).is_some_and(|held| !held.exhausted);
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    (ready && mine && legion(ctx, seat)).then_some(ADDS)
}

fn adds(ctx: &Ctx, seat: u8, legend: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, legend).map(Adds::exhausting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Cost as Total;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;

    const DARIUS: u32 = fixtures::LEGEND_CARD;

    fn noxus() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(DARIUS).unwrap().name = CARD.name.into();
        fixture.resolve();
        fixture
    }

    fn with_one_ready_rune(mut fixture: Fixture) -> Fixture {
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 {
                card.exhausted = card.id != 41;
            }
        }
        fixture.resolve();
        fixture
    }

    fn energy(amount: u8) -> Total {
        Total {
            energy: amount,
            power: Vec::new(),
            ..Total::default()
        }
    }

    fn exhausted(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Annotate {
                    card,
                    key,
                    value: Some(_),
                } if key == "exhausted" => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_legend_is_a_legion_gated_add_source_that_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(
            CARD.keywords.is_empty(),
            "Legion gates the ability, not the card"
        );
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 1);
        assert!(ADDS.power.is_empty());
        let mut fixture = noxus();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(DARIUS).unwrap(), &CARD));
        assert!(ctx.is_legend(DARIUS));
        assert_eq!(
            activate::legal(&ctx, 0, DARIUS, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
    }

    #[test]
    fn he_adds_one_energy_once_a_card_has_been_played_this_turn_and_only_while_ready() {
        let mut fixture = noxus();
        let mut ctx = fixture.ctx();
        assert!(!legion(&ctx, 0));
        assert_eq!(
            adds_while_paying(&ctx, 0, DARIUS),
            None,
            "812.1.b.1 · nothing played yet, so no Legion"
        );
        ctx.note_played(0);
        assert!(legion(&ctx, 0));
        assert_eq!(adds_while_paying(&ctx, 0, DARIUS), Some(ADDS));
        assert_eq!(
            adds_while_paying(&ctx, 1, DARIUS),
            None,
            "the legend is seat 0's"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, fixtures::VI),
            None,
            "a unit is no legend"
        );
        ctx.exhaust(DARIUS);
        assert_eq!(adds_while_paying(&ctx, 0, DARIUS), None);
    }

    #[test]
    fn playing_a_card_satisfies_legion_for_the_rest_of_the_turn() {
        let mut fixture = noxus();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(legion(&ctx, 0), "gear is a card played this turn");
        assert_eq!(adds_while_paying(&ctx, 0, DARIUS), Some(ADDS));
        assert_eq!(
            adds_while_paying(&ctx, 1, DARIUS),
            None,
            "seat 1 played nothing and owns no Darius"
        );
    }

    #[test]
    fn with_legion_a_ready_darius_stands_in_for_one_energy_and_pays_by_exhausting() {
        let mut fixture = with_one_ready_rune(noxus());
        let mut ctx = fixture.ctx();
        let two = energy(2);
        assert!(
            pay::plan(&ctx, 0, &two).is_err(),
            "one rune and no Legion · two energy is out of reach"
        );
        ctx.note_played(0);
        let planned = pay::plan(&ctx, 0, &two).expect("Darius adds the missing energy");
        pay::pay(&mut ctx, 0, &planned);
        assert_eq!(
            exhausted(&ctx),
            [DARIUS, 41],
            "the rune and Darius exhaust together"
        );
        assert_eq!(adds_while_paying(&ctx, 0, DARIUS), None);
        assert!(
            pay::plan(&ctx, 0, &energy(1)).is_err(),
            "an exhausted Darius adds nothing more this turn"
        );
    }
}
