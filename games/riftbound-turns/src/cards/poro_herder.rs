use super::prelude::{buff, done, draw, friendly_units, play, unit, when};
use super::{base_name, Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const DRAWS: usize = 1;
const PORO: &str = "Poro";

pub fn is_poro(ctx: &Ctx, card: u32) -> bool {
    ctx.is_unit(card)
        && ctx.card(card).is_some_and(|held| {
            base_name(&held.name)
                .split_whitespace()
                .last()
                .is_some_and(|word| word.contains(PORO))
        })
}

pub fn controls_a_poro(ctx: &Ctx, seat: u8) -> bool {
    friendly_units(ctx, seat)
        .into_iter()
        .any(|unit| is_poro(ctx, unit))
}

fn a_poro_on_entry(ctx: &Ctx, _: &Event, source: Source) -> bool {
    controls_a_poro(ctx, ctx.controller(source.card))
}

fn herd(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit(
    "Poro Herder",
    &[],
    &[when(play(&[], herd), a_poro_on_entry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::Trigger;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as plays, settle};
    use crate::state::{ItemKind, Origin};

    const HERDER: u32 = 90;
    const PORO_UNIT: u32 = 91;
    const SPARE_RUNE: u32 = 46;

    fn herder() -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::unit(HERDER, fixtures::HAND, 0, "Poro Herder", 3);
        card.domain = vec!["Calm".into()];
        card.energy = Some(3);
        card.power = Some(1);
        card
    }

    fn pasture(poro: Option<(u8, u16)>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(herder());
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_RUNE, 0, "Calm", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        if let Some((seat, zone)) = poro {
            fixture
                .table
                .cards
                .push(fixtures::unit(PORO_UNIT, zone, seat, "Pouty Poro", 1));
        }
        fixture.resolve();
        fixture
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_is_conditioned_on_a_friendly_poro() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Poro Herder").unwrap(),
            &CARD
        ));
        let fixture = pasture(None);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HERDER).unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(
            ability.condition.is_some(),
            "383.2.a.1 · the if is part of the condition"
        );
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn with_a_friendly_poro_on_the_board_the_herder_buffs_itself_and_draws_one() {
        let mut fixture = pasture(Some((0, fixtures::BASE)));
        let mut ctx = fixture.ctx();
        assert!(is_poro(&ctx, PORO_UNIT));
        assert!(!is_poro(&ctx, fixtures::VI));
        assert!(controls_a_poro(&ctx, 0));
        assert!(!controls_a_poro(&ctx, 1));
        fixtures::play_from_hand(&mut ctx, 0, HERDER).unwrap();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.on_board(HERDER));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HERDER
        ));
        assert!(ctx.blob.prompt.is_none(), "the herder asks nothing");
        assert_eq!(draws(&ctx), 0, "the draw waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(HERDER));
        assert_eq!(ctx.current_might(HERDER), 4);
        assert!(!ctx.is_buffed(PORO_UNIT), "the poro is only the reason");
        assert_eq!(draws(&ctx), 1);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {HERDER}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_poro_at_a_battlefield_counts_and_the_herder_itself_is_not_a_poro() {
        let mut fixture = pasture(Some((0, fixtures::BF1)));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let action = fixtures::move_action(HERDER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        plays::begin(&mut ctx, 0, HERDER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(HERDER));
        assert_eq!(draws(&ctx), 1);
        drop(ctx);

        let mut fixture = pasture(None);
        let ctx = fixture.ctx();
        assert!(
            !is_poro(&ctx, HERDER),
            "the Herder's name carries the word but not the tag"
        );
        let mut fixture = pasture(None);
        fixture.table.cards.push(fixtures::unit(
            PORO_UNIT,
            fixtures::BASE,
            0,
            "Patched Porobot",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(93, fixtures::BASE, 0, "Poro Snax", 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_poro(&ctx, PORO_UNIT), "the word inside the last word");
        assert!(!is_poro(&ctx, 93), "a gear is not a unit");
    }

    #[test]
    fn without_a_poro_or_with_only_the_enemys_the_herder_triggers_nothing() {
        let mut fixture = pasture(None);
        let mut ctx = fixture.ctx();
        assert!(!controls_a_poro(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, HERDER).unwrap();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.on_board(HERDER));
        assert!(ctx.blob.chain.is_empty(), "no poro, no trigger");
        assert!(!ctx.is_buffed(HERDER));
        assert_eq!(draws(&ctx), 0);
        assert_eq!(ctx.hand_of(0).len(), hand);
        drop(ctx);

        let mut fixture = pasture(Some((1, fixtures::BASE)));
        let mut ctx = fixture.ctx();
        assert!(controls_a_poro(&ctx, 1));
        assert!(!controls_a_poro(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, HERDER).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the enemy's poro is not one you control"
        );
        assert!(!ctx.is_buffed(HERDER));
        assert_eq!(draws(&ctx), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_poro_leaving_in_response_does_not_take_the_trigger_with_it() {
        let mut fixture = pasture(Some((0, fixtures::BASE)));
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HERDER).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.bounce(PORO_UNIT));
        assert!(!controls_a_poro(&ctx, 0));
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.is_buffed(HERDER),
            "383.2.a.1 · the condition was read when it triggered"
        );
        assert_eq!(draws(&ctx), 1);
    }

    #[test]
    #[ignore = "CardInfo carries no tags: is_poro reads the printed name, which every Poro so far carries; a Poro-tagged unit named otherwise needs a tag on the face"]
    fn a_poro_tagged_unit_whose_name_lacks_the_word_still_counts() {
        let mut fixture = pasture(None);
        fixture.table.cards.push(fixtures::unit(
            PORO_UNIT,
            fixtures::BASE,
            0,
            "Fluffy Friend",
            1,
        ));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(controls_a_poro(&ctx, 0));
    }
}
