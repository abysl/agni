use super::prelude::{done, might_this_turn, ready, unit};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;

pub fn you_discarded_until_a_discarded_event_exists(ctx: &Ctx, by: u8, source: Source) -> bool {
    ctx.on_board(source.card) && by == ctx.controller(source.card)
}

pub fn rebel(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!(
            "{{card {me}}} is not on the board · nothing to ready"
        ));
        return done();
    }
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    might_this_turn(ctx, item, me, MIGHT, None);
    ctx.narrate(format!("{{card {me}}} gets +{MIGHT} this turn"));
    done()
}

pub static CARD: Card = unit("Jinx - Rebel", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const JINX: u32 = 90;

    fn jinx(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        let mut card = fixtures::unit(JINX, zone, seat, "Jinx - Rebel", 5);
        card.energy = Some(5);
        card.power = Some(1);
        card.domain = vec!["Chaos".into()];
        card.exhausted = exhausted;
        card
    }

    fn rebellious(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jinx(zone, 0, true));
        fixture.resolve();
        fixture
    }

    fn source() -> Source {
        Source {
            card: JINX,
            ability: 0,
        }
    }

    fn trigger() -> Item {
        Item::new(
            9,
            ItemKind::Trigger {
                source: JINX,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_stub_is_the_pool_name_with_no_keywords_and_the_discard_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Jinx - Rebel").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(MIGHT, 1);
        let fixture = rebellious(fixtures::BASE);
        assert!(std::ptr::eq(fixture.scripts.of_card(JINX).unwrap(), &CARD));
    }

    #[test]
    fn the_condition_reads_a_discard_by_her_controller_while_she_stands() {
        let mut fixture = rebellious(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(you_discarded_until_a_discarded_event_exists(
            &ctx,
            0,
            source()
        ));
        assert!(
            !you_discarded_until_a_discarded_event_exists(&ctx, 1, source()),
            "an opponent's discard is not yours"
        );
        drop(ctx);
        let mut fixture = rebellious(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(
            !you_discarded_until_a_discarded_event_exists(&ctx, 0, source()),
            "in hand she watches nothing"
        );
    }

    #[test]
    fn the_effect_readies_her_and_gives_one_might_until_the_end_of_the_turn() {
        let mut fixture = rebellious(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(ctx.card(JINX).unwrap().exhausted);
        assert_eq!(ctx.current_might(JINX), 5);
        assert_eq!(rebel(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(!ctx.card(JINX).unwrap().exhausted);
        assert!(ctx.events.contains(&Event::Readied { card: JINX, by: 0 }));
        assert_eq!(ctx.current_might(JINX), 5 + i32::from(MIGHT));
        assert!(ctx.blob.log.contains(&format!("{{card {JINX}}} readies")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {JINX}}} gets +1 this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(JINX), 5, "gone with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_jinx_keeps_the_might_and_readies_nothing_twice() {
        let mut fixture = rebellious(fixtures::BASE);
        fixture.table.card_mut(JINX).unwrap().exhausted = false;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(rebel(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
        assert_eq!(ctx.current_might(JINX), 6);
        assert!(!ctx.blob.log.contains(&format!("{{card {JINX}}} readies")));
    }

    #[test]
    fn the_effect_refuses_a_jinx_that_is_not_on_the_board() {
        let mut fixture = rebellious(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        assert_eq!(rebel(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(ctx.effects.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {JINX}}} is not on the board · nothing to ready"
        )));
    }

    #[test]
    #[ignore = "engine gap · no Discarded event reaches triggers::matches; with it the script is triggered(Discarded(Who::You), &[], rebel) and one trigger covers a batch of discards"]
    fn a_discard_by_her_controller_readies_her_through_the_chain() {
        let mut fixture = rebellious(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.trash(fixtures::HAND_UNIT);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(JINX).unwrap().exhausted);
        assert_eq!(ctx.current_might(JINX), 6);
    }
}
