use super::prelude::{buff, card_targets, done, play, target, unit, ANOTHER_FRIENDLY_UNIT_THAN_ME};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const UNITS: u8 = 2;

pub const OTHERS_TO_BUFF: TargetSpec = target(
    ANOTHER_FRIENDLY_UNIT_THAN_ME,
    0,
    UNITS,
    TargetKind::Card,
    "up to two other friendly units to buff",
);

fn bless(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for unit in card_targets(ctx, item) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
    done()
}

pub static CARD: Card = unit("Kinkou Monk", &[], &[play(&[OTHERS_TO_BUFF], bless)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{Filter, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as plays, prompts};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const MONK: u32 = 90;
    const ALLY: u32 = 91;

    fn monastery() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut monk = fixtures::unit(MONK, fixtures::HAND, 0, "Kinkou Monk", 4);
        monk.domain = vec!["Body".into()];
        monk.energy = Some(2);
        fixture.table.cards.push(monk);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.resolve();
        fixture
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    fn pending_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for units to buff, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_a_unit_whose_play_trigger_chooses_up_to_two_other_friendly_units() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Kinkou Monk").unwrap(),
            &CARD
        ));
        let fixture = monastery();
        assert!(std::ptr::eq(fixture.scripts.of_card(MONK).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets.len(), 1);
        let spec = ability.targets[0];
        assert_eq!(
            spec.filter,
            Filter::And(&[Filter::Unit, Filter::Friendly, Filter::NotSelf])
        );
        assert_eq!((spec.min, spec.max), (0, 2));
        assert_eq!(spec.kind, TargetKind::Card);
        assert_ne!(spec.filter, FRIENDLY_UNIT, "never the monk itself");
    }

    #[test]
    fn playing_the_monk_asks_for_up_to_two_others_and_buffs_both_picks() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        let item = pending_item(&ctx);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {MONK}}}: choose up to two other friendly units to buff (0 of 2)")
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                card_label(fixtures::VI),
                card_label(ALLY),
                "done".to_string(),
                "skip".to_string()
            ],
            "the two other friendly units; never the monk, never an enemy; a trigger has no cancel"
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(ALLY)).unwrap();
        assert!(ctx.blob.prompt.is_none(), "two is the most");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(ALLY)]
        );
        assert!(!ctx.is_buffed(fixtures::VI), "the buffs wait for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(ctx.is_buffed(ALLY));
        assert!(!ctx.is_buffed(MONK));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(ctx.current_might(ALLY), 3);
        assert_eq!(ctx.current_might(MONK), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is buffed", fixtures::VI)));
        assert!(ctx.blob.log.contains(&format!("{{card {ALLY}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn one_pick_then_done_buffs_only_that_unit_and_an_already_buffed_pick_gains_nothing() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [card_label(ALLY), "done".to_string()]
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "702.3 · one buff at a time"
        );
        assert!(!ctx.is_buffed(ALLY));
        assert!(
            !ctx.blob.log.iter().any(|line| line.contains("is buffed")),
            "the buff it already had is the only one"
        );
    }

    #[test]
    fn skipping_buffs_nobody_and_a_monk_with_no_other_friendly_unit_asks_nothing() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain[0].targets.is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(ALLY));
        drop(ctx);

        let mut fixture = monastery();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::VI && card.id != ALLY);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no candidate and a minimum of none: nothing to ask"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(MONK));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_cannot_answer_and_an_enemy_or_the_monk_itself_is_not_a_legal_target() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        let item = pending_item(&ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Jinx is the enemy's"
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, item, 0, &[MONK]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "other friendly units, not me"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt is still open");
        assert!(!ctx.is_buffed(fixtures::THEIR_UNIT));
    }

    #[test]
    fn a_pick_that_left_the_board_before_the_trigger_resolves_is_not_buffed() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(ALLY)).unwrap();
        assert!(ctx.bounce(ALLY));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(ALLY));
        assert_eq!(ctx.card(ALLY).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx.blob.log.contains(&format!("{{card {ALLY}}} is buffed")));
    }
}
