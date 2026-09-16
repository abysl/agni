use super::prelude::{done, friendly_gear, play, ready, unit, when};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const GEAR_NEEDED: usize = 2;

pub fn controls_enough_gear(ctx: &Ctx, _: &Event, source: Source) -> bool {
    friendly_gear(ctx, ctx.controller(source.card)).len() >= GEAR_NEEDED
}

fn drop_in(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = unit(
    "Dropboarder",
    &[],
    &[when(play(&[], drop_in), controls_enough_gear)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const DROPBOARDER: u32 = 90;
    const GEAR: [u32; 2] = [91, 92];
    const THEIR_GEAR: u32 = 93;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 4;

    fn dropboarder(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Mind".into()],
            ..fixtures::unit(DROPBOARDER, zone, 0, "Dropboarder", MIGHT)
        }
    }

    fn workshop(mine: usize, theirs: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dropboarder(fixtures::HAND));
        for gear in GEAR.into_iter().take(mine) {
            fixture
                .table
                .cards
                .push(fixtures::gear(gear, fixtures::BASE, 0, "Gadget", 1));
        }
        if theirs > 0 {
            fixture
                .table
                .cards
                .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Gadget", 1));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DROPBOARDER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_it(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, DROPBOARDER).unwrap();
        assert_eq!(ctx.location(DROPBOARDER), Some(Location::Base(0)));
        assert!(
            ctx.card(DROPBOARDER).unwrap().exhausted,
            "a played unit enters exhausted"
        );
    }

    #[test]
    fn the_script_is_a_unit_with_one_gated_play_trigger() {
        assert!(std::ptr::eq(script_of("Dropboarder").unwrap(), &CARD));
        assert_eq!(CARD.name, "Dropboarder");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(
            ability.condition.is_some(),
            "383.2.a.1 · the if right after the when is part of the condition"
        );
    }

    #[test]
    fn with_two_gear_the_trigger_readies_it_when_it_resolves() {
        let mut fixture = workshop(2, 0);
        let mut ctx = fixture.ctx();
        play_it(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DROPBOARDER
        ));
        assert!(ctx.blob.prompt.is_none());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(DROPBOARDER).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == DROPBOARDER
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DROPBOARDER}}} readies")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_one_gear_nothing_triggers_and_it_stays_exhausted() {
        let mut fixture = workshop(1, 0);
        let mut ctx = fixture.ctx();
        play_it(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "one gear is not two");
        assert!(ctx.card(DROPBOARDER).unwrap().exhausted);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, .. } if *card == DROPBOARDER)));
    }

    #[test]
    fn an_opponents_gear_does_not_count_and_a_gold_token_does() {
        let mut fixture = workshop(1, 1);
        let mut ctx = fixture.ctx();
        play_it(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "their gear is not yours");
        assert!(ctx.card(DROPBOARDER).unwrap().exhausted);
        drop(ctx);
        let mut fixture = workshop(1, 0);
        fixture.table.cards.push(fixtures::gold(94, 0, true));
        fixture.table.tokens.push(94);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_it(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1, "a Gold token is gear you control");
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(DROPBOARDER).unwrap().exhausted);
    }

    #[test]
    fn a_dropboarder_readied_in_response_readies_nothing_again() {
        let mut fixture = workshop(2, 0);
        let mut ctx = fixture.ctx();
        play_it(&mut ctx);
        assert!(ctx.ready(DROPBOARDER));
        let readied = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::Readied { card, .. } if *card == DROPBOARDER))
            .count();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(DROPBOARDER).unwrap().exhausted);
        assert_eq!(
            ctx.events
                .iter()
                .filter(
                    |event| matches!(event, Event::Readied { card, .. } if *card == DROPBOARDER)
                )
                .count(),
            readied,
            "already ready: nothing to ready"
        );
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {DROPBOARDER}}} readies")));
    }
}
