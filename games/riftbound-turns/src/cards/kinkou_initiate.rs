use super::prelude::{done, draw, friendly_units, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const TOTAL_MIGHT: i32 = 5;

pub fn other_units_might(ctx: &Ctx, seat: u8, me: u32) -> i32 {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| *unit != me)
        .map(|unit| ctx.current_might(unit))
        .sum()
}

fn initiate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    let total = other_units_might(ctx, seat, me);
    if total < TOTAL_MIGHT {
        ctx.narrate(format!(
            "{{card {me}}} · your other units total {total} Might · nothing is drawn"
        ));
        return done();
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {me}}} · your other units total {total} Might · {{seat {seat}}} draws {drawn}"
    ));
    done()
}

pub static CARD: Card = unit("Kinkou Initiate", &[], &[play(&[], initiate)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, settle};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const INITIATE: u32 = 90;
    const ALLY: u32 = 91;
    const BODY_RUNE: u32 = 46;

    fn initiate_card(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(0),
            domain: vec!["Body".into()],
            ..fixtures::unit(INITIATE, zone, seat, "Kinkou Initiate", 3)
        }
    }

    fn dojo(ally_might: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(initiate_card(fixtures::HAND, 0));
        if let Some(might) = ally_might {
            fixture
                .table
                .cards
                .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", might));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn enter(ctx: &mut Ctx) {
        play_engine::begin(ctx, 0, INITIATE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == INITIATE
        ));
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_untargeted_play_trigger() {
        assert!(std::ptr::eq(script_of("Kinkou Initiate").unwrap(), &CARD));
        assert_eq!(CARD.name, "Kinkou Initiate");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.condition.is_none(), "the if is read as it resolves");
        assert!(ability.targets.is_empty());
        assert_eq!((DRAWS, TOTAL_MIGHT), (1, 5));
    }

    #[test]
    fn the_total_counts_every_other_friendly_unit_by_current_might_and_never_the_initiate() {
        let mut fixture = dojo(Some(2));
        fixture.table.card_mut(INITIATE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            other_units_might(&ctx, 0, INITIATE),
            5,
            "Vi 3 and the ally 2"
        );
        assert_eq!(
            other_units_might(&ctx, 1, fixtures::THEIR_UNIT),
            3,
            "the Sprite"
        );
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.might(ALLY, -1, until, None, 0);
        assert_eq!(
            other_units_might(&ctx, 0, INITIATE),
            4,
            "current Might, not printed"
        );
    }

    #[test]
    fn with_five_might_among_the_others_the_initiate_draws_one_as_the_trigger_resolves() {
        let mut fixture = dojo(Some(2));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        enter(&mut ctx);
        assert_eq!(draws(&ctx), 1);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the initiate left the hand and the draw replaced him"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {INITIATE}}} · your other units total 5 Might · {{seat 0}} draws 1"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_less_than_five_nothing_is_drawn_and_the_initiates_own_might_does_not_count() {
        let mut fixture = dojo(Some(1));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        enter(&mut ctx);
        assert_eq!(draws(&ctx), 0);
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "only the initiate left");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {INITIATE}}} · your other units total 4 Might · nothing is drawn"
        )));
        drop(ctx);
        let mut alone = dojo(None);
        alone.table.card_mut(fixtures::VI).unwrap().might = Some(4);
        alone.resolve();
        let mut ctx = alone.ctx();
        enter(&mut ctx);
        assert_eq!(
            draws(&ctx),
            0,
            "Vi's 4 and the initiate's own 3 are not 5 others"
        );
    }

    #[test]
    fn the_condition_is_read_as_the_trigger_resolves_not_as_it_is_played() {
        let mut fixture = dojo(Some(2));
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 0, INITIATE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.table.card_mut(ALLY).unwrap().zone = Some(fixtures::TRASH);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 0, "the ally left before the trigger resolved");
    }
}
