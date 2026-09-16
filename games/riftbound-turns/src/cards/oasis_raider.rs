use super::forsaken_baccai::outnumbered;
use super::prelude::{done, grant_this_turn, might_this_turn, triggered, unit, when};
use super::{Card, Flow, Item, Keyword, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 2;

fn raid(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!(
            "{{card {me}}} has left the board · no Might, no Ganking"
        ));
        return done();
    }
    might_this_turn(ctx, item, me, MIGHT, None);
    grant_this_turn(ctx, me, Keyword::Ganking);
    ctx.narrate(format!("{{card {me}}} gets +{MIGHT} and Ganking this turn"));
    done()
}

pub static CARD: Card = unit(
    "Oasis Raider",
    &[],
    &[when(
        triggered(Trigger::BeginningPhase, &[], raid),
        outnumbered,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, Location};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, phases, priority};
    use crate::state::{ItemKind, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const RAIDER: u32 = 90;
    const THEIR_EXTRA_RUNES: [u32; 3] = [46, 47, 48];

    fn raider(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Fury".into()],
            ..fixtures::unit(RAIDER, zone, 0, "Oasis Raider", 4)
        }
    }

    fn dunes(their_extra_runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(raider(fixtures::BF1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        for rune in THEIR_EXTRA_RUNES.iter().take(their_extra_runes) {
            fixture
                .table
                .cards
                .push(fixtures::rune(*rune, 1, "Mind", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RAIDER).unwrap(),
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

    fn raider_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(
                |item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == RAIDER),
            )
            .count()
    }

    fn across(ctx: &Ctx) -> bool {
        march::legal_destination(
            ctx,
            RAIDER,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
        .is_ok()
    }

    #[test]
    fn the_script_is_the_baccais_conditional_beginning_phase_trigger_without_printed_ganking() {
        assert!(std::ptr::eq(script_of("Oasis Raider").unwrap(), &CARD));
        assert_eq!(CARD.name, "Oasis Raider");
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Ganking));
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some());
        assert!(!ability.optional);
        assert_eq!(MIGHT, 2);
    }

    #[test]
    fn outnumbered_he_gets_two_might_and_walks_battlefield_to_battlefield_this_turn() {
        let mut fixture = dunes(3);
        let mut ctx = fixture.ctx();
        assert!(
            !across(&ctx),
            "736 · no Ganking, no battlefield-to-battlefield move"
        );
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(raider_items(&ctx), 1);
        assert_eq!(ctx.current_might(RAIDER), 4, "nothing until it resolves");
        assert!(!ctx.has_keyword(RAIDER, Keyword::Ganking));
        resolve_the_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(RAIDER), 6);
        assert!(ctx.has_keyword(RAIDER, Keyword::Ganking));
        assert!(across(&ctx));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RAIDER}}} gets +2 and Ganking this turn")));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(RAIDER), 4, "this turn only");
        assert!(!ctx.has_keyword(RAIDER, Keyword::Ganking));
        assert!(!across(&ctx));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_more_runes_than_every_opponent_nothing_triggers() {
        let mut fixture = dunes(0);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(raider_items(&ctx), 0);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(RAIDER), 4);
        assert!(!ctx.has_keyword(RAIDER, Keyword::Ganking));
        assert!(!across(&ctx));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }
}
