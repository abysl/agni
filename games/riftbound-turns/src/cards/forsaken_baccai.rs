use super::prelude::{done, might_this_turn, triggered, unit, when};
use super::{Card, Event, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;

pub fn fewer_runes_than_an_opponent(ctx: &Ctx, seat: u8) -> bool {
    let mine = ctx.runes_of(seat).len();
    (0..ctx.players())
        .filter(|other| *other != seat)
        .any(|other| ctx.runes_of(other).len() > mine)
}

pub fn outnumbered(ctx: &Ctx, _: &Event, source: Source) -> bool {
    fewer_runes_than_an_opponent(ctx, ctx.controller(source.card))
}

fn desperation(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!("{{card {me}}} has left the board · no Might"));
        return done();
    }
    might_this_turn(ctx, item, me, MIGHT, None);
    ctx.narrate(format!("{{card {me}}} gets +{MIGHT} this turn"));
    done()
}

pub static CARD: Card = unit(
    "Forsaken Baccai",
    &[],
    &[when(
        triggered(Trigger::BeginningPhase, &[], desperation),
        outnumbered,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority};
    use crate::state::{ItemKind, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const BACCAI: u32 = 90;
    const THEIR_EXTRA_RUNES: [u32; 3] = [46, 47, 48];

    fn baccai(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Fury".into()],
            ..fixtures::unit(BACCAI, zone, seat, "Forsaken Baccai", 2)
        }
    }

    fn oasis(their_extra_runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(baccai(fixtures::BASE, 0));
        for rune in THEIR_EXTRA_RUNES.iter().take(their_extra_runes) {
            fixture
                .table
                .cards
                .push(fixtures::rune(*rune, 1, "Mind", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BACCAI).unwrap(),
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

    fn baccai_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(
                |item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == BACCAI),
            )
            .count()
    }

    #[test]
    fn the_script_is_one_conditional_beginning_phase_trigger_with_no_targets() {
        assert!(std::ptr::eq(script_of("Forsaken Baccai").unwrap(), &CARD));
        assert_eq!(CARD.name, "Forsaken Baccai");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some(), "fewer runes than an opponent");
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(MIGHT, 1);
    }

    #[test]
    fn the_condition_counts_runes_controlled_ready_or_not_against_every_opponent() {
        let mut fixture = oasis(0);
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(ctx.runes_of(1).len(), 2);
        assert!(
            !fewer_runes_than_an_opponent(&ctx, 0),
            "four is not fewer than two"
        );
        assert!(fewer_runes_than_an_opponent(&ctx, 1));
        drop(ctx);
        let mut fixture = oasis(2);
        let ctx = fixture.ctx();
        assert!(
            !fewer_runes_than_an_opponent(&ctx, 0),
            "four against four is not fewer"
        );
        drop(ctx);
        let mut fixture = oasis(3);
        let ctx = fixture.ctx();
        assert!(fewer_runes_than_an_opponent(&ctx, 0));
        let source = Source {
            card: BACCAI,
            ability: 0,
        };
        assert!(outnumbered(
            &ctx,
            &Event::BeginningPhase { seat: 0 },
            source
        ));
        assert!(
            ctx.runes_of(0).iter().any(|rune| rune.exhausted),
            "an exhausted rune is still controlled"
        );
    }

    #[test]
    fn outnumbered_at_the_start_of_his_beginning_phase_he_gets_one_might_this_turn() {
        let mut fixture = oasis(3);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(baccai_items(&ctx), 1);
        assert_eq!(
            ctx.runes_of(0).len(),
            4,
            "the trigger is evaluated before the Channel Phase"
        );
        assert_eq!(ctx.current_might(BACCAI), 2, "nothing until it resolves");
        resolve_the_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BACCAI), 3);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BACCAI}}} gets +1 this turn")));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            ctx.table.held(fixtures::RUNE_POOL, 0).count(),
            6,
            "the Channel Phase followed"
        );
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(BACCAI), 2, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_as_many_runes_as_the_opponent_nothing_triggers_and_their_turn_is_not_his() {
        let mut fixture = oasis(2);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(baccai_items(&ctx), 0);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BACCAI), 2);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        drop(ctx);
        let mut fixture = oasis(3);
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(
            baccai_items(&ctx),
            0,
            "the opponent's Beginning Phase is not yours"
        );
        assert_eq!(ctx.current_might(BACCAI), 2);
    }
}
