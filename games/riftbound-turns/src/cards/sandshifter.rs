use super::prelude::{a_card, card_target, done, kill, play, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT_LIMIT: u8 = 3;

pub const SMALL_ENEMY_UNIT: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::MightAtMost(MIGHT_LIMIT),
]);

pub const PREY: TargetSpec = a_card(
    SMALL_ENEMY_UNIT,
    "an enemy unit with 3 Might or less to kill",
);

fn shift(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        kill(ctx, item, unit);
    }
    done()
}

pub static CARD: Card = unit("Sandshifter", &[], &[play(&[PREY], shift)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SHIFTER: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const ORDER_RUNES: [u32; 3] = [46, 47, 48];

    fn shifter(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(2),
            domain: vec!["Order".into()],
            ..fixtures::unit(SHIFTER, zone, 0, "Sandshifter", 6)
        }
    }

    fn dunes() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shifter(fixtures::HAND));
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_one_small_enemy_unit() {
        assert!(std::ptr::eq(script_of("Sandshifter").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sandshifter");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[PREY]);
        assert_eq!((PREY.min, PREY.max), (1, 1));
        assert_eq!(PREY.kind, TargetKind::Card);
        assert_eq!(PREY.filter, SMALL_ENEMY_UNIT);
        assert_eq!(MIGHT_LIMIT, 3);
    }

    #[test]
    fn playing_it_offers_enemy_units_of_three_might_or_less_and_the_pick_dies_when_it_resolves() {
        let mut fixture = dunes();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHIFTER).unwrap();
        assert_eq!(ctx.location(SHIFTER), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT)
            ],
            "the 4 Might brute is too big, Vi is friendly"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {SHIFTER}}}: choose an enemy unit with 3 Might or less to kill (0 of 1)"
            )
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "4 Might is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SHIFTER
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "the kill waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(fixtures::THEIR_UNIT));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx.on_board(THEIR_BRUTE));
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pick_grown_past_three_might_before_resolution_survives() {
        let mut fixture = dunes();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHIFTER).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.might(fixtures::THEIR_UNIT, 2, until, None, 0);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "356.3.e · at 4 Might it no longer matches the spec"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
    }

    #[test]
    fn with_no_small_enemy_unit_the_trigger_fizzles_and_it_still_lands() {
        let mut fixture = dunes();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHIFTER).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "402.4 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SHIFTER}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(SHIFTER), Some(Location::Base(0)));
        assert!(ctx.on_board(THEIR_BRUTE));
    }
}
