use super::prelude::{
    done, gear, might_this_turn, on_friendly_unit_readied, trigger_subject, when,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const BONUS: i16 = 1;

pub fn readied_friendly_unit(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Readied { card, by } = event else {
        return false;
    };
    let seat = ctx.controller(source.card);
    *by == seat && ctx.is_unit(*card) && ctx.controller(*card) == seat && ctx.on_board(*card)
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = trigger_subject(item) else {
        return done();
    };
    if ctx.is_unit(unit) && ctx.on_board(unit) {
        might_this_turn(ctx, item, unit, BONUS, None);
        ctx.narrate(format!("{{card {unit}}} gets +{BONUS} Might this turn"));
    }
    done()
}

pub static CARD: Card = gear(
    "Pirate's Haven",
    &[],
    &[when(
        on_friendly_unit_readied(&[], rally),
        readied_friendly_unit,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, settle};
    use agni_plugin_sdk::table::Target;

    const HAVEN: u32 = 90;
    const DECKHAND: u32 = 91;

    fn cove() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut haven = fixtures::gear(HAVEN, fixtures::BASE, 0, "Pirate's Haven", 3);
        haven.domain = vec!["Body".into()];
        haven.exhausted = true;
        fixture.table.cards.push(haven);
        let mut deckhand = fixtures::unit(DECKHAND, fixtures::BASE, 0, "Deckhand", 2);
        deckhand.exhausted = true;
        fixture.table.cards.push(deckhand);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn quiet(ctx: &Ctx) -> bool {
        ctx.blob.prompt.is_none() && ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty()
    }

    #[test]
    fn the_script_is_a_readied_trigger_gated_on_the_readied_friendly_unit_seam() {
        let fixture = cove();
        assert!(std::ptr::eq(fixture.scripts.of_card(HAVEN).unwrap(), &CARD));
        assert_eq!(CARD.name, "Pirate's Haven");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Readied(Who::Friendly));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    #[test]
    fn the_seam_accepts_a_friendly_unit_readied_by_its_controller_and_nothing_else() {
        let mut fixture = cove();
        let ctx = fixture.ctx();
        let source = Source {
            card: HAVEN,
            ability: 0,
        };
        assert!(readied_friendly_unit(
            &ctx,
            &Event::Readied {
                card: DECKHAND,
                by: 0
            },
            source
        ));
        assert!(
            !readied_friendly_unit(
                &ctx,
                &Event::Readied {
                    card: DECKHAND,
                    by: 1
                },
                source
            ),
            "the opponent readying your unit is not you readying it"
        );
        assert!(
            !readied_friendly_unit(
                &ctx,
                &Event::Readied {
                    card: fixtures::THEIR_UNIT,
                    by: 0
                },
                source
            ),
            "an enemy unit is not friendly"
        );
        assert!(
            !readied_friendly_unit(&ctx, &Event::Readied { card: HAVEN, by: 0 }, source),
            "the haven readying itself is not a unit"
        );
        assert!(
            !readied_friendly_unit(
                &ctx,
                &Event::Readied {
                    card: fixtures::RUNE_A,
                    by: 0
                },
                source
            ),
            "a rune is not a unit"
        );
    }

    #[test]
    fn the_awaken_step_readies_a_friendly_unit_but_the_havens_own_readying_is_not_a_unit() {
        let mut fixture = cove();
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.turn_player(), 1);
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.turn_player(), 0);
        assert!(!ctx.card(HAVEN).unwrap().exhausted);
        assert!(!ctx.card(DECKHAND).unwrap().exhausted);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "415.3.a: readying at the awaken step is you readying, and only Deckhand is a unit"
        );
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(quiet(&ctx));
        assert_eq!(might_counter(&ctx, DECKHAND), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn readying_a_friendly_unit_gives_it_one_might_this_turn() {
        let mut fixture = cove();
        let mut ctx = fixture.ctx();
        ctx.ready(DECKHAND);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the haven's trigger waits on the chain"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, DECKHAND), 1);
        assert_eq!(ctx.current_might(DECKHAND), 3);
        ctx.ready(fixtures::THEIR_UNIT);
        settle(&mut ctx).unwrap();
        assert!(quiet(&ctx), "an enemy unit readying is not yours");
    }
}
