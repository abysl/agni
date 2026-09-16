use super::prelude::{done, might_this_turn, on_readied, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 2;

fn fret(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!("{{card {me}}} has left the board · no Might"));
        return done();
    }
    might_this_turn(ctx, item, me, MIGHT, None);
    ctx.narrate(format!("{{card {me}}} gets +{MIGHT} this turn"));
    done()
}

pub static CARD: Card = unit("Fretful Feline", &[], &[on_readied(&[], fret)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, settle};
    use crate::state::{ItemKind, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const FELINE: u32 = 90;
    const THEIR_CAT: u32 = 91;

    fn feline(id: u32, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::unit(id, fixtures::BASE, seat, "Fretful Feline", 5)
        }
    }

    fn alley(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(feline(FELINE, 0, exhausted));
        fixture.table.cards.push(feline(THEIR_CAT, 1, true));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FELINE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn feline_items(ctx: &Ctx, source: u32) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source: held, .. } if held == source))
            .count()
    }

    #[test]
    fn the_script_is_one_untargeted_readied_trigger() {
        assert!(std::ptr::eq(script_of("Fretful Feline").unwrap(), &CARD));
        assert_eq!(CARD.name, "Fretful Feline");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Readied(Who::Me));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert!(!ability.optional);
        assert_eq!(MIGHT, 2);
    }

    #[test]
    fn the_awaken_step_readies_her_and_she_is_seven_might_for_the_turn() {
        let mut fixture = alley(true);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert!(!ctx.card(FELINE).unwrap().exhausted);
        assert!(ctx.events.contains(&Event::Readied {
            card: FELINE,
            by: 0
        }));
        assert_eq!(feline_items(&ctx, FELINE), 1);
        assert_eq!(
            feline_items(&ctx, THEIR_CAT),
            0,
            "the opponent's cat stays exhausted on your turn"
        );
        assert!(ctx.card(THEIR_CAT).unwrap().exhausted);
        assert_eq!(ctx.current_might(FELINE), 5, "nothing until it resolves");
        resolve_the_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(FELINE), 7);
        assert_eq!(ctx.current_might(THEIR_CAT), 5);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FELINE}}} gets +2 this turn")));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(FELINE), 5, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_effect_readying_her_mid_turn_fires_it_too() {
        let mut fixture = alley(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.ready(FELINE));
        settle(&mut ctx).unwrap();
        assert_eq!(feline_items(&ctx, FELINE), 1);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.current_might(FELINE), 7);
        assert!(
            ctx.ready(THEIR_CAT),
            "readying the enemy cat is its trigger"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(feline_items(&ctx, THEIR_CAT), 1);
        assert_eq!(feline_items(&ctx, FELINE), 0);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.current_might(THEIR_CAT), 7);
        assert_eq!(
            ctx.current_might(FELINE),
            7,
            "hers does not stack on theirs"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_cat_that_is_already_ready_does_not_become_ready() {
        let mut fixture = alley(false);
        let mut ctx = fixture.ctx();
        assert!(!ctx.ready(FELINE), "415 · ready is a change of state");
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, .. } if *card == FELINE)));
        phases::start_turn(&mut ctx);
        assert_eq!(feline_items(&ctx, FELINE), 0, "nothing to awaken");
        assert_eq!(ctx.current_might(FELINE), 5);
    }
}
